// Operaciones del MCP sobre el estado de Foco (el mismo JSON que usa la
// interfaz). Se ejecutan dentro de la app si está abierta, o directamente
// sobre el archivo si no lo está. Ver `mcp.rs`.
//
// Modelo de notas:
// - `internalNotes`: notas internas (las que añade Claude u otra herramienta).
//   Van en su propio apartado dentro de la tarea.
// - `notes`: las notas comunes que escribes en la app. Se tratan como tu
//   RESPUESTA a las notas internas.
use std::{
    sync::atomic::{AtomicU32, Ordering::Relaxed},
    time::{SystemTime, UNIX_EPOCH},
};

use serde_json::{json, Value};

/// Colores de las listas (mismos que `COLORS` en `src/store.ts`).
const COLORS: [&str; 8] = [
    "#0a84ff", "#ff9f0a", "#30d158", "#bf5af2", "#ff375f", "#64d2ff", "#ffd60a", "#8e8e93",
];

pub struct Outcome {
    pub result: Value,
    /// true si la operación modificó el estado (hay que guardarlo).
    pub changed: bool,
}

type R = Result<Outcome, String>;

fn read(result: Value) -> R {
    Ok(Outcome {
        result,
        changed: false,
    })
}
fn wrote(result: Value) -> R {
    Ok(Outcome {
        result,
        changed: true,
    })
}

pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}

/// Identificador corto, como `uid()` en `src/store.ts`.
fn uid() -> String {
    static SEQ: AtomicU32 = AtomicU32::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.subsec_nanos());
    let mix = nanos
        ^ std::process::id().rotate_left(16)
        ^ SEQ.fetch_add(1, Relaxed).wrapping_mul(2_654_435_761);
    // Los últimos dígitos son los que más varían.
    let r = base36(u64::from(mix) | 1 << 40);
    format!("{}{}", base36(now_ms() as u64), &r[r.len() - 6..])
}

fn base36(mut n: u64) -> String {
    const D: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz";
    let mut out = Vec::new();
    loop {
        out.push(D[(n % 36) as usize]);
        n /= 36;
        if n == 0 {
            break;
        }
    }
    out.reverse();
    String::from_utf8(out).unwrap()
}

/// Estado mínimo válido (la app completa el resto al abrirlo).
pub fn empty_state() -> Value {
    let id = uid();
    json!({
        "v": 1,
        "rev": 0,
        "lists": [{ "id": id, "name": "Personal", "color": COLORS[0] }],
        "tasks": [],
        "selectedListId": id,
        "active": { "taskId": null, "since": null },
        "settings": {}
    })
}

/// Asegura la forma básica del estado, sea cual sea el archivo de origen.
pub fn ensure_shape(state: &mut Value) {
    if !state.is_object() || !state["lists"].is_array() || !state["tasks"].is_array() {
        *state = empty_state();
    }
    if !state["active"].is_object() {
        state["active"] = json!({ "taskId": null, "since": null });
    }
}

pub fn rev(state: &Value) -> i64 {
    state["rev"].as_i64().unwrap_or(-1)
}

pub fn bump_rev(state: &mut Value) {
    state["rev"] = json!(rev(state).max(0) + 1);
}

// ───────────────────────── Herramientas ─────────────────────────

pub fn call(state: &mut Value, tool: &str, args: &Value) -> R {
    ensure_shape(state);
    match tool {
        "listar_listas" => listar_listas(state),
        "crear_lista" => crear_lista(state, args),
        "listar_tareas" => listar_tareas(state, args),
        "ver_tarea" => ver_tarea(state, args),
        "crear_tarea" => crear_tarea(state, args),
        "asignar_codigo" => asignar_codigo(state, args),
        "marcar_concluida" => marcar_concluida(state, args),
        "agregar_nota_interna" => agregar_nota_interna(state, args),
        _ => Err(format!("Herramienta desconocida: {tool}")),
    }
}

fn str_arg<'a>(args: &'a Value, key: &str) -> Option<&'a str> {
    args[key].as_str().map(str::trim).filter(|s| !s.is_empty())
}

fn req_arg<'a>(args: &'a Value, key: &str) -> Result<&'a str, String> {
    str_arg(args, key).ok_or_else(|| format!("Falta el parámetro «{key}»."))
}

fn lists(state: &Value) -> &Vec<Value> {
    state["lists"].as_array().expect("ensure_shape")
}
fn tasks(state: &Value) -> &Vec<Value> {
    state["tasks"].as_array().expect("ensure_shape")
}
fn tasks_mut(state: &mut Value) -> &mut Vec<Value> {
    state["tasks"].as_array_mut().expect("ensure_shape")
}

/// Busca una lista por id o por nombre (sin distinguir mayúsculas).
fn find_list(state: &Value, key: &str) -> Result<usize, String> {
    let ls = lists(state);
    if let Some(i) = ls.iter().position(|l| l["id"].as_str() == Some(key)) {
        return Ok(i);
    }
    let low = key.to_lowercase();
    let found: Vec<usize> = ls
        .iter()
        .enumerate()
        .filter(|(_, l)| {
            l["name"]
                .as_str()
                .is_some_and(|n| n.trim().to_lowercase() == low)
        })
        .map(|(i, _)| i)
        .collect();
    match found[..] {
        [i] => Ok(i),
        [] => Err(format!(
            "No existe la lista «{key}». Usa listar_listas para ver las disponibles."
        )),
        _ => Err(format!("Hay varias listas llamadas «{key}»; usa su id.")),
    }
}

/// Busca una tarea por id o por código interno (sin distinguir mayúsculas).
fn find_task(state: &Value, key: &str) -> Result<usize, String> {
    let ts = tasks(state);
    if let Some(i) = ts.iter().position(|t| t["id"].as_str() == Some(key)) {
        return Ok(i);
    }
    let low = key.to_lowercase();
    let found: Vec<usize> = ts
        .iter()
        .enumerate()
        .filter(|(_, t)| {
            t["code"]
                .as_str()
                .is_some_and(|c| !c.is_empty() && c.trim().to_lowercase() == low)
        })
        .map(|(i, _)| i)
        .collect();
    match found[..] {
        [i] => Ok(i),
        [] => Err(format!("No existe ninguna tarea con id o código «{key}».")),
        _ => Err(format!(
            "Hay varias tareas con el código «{key}»; usa su id."
        )),
    }
}

fn list_name(state: &Value, list_id: &str) -> String {
    lists(state)
        .iter()
        .find(|l| l["id"].as_str() == Some(list_id))
        .and_then(|l| l["name"].as_str())
        .unwrap_or("")
        .to_string()
}

fn fmt_ms(ms: i64) -> String {
    let s = ms.max(0) / 1000;
    let (h, m, s) = (s / 3600, (s % 3600) / 60, s % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m:02}:{s:02}")
    }
}

/// Tiempo total en foco, incluido el tramo en curso.
fn elapsed(state: &Value, t: &Value) -> i64 {
    let spent = t["spentMs"].as_i64().unwrap_or(0);
    let running = state["active"]["taskId"] == t["id"];
    let live = match state["active"]["since"].as_i64() {
        Some(since) if running => (now_ms() - since).max(0),
        _ => 0,
    };
    spent + live
}

fn internal_notes(t: &Value) -> Vec<Value> {
    t["internalNotes"].as_array().cloned().unwrap_or_default()
}

/// Resumen de una tarea para las respuestas del MCP.
fn task_view(state: &Value, t: &Value, full: bool) -> Value {
    let list_id = t["listId"].as_str().unwrap_or("");
    let done = t["done"].as_bool().unwrap_or(false);
    let mut v = json!({
        "id": t["id"],
        "codigo": t["code"].as_str().unwrap_or(""),
        "titulo": t["title"].as_str().unwrap_or(""),
        "lista": list_name(state, list_id),
        "lista_id": list_id,
        "concluida": done,
        "concluida_en": if done { iso(t["doneAt"].as_i64()) } else { Value::Null },
        "en_foco": state["active"]["taskId"] == t["id"],
        "tiempo_en_foco": fmt_ms(elapsed(state, t)),
    });
    let notes = internal_notes(t);
    let reply = t["notes"].as_str().unwrap_or("");
    if full {
        v["notas_internas"] = Value::Array(
            notes
                .iter()
                .map(|n| {
                    json!({
                        "id": n["id"],
                        "texto": n["text"],
                        "autor": n["author"],
                        "fecha": iso(n["at"].as_i64()),
                    })
                })
                .collect(),
        );
        v["respuesta"] = json!(reply);
    } else {
        v["notas_internas"] = json!(notes.len());
        v["tiene_respuesta"] = json!(!reply.trim().is_empty());
    }
    v
}

/// Fecha UTC en ISO 8601 (sin dependencias externas).
fn iso(ms: Option<i64>) -> Value {
    let Some(ms) = ms else { return Value::Null };
    let secs = ms.div_euclid(1000);
    let (days, rem) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    // Algoritmo de Howard Hinnant (días desde 1970 → fecha civil).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    json!(format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        rem / 3600,
        (rem % 3600) / 60,
        rem % 60
    ))
}

fn listar_listas(state: &Value) -> R {
    let out: Vec<Value> = lists(state)
        .iter()
        .map(|l| {
            let id = l["id"].as_str().unwrap_or("");
            let of_list = || {
                tasks(state)
                    .iter()
                    .filter(move |t| t["listId"].as_str() == Some(id))
            };
            json!({
                "id": id,
                "nombre": l["name"],
                "color": l["color"],
                "pendientes": of_list().filter(|t| t["done"] != json!(true)).count(),
                "concluidas": of_list().filter(|t| t["done"] == json!(true)).count(),
            })
        })
        .collect();
    read(json!(out))
}

fn crear_lista(state: &mut Value, args: &Value) -> R {
    let name = req_arg(args, "nombre")?.to_string();
    let n = lists(state).len();
    let color = match str_arg(args, "color") {
        Some(c)
            if c.starts_with('#')
                && (4..=9).contains(&c.len())
                && c[1..].chars().all(|ch| ch.is_ascii_hexdigit()) =>
        {
            c.to_string()
        }
        Some(c) => return Err(format!("Color «{c}» no válido; usa un hex como #30d158.")),
        None => COLORS[n % COLORS.len()].to_string(),
    };
    let list = json!({ "id": uid(), "name": name, "color": color });
    state["lists"].as_array_mut().unwrap().push(list.clone());
    wrote(json!({ "id": list["id"], "nombre": list["name"], "color": list["color"] }))
}

fn listar_tareas(state: &Value, args: &Value) -> R {
    let list_id = match str_arg(args, "lista") {
        Some(k) => Some(
            lists(state)[find_list(state, k)?]["id"]
                .as_str()
                .unwrap_or("")
                .to_string(),
        ),
        None => None,
    };
    let estado = str_arg(args, "estado").unwrap_or("todas");
    let want_done = match estado {
        "pendientes" => Some(false),
        "concluidas" => Some(true),
        "todas" => None,
        other => {
            return Err(format!(
                "Estado «{other}» no válido: usa pendientes, concluidas o todas."
            ))
        }
    };
    let list_pos = |id: &str| {
        lists(state)
            .iter()
            .position(|l| l["id"].as_str() == Some(id))
            .unwrap_or(usize::MAX)
    };
    let mut ts: Vec<&Value> = tasks(state)
        .iter()
        .filter(|t| {
            list_id
                .as_deref()
                .map_or(true, |id| t["listId"].as_str() == Some(id))
        })
        .filter(|t| want_done.map_or(true, |d| t["done"].as_bool().unwrap_or(false) == d))
        .collect();
    ts.sort_by(|a, b| {
        let key = |t: &Value| {
            (
                list_pos(t["listId"].as_str().unwrap_or("")),
                t["order"].as_f64().unwrap_or(0.0),
            )
        };
        let (ka, kb) = (key(a), key(b));
        ka.0.cmp(&kb.0).then(ka.1.total_cmp(&kb.1))
    });
    read(json!(ts
        .iter()
        .map(|t| task_view(state, t, false))
        .collect::<Vec<_>>()))
}

fn ver_tarea(state: &Value, args: &Value) -> R {
    let i = find_task(state, req_arg(args, "tarea")?)?;
    read(task_view(state, &tasks(state)[i], true))
}

fn check_code_free(state: &Value, code: &str, except: Option<&str>) -> Result<(), String> {
    let low = code.to_lowercase();
    let taken = tasks(state).iter().any(|t| {
        t["id"].as_str() != except
            && t["code"]
                .as_str()
                .is_some_and(|c| c.trim().to_lowercase() == low)
    });
    if taken {
        Err(format!("El código «{code}» ya lo usa otra tarea."))
    } else {
        Ok(())
    }
}

fn crear_tarea(state: &mut Value, args: &Value) -> R {
    let title = req_arg(args, "titulo")?.to_string();
    let list_idx = match str_arg(args, "lista") {
        Some(k) => find_list(state, k)?,
        None => {
            // La lista seleccionada en la app, o la primera.
            let sel = state["selectedListId"].as_str().unwrap_or("").to_string();
            lists(state)
                .iter()
                .position(|l| l["id"].as_str() == Some(&sel))
                .unwrap_or(0)
        }
    };
    let list_id = lists(state)
        .get(list_idx)
        .and_then(|l| l["id"].as_str())
        .ok_or("No hay ninguna lista; crea una con crear_lista.")?
        .to_string();
    let code = str_arg(args, "codigo").unwrap_or("").to_string();
    if !code.is_empty() {
        check_code_free(state, &code, None)?;
    }
    let order = tasks(state)
        .iter()
        .filter(|t| t["listId"].as_str() == Some(&list_id))
        .filter_map(|t| t["order"].as_f64())
        .fold(-1.0, f64::max)
        + 1.0;
    let now = now_ms();
    let mut notes = Vec::new();
    if let Some(text) = str_arg(args, "nota_interna") {
        notes.push(new_note(text, args));
    }
    let task = json!({
        "id": uid(),
        "listId": list_id,
        "title": title,
        "code": code,
        "notes": "",
        "internalNotes": notes,
        "done": false,
        "createdAt": now,
        "doneAt": null,
        "spentMs": 0,
        "order": order,
    });
    tasks_mut(state).push(task.clone());
    let view = task_view(state, &task, true);
    wrote(view)
}

fn asignar_codigo(state: &mut Value, args: &Value) -> R {
    let i = find_task(state, req_arg(args, "tarea")?)?;
    let code = args["codigo"].as_str().unwrap_or("").trim().to_string();
    let id = tasks(state)[i]["id"].as_str().unwrap_or("").to_string();
    if !code.is_empty() {
        check_code_free(state, &code, Some(&id))?;
    }
    tasks_mut(state)[i]["code"] = json!(code);
    let view = task_view(state, &tasks(state)[i], false);
    wrote(view)
}

fn new_note(text: &str, args: &Value) -> Value {
    json!({
        "id": uid(),
        "text": text,
        "author": str_arg(args, "autor").unwrap_or("Claude"),
        "at": now_ms(),
    })
}

fn agregar_nota_interna(state: &mut Value, args: &Value) -> R {
    let i = find_task(state, req_arg(args, "tarea")?)?;
    let note = new_note(req_arg(args, "texto")?, args);
    let t = &mut tasks_mut(state)[i];
    if !t["internalNotes"].is_array() {
        t["internalNotes"] = json!([]);
    }
    t["internalNotes"].as_array_mut().unwrap().push(note);
    let view = task_view(state, &tasks(state)[i], true);
    wrote(view)
}

/// Igual que `complete()` en `src/store.ts`: si la tarea estaba en foco, se
/// pausa su cronómetro y el foco pasa a la siguiente pendiente de la lista.
fn marcar_concluida(state: &mut Value, args: &Value) -> R {
    let i = find_task(state, req_arg(args, "tarea")?)?;
    let done = args["concluida"].as_bool().unwrap_or(true);
    let id = tasks(state)[i]["id"].clone();
    let was_done = tasks(state)[i]["done"].as_bool().unwrap_or(false);
    if done == was_done {
        let view = task_view(state, &tasks(state)[i], false);
        return read(view);
    }
    if !done {
        let t = &mut tasks_mut(state)[i];
        t["done"] = json!(false);
        t["doneAt"] = Value::Null;
    } else {
        let was_active = state["active"]["taskId"] == id;
        let since = state["active"]["since"].as_i64();
        let now = now_ms();
        if was_active {
            if let Some(since) = since {
                let spent = tasks(state)[i]["spentMs"].as_i64().unwrap_or(0);
                tasks_mut(state)[i]["spentMs"] = json!(spent + (now - since).max(0));
            }
        }
        {
            let t = &mut tasks_mut(state)[i];
            t["done"] = json!(true);
            t["doneAt"] = json!(now);
        }
        if was_active {
            let next = next_pending(state, i);
            let running = since.is_some() && next.is_some();
            state["active"] = json!({
                "taskId": next,
                "since": if running { json!(now) } else { Value::Null },
            });
        }
    }
    let view = task_view(state, &tasks(state)[i], false);
    wrote(view)
}

/// Siguiente tarea pendiente de la misma lista (circular), como `nextAfter`.
fn next_pending(state: &Value, i: usize) -> Option<String> {
    let t = &tasks(state)[i];
    let list = t["listId"].as_str()?;
    let order = t["order"].as_f64().unwrap_or(0.0);
    let mut rest: Vec<&Value> = tasks(state)
        .iter()
        .filter(|x| {
            x["listId"].as_str() == Some(list) && x["done"] != json!(true) && x["id"] != t["id"]
        })
        .collect();
    rest.sort_by(|a, b| {
        a["order"]
            .as_f64()
            .unwrap_or(0.0)
            .total_cmp(&b["order"].as_f64().unwrap_or(0.0))
    });
    rest.iter()
        .find(|x| x["order"].as_f64().unwrap_or(0.0) > order)
        .or(rest.first())
        .and_then(|x| x["id"].as_str())
        .map(String::from)
}

// ───────────────────────── Esquema de herramientas ─────────────────────────

/// Definiciones para `tools/list` del MCP.
pub fn tool_defs() -> Value {
    let tarea = json!({ "type": "string", "description": "Id de la tarea o su código interno (p. ej. PRJ-142)." });
    let lista = json!({ "type": "string", "description": "Id o nombre de la lista." });
    let ro = json!({ "readOnlyHint": true, "openWorldHint": false });
    let rw = json!({ "readOnlyHint": false, "destructiveHint": false, "openWorldHint": false });
    json!([
        {
            "name": "listar_listas",
            "description": "Lista las listas de Foco con sus tareas pendientes y concluidas.",
            "inputSchema": { "type": "object", "properties": {} },
            "annotations": ro,
        },
        {
            "name": "crear_lista",
            "description": "Crea una lista nueva de tareas.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "nombre": { "type": "string", "description": "Nombre de la lista." },
                    "color": { "type": "string", "description": "Color hex opcional (p. ej. #30d158). Si se omite, se elige uno." }
                },
                "required": ["nombre"]
            },
            "annotations": rw,
        },
        {
            "name": "listar_tareas",
            "description": "Lista tareas con su estado (concluida o pendiente), código, tiempo en foco, número de notas internas y si el usuario ha respondido.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "lista": lista,
                    "estado": { "type": "string", "enum": ["pendientes", "concluidas", "todas"], "description": "Filtro por estado. Por defecto, todas." }
                }
            },
            "annotations": ro,
        },
        {
            "name": "ver_tarea",
            "description": "Detalle de una tarea: estado de conclusión, notas internas y la respuesta del usuario (sus notas comunes, que responden a las notas internas).",
            "inputSchema": { "type": "object", "properties": { "tarea": tarea }, "required": ["tarea"] },
            "annotations": ro,
        },
        {
            "name": "crear_tarea",
            "description": "Crea una tarea pendiente. Opcionalmente le asigna un código interno y una primera nota interna.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "titulo": { "type": "string", "description": "Título de la tarea." },
                    "lista": { "type": "string", "description": "Id o nombre de la lista. Por defecto, la lista seleccionada en la app." },
                    "codigo": { "type": "string", "description": "Código interno único (p. ej. PRJ-142)." },
                    "nota_interna": { "type": "string", "description": "Primera nota interna de la tarea." }
                },
                "required": ["titulo"]
            },
            "annotations": rw,
        },
        {
            "name": "asignar_codigo",
            "description": "Asigna o cambia el código interno de una tarea. Una cadena vacía lo borra. Los códigos son únicos.",
            "inputSchema": {
                "type": "object",
                "properties": { "tarea": tarea, "codigo": { "type": "string", "description": "Nuevo código interno." } },
                "required": ["tarea", "codigo"]
            },
            "annotations": rw,
        },
        {
            "name": "marcar_concluida",
            "description": "Marca una tarea como concluida (o de nuevo pendiente con concluida=false). Si estaba en foco, el foco pasa a la siguiente pendiente.",
            "inputSchema": {
                "type": "object",
                "properties": { "tarea": tarea, "concluida": { "type": "boolean", "description": "true = concluida (por defecto), false = pendiente." } },
                "required": ["tarea"]
            },
            "annotations": rw,
        },
        {
            "name": "agregar_nota_interna",
            "description": "Añade una nota interna a una tarea. Aparece en el apartado «Notas internas»; el usuario responde en sus notas comunes, que ver_tarea devuelve como «respuesta».",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "tarea": tarea,
                    "texto": { "type": "string", "description": "Texto de la nota interna." },
                    "autor": { "type": "string", "description": "Quién la escribe. Por defecto, Claude." }
                },
                "required": ["tarea", "texto"]
            },
            "annotations": rw,
        },
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &mut Value, tool: &str, args: Value) -> Value {
        call(s, tool, &args)
            .unwrap_or_else(|e| panic!("{tool}: {e}"))
            .result
    }

    #[test]
    fn flujo_completo() {
        let mut s = empty_state();
        let l = run(&mut s, "crear_lista", json!({ "nombre": "Trabajo" }));
        assert_eq!(l["nombre"], "Trabajo");
        let t = run(
            &mut s,
            "crear_tarea",
            json!({ "titulo": "Informe", "lista": "trabajo", "codigo": "PRJ-1", "nota_interna": "Revisar cifras" }),
        );
        assert_eq!(t["codigo"], "PRJ-1");
        assert_eq!(t["notas_internas"][0]["texto"], "Revisar cifras");
        assert_eq!(t["notas_internas"][0]["autor"], "Claude");
        // Código duplicado
        assert!(call(
            &mut s,
            "crear_tarea",
            &json!({ "titulo": "Otra", "codigo": "prj-1" })
        )
        .is_err());
        run(
            &mut s,
            "asignar_codigo",
            json!({ "tarea": "PRJ-1", "codigo": "PRJ-2" }),
        );
        run(
            &mut s,
            "agregar_nota_interna",
            json!({ "tarea": "prj-2", "texto": "¿Fecha?" }),
        );
        let v = run(&mut s, "ver_tarea", json!({ "tarea": "PRJ-2" }));
        assert_eq!(v["notas_internas"].as_array().unwrap().len(), 2);
        assert_eq!(v["respuesta"], "");
        assert_eq!(v["concluida"], false);
        let c = run(&mut s, "marcar_concluida", json!({ "tarea": "PRJ-2" }));
        assert_eq!(c["concluida"], true);
        assert!(c["concluida_en"].as_str().unwrap().ends_with('Z'));
        let pend = run(&mut s, "listar_tareas", json!({ "estado": "pendientes" }));
        assert_eq!(pend.as_array().unwrap().len(), 0);
        let hechas = run(
            &mut s,
            "listar_tareas",
            json!({ "lista": "Trabajo", "estado": "concluidas" }),
        );
        assert_eq!(hechas[0]["codigo"], "PRJ-2");
        let ls = run(&mut s, "listar_listas", json!({}));
        assert_eq!(ls[1]["concluidas"], 1);
    }

    #[test]
    fn concluir_tarea_en_foco_pasa_a_la_siguiente() {
        let mut s = empty_state();
        let a = run(&mut s, "crear_tarea", json!({ "titulo": "A" }));
        let b = run(&mut s, "crear_tarea", json!({ "titulo": "B" }));
        s["active"] = json!({ "taskId": a["id"], "since": now_ms() - 5_000 });
        run(&mut s, "marcar_concluida", json!({ "tarea": a["id"] }));
        assert_eq!(s["active"]["taskId"], b["id"]);
        assert!(s["active"]["since"].is_i64());
        assert!(s["tasks"][0]["spentMs"].as_i64().unwrap() >= 5_000);
    }

    #[test]
    fn fecha_iso() {
        assert_eq!(iso(Some(0)), json!("1970-01-01T00:00:00Z"));
        assert_eq!(iso(Some(1_763_251_200_000)), json!("2025-11-16T00:00:00Z"));
    }
}
