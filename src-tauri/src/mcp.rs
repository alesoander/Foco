// Servidor MCP de Foco: `foco --mcp`.
//
// Claude lanza este proceso y habla con él por stdin/stdout (JSON-RPC, un
// mensaje por línea). Cada herramienta se ejecuta así:
// - Si la app está abierta, se le envía por un socket local y es ella quien
//   modifica el estado, lo guarda y avisa a sus ventanas. Así nunca hay dos
//   procesos escribiendo el mismo archivo.
// - Si no está abierta, se modifica directamente el archivo de estado.
use std::{
    env, fs,
    io::{self, BufRead, Write},
    path::{Path, PathBuf},
};

use serde_json::{json, Value};

use crate::ops;

const IDENTIFIER: &str = "io.github.foco";
const PROTOCOLS: &[&str] = &["2025-06-18", "2025-03-26", "2024-11-05"];

const INSTRUCTIONS: &str = "Foco es el organizador de tareas del usuario. \
Las tareas se pueden buscar por id o por su código interno. \
Las notas internas son tuyas (van en su propio apartado de la tarea); las notas \
comunes del usuario son su RESPUESTA a esas notas internas y ver_tarea las \
devuelve en el campo «respuesta». Usa listar_tareas o ver_tarea para conocer si \
una tarea está concluida.";

/// Misma ruta que `app_data_dir()` de Tauri.
pub fn state_path() -> Option<PathBuf> {
    let base = if cfg!(target_os = "windows") {
        env::var_os("APPDATA").map(PathBuf::from)
    } else if cfg!(target_os = "macos") {
        env::var_os("HOME").map(|h| PathBuf::from(h).join("Library/Application Support"))
    } else {
        env::var_os("XDG_DATA_HOME")
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .or_else(|| env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
    }?;
    Some(base.join(IDENTIFIER).join("state.json"))
}

/// Socket por el que la app abierta recibe las operaciones del MCP.
pub fn socket_path() -> Option<PathBuf> {
    match env::var_os("XDG_RUNTIME_DIR").filter(|v| !v.is_empty()) {
        Some(dir) => Some(PathBuf::from(dir).join("foco-mcp.sock")),
        None => state_path().map(|p| p.with_file_name("mcp.sock")),
    }
}

pub fn write_atomic(path: &Path, data: &str) -> io::Result<()> {
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, data)?;
    fs::rename(&tmp, path)
}

/// Aplica una operación sobre un estado en texto. Devuelve el nuevo texto si cambió.
pub fn apply(json: &str, tool: &str, args: &Value) -> (Result<Value, String>, Option<String>) {
    let mut state: Value = serde_json::from_str(json).unwrap_or(Value::Null);
    match ops::call(&mut state, tool, args) {
        Ok(out) if out.changed => {
            ops::bump_rev(&mut state);
            (Ok(out.result), Some(state.to_string()))
        }
        Ok(out) => (Ok(out.result), None),
        Err(e) => (Err(e), None),
    }
}

// ───────────────────────── Ejecutar una herramienta ─────────────────────────

fn run_tool(tool: &str, args: &Value) -> Result<Value, String> {
    #[cfg(unix)]
    if let Some(res) = via_app(tool, args) {
        return res;
    }
    // La app no está abierta: se edita el archivo.
    let path = state_path().ok_or("No se encuentra la carpeta de datos de Foco.")?;
    let json = fs::read_to_string(&path).unwrap_or_default();
    let (res, new) = apply(&json, tool, args);
    if let Some(new) = new {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        write_atomic(&path, &new).map_err(|e| format!("No se pudo guardar: {e}"))?;
    }
    res
}

/// `None` si la app no está abierta (no hay nadie escuchando en el socket).
#[cfg(unix)]
fn via_app(tool: &str, args: &Value) -> Option<Result<Value, String>> {
    use std::{io::BufReader, os::unix::net::UnixStream, time::Duration};
    let mut s = UnixStream::connect(socket_path()?).ok()?;
    let _ = s.set_read_timeout(Some(Duration::from_secs(10)));
    let req = json!({ "tool": tool, "args": args }).to_string() + "\n";
    if let Err(e) = s.write_all(req.as_bytes()) {
        return Some(Err(format!("No se pudo hablar con Foco: {e}")));
    }
    let mut line = String::new();
    if let Err(e) = BufReader::new(s).read_line(&mut line) {
        return Some(Err(format!("Foco no respondió: {e}")));
    }
    let resp: Value = match serde_json::from_str(&line) {
        Ok(v) => v,
        Err(e) => return Some(Err(format!("Respuesta no válida de Foco: {e}"))),
    };
    Some(if resp["ok"] == json!(true) {
        Ok(resp["result"].clone())
    } else {
        Err(resp["error"]
            .as_str()
            .unwrap_or("Error desconocido")
            .to_string())
    })
}

/// Responde a una petición del socket (lado de la app). `state` es el JSON
/// actual; devuelve la respuesta y, si cambió, el nuevo estado.
pub fn handle_socket_request(line: &str, state: &str) -> (String, Option<String>) {
    let req: Value = serde_json::from_str(line).unwrap_or(Value::Null);
    let tool = req["tool"].as_str().unwrap_or("");
    let (res, new) = apply(state, tool, &req["args"]);
    let resp = match res {
        Ok(result) => json!({ "ok": true, "result": result }),
        Err(error) => json!({ "ok": false, "error": error }),
    };
    (resp.to_string() + "\n", new)
}

// ───────────────────────── Protocolo MCP (stdio) ─────────────────────────

pub fn run() {
    let stdin = io::stdin();
    let mut stdout = io::stdout().lock();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        let reply = match serde_json::from_str::<Value>(&line) {
            Ok(Value::Array(batch)) => {
                let out: Vec<Value> = batch.iter().filter_map(handle).collect();
                (!out.is_empty()).then_some(Value::Array(out))
            }
            Ok(msg) => handle(&msg),
            Err(e) => Some(error(Value::Null, -32700, &format!("JSON no válido: {e}"))),
        };
        if let Some(reply) = reply {
            if writeln!(stdout, "{reply}")
                .and_then(|_| stdout.flush())
                .is_err()
            {
                break;
            }
        }
    }
}

fn error(id: Value, code: i64, msg: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": msg } })
}

/// Devuelve `None` para las notificaciones (mensajes sin id).
fn handle(msg: &Value) -> Option<Value> {
    let id = msg.get("id")?.clone();
    let params = &msg["params"];
    let result = match msg["method"].as_str().unwrap_or("") {
        "initialize" => {
            let asked = params["protocolVersion"].as_str().unwrap_or("");
            let version = if PROTOCOLS.contains(&asked) {
                asked
            } else {
                PROTOCOLS[0]
            };
            json!({
                "protocolVersion": version,
                "capabilities": { "tools": { "listChanged": false } },
                "serverInfo": { "name": "foco", "title": "Foco", "version": env!("CARGO_PKG_VERSION") },
                "instructions": INSTRUCTIONS,
            })
        }
        "ping" => json!({}),
        "tools/list" => json!({ "tools": ops::tool_defs() }),
        "tools/call" => {
            let name = params["name"].as_str().unwrap_or("");
            let args = if params["arguments"].is_object() {
                params["arguments"].clone()
            } else {
                json!({})
            };
            match run_tool(name, &args) {
                Ok(v) => json!({
                    "content": [{ "type": "text", "text": serde_json::to_string_pretty(&v).unwrap_or_default() }],
                    "isError": false,
                }),
                Err(e) => json!({ "content": [{ "type": "text", "text": e }], "isError": true }),
            }
        }
        "resources/list" => json!({ "resources": [] }),
        "prompts/list" => json!({ "prompts": [] }),
        other => return Some(error(id, -32601, &format!("Método no soportado: {other}"))),
    };
    Some(json!({ "jsonrpc": "2.0", "id": id, "result": result }))
}
