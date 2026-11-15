// ¿Sigue la ventana principal a la vista aunque haya perdido el foco?
//
// Con dos monitores (o con Foco en mosaico junto a otra ventana) puedes
// estar trabajando en otra ventana mientras Foco sigue visible. En ese caso
// el widget sobra y no se muestra.
//
// - Hyprland: se pregunta al compositor por su socket de IPC y se escuchan
//   sus eventos (cambio de escritorio, de monitor, pantalla completa…).
// - Sway: igual, mediante `swaymsg`.
// - Resto (X11, Windows): no hay forma fiable de saber si una ventana está
//   tapada, así que se usa una aproximación: si el puntero está en OTRO
//   monitor distinto del de Foco, Foco sigue a la vista.
use std::{
    env,
    io::{BufRead, BufReader},
    thread,
};

use serde_json::Value;
use tauri::{AppHandle, Manager};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Backend {
    Hyprland,
    Sway,
    /// X11, Windows… Se comprueba el monitor del puntero.
    Pointer,
    /// Wayland sin IPC conocido (GNOME, KDE…): no se puede saber.
    Unknown,
}

pub fn backend() -> Backend {
    let set = |k: &str| env::var_os(k).is_some_and(|v| !v.is_empty());
    if cfg!(unix) && set("HYPRLAND_INSTANCE_SIGNATURE") {
        Backend::Hyprland
    } else if cfg!(unix) && set("SWAYSOCK") {
        Backend::Sway
    } else if cfg!(target_os = "linux")
        && set("WAYLAND_DISPLAY")
        && env::var("GDK_BACKEND").map_or(true, |b| !b.starts_with("x11"))
    {
        // En Wayland nativo no se puede leer la posición global del puntero.
        Backend::Unknown
    } else {
        Backend::Pointer
    }
}

/// `Some(true)` si la ventana principal sigue visible, `Some(false)` si no
/// y `None` si no se pudo averiguar (entonces se muestra el widget, como antes).
pub fn main_visible(
    app: &AppHandle,
    backend: Backend,
    main_label: &str,
    main_title: &str,
) -> Option<bool> {
    match backend {
        #[cfg(unix)]
        Backend::Hyprland => hypr::main_visible(main_title),
        #[cfg(unix)]
        Backend::Sway => sway::main_visible(main_title),
        Backend::Pointer => pointer_on_other_monitor(app, main_label),
        _ => None,
    }
}

/// Escucha los eventos del compositor que pueden cambiar la visibilidad de
/// Foco sin que cambie su foco (p. ej. cambiar de escritorio en el otro monitor).
/// Llama a `on_change` en cada uno. No hace nada fuera de Hyprland/Sway.
pub fn watch(backend: Backend, on_change: impl Fn() + Send + 'static) {
    #[cfg(unix)]
    match backend {
        Backend::Hyprland => {
            thread::spawn(move || hypr::watch(on_change));
        }
        Backend::Sway => {
            thread::spawn(move || sway::watch(on_change));
        }
        _ => {}
    }
    #[cfg(not(unix))]
    let _ = (backend, on_change);
}

fn pointer_on_other_monitor(app: &AppHandle, main_label: &str) -> Option<bool> {
    let main = app.get_webview_window(main_label)?;
    let monitors = app.available_monitors().ok()?;
    if monitors.len() < 2 {
        // Con un solo monitor esta aproximación no aporta nada.
        return Some(false);
    }
    let p = app.cursor_position().ok()?;
    let pointer_mon = app.monitor_from_point(p.x, p.y).ok()??;
    let main_mon = main.current_monitor().ok()??;
    Some(pointer_mon.position() != main_mon.position())
}

fn truthy(v: &Value) -> bool {
    match v {
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_i64().unwrap_or(0) != 0,
        _ => false,
    }
}

#[cfg(unix)]
mod hypr {
    use super::*;
    use std::{
        io::{Read, Write},
        os::unix::net::UnixStream,
        path::PathBuf,
        time::Duration,
    };

    /// `$XDG_RUNTIME_DIR/hypr/<firma>/<name>` (actual) o `/tmp/hypr/…` (antiguo).
    fn socket(name: &str) -> Option<PathBuf> {
        let sig = env::var("HYPRLAND_INSTANCE_SIGNATURE").ok()?;
        let mut dirs = Vec::new();
        if let Some(rt) = env::var_os("XDG_RUNTIME_DIR") {
            dirs.push(PathBuf::from(rt).join("hypr"));
        }
        dirs.push(PathBuf::from("/tmp/hypr"));
        dirs.into_iter()
            .map(|d| d.join(&sig).join(name))
            .find(|p| p.exists())
    }

    /// Igual que `hyprctl -j <cmd>`, pero sin lanzar un proceso.
    fn query(cmd: &str) -> Option<Value> {
        let mut s = UnixStream::connect(socket(".socket.sock")?).ok()?;
        let timeout = Some(Duration::from_millis(500));
        s.set_read_timeout(timeout).ok()?;
        s.set_write_timeout(timeout).ok()?;
        s.write_all(format!("j/{cmd}").as_bytes()).ok()?;
        let mut out = String::new();
        s.read_to_string(&mut out).ok()?;
        serde_json::from_str(&out).ok()
    }

    pub fn main_visible(title: &str) -> Option<bool> {
        let pid = i64::from(std::process::id());
        let clients = query("clients")?;
        let clients = clients.as_array()?;
        let me = clients
            .iter()
            .find(|c| c["pid"].as_i64() == Some(pid) && c["title"].as_str() == Some(title))?;
        if truthy(&me["hidden"]) || me["mapped"].as_bool() == Some(false) {
            return Some(false);
        }
        let ws = me["workspace"]["id"].as_i64()?;
        let monitors = query("monitors")?;
        let shown = monitors.as_array()?.iter().any(|m| {
            let special = m["specialWorkspace"]["id"].as_i64().unwrap_or(0);
            // Un escritorio especial abierto encima tapa al normal.
            special == ws || (m["activeWorkspace"]["id"].as_i64() == Some(ws) && special == 0)
        });
        if !shown {
            return Some(false);
        }
        // Otra ventana a pantalla completa en el mismo escritorio la tapa.
        let covered = !truthy(&me["fullscreen"])
            && clients
                .iter()
                .any(|c| c["workspace"]["id"].as_i64() == Some(ws) && truthy(&c["fullscreen"]));
        Some(!covered)
    }

    const EVENTS: &[&str] = &[
        "workspace",
        "workspacev2",
        "focusedmon",
        "focusedmonv2",
        "moveworkspace",
        "moveworkspacev2",
        "activespecial",
        "activespecialv2",
        "fullscreen",
        "movewindow",
        "movewindowv2",
        "openwindow",
        "closewindow",
        "changefloatingmode",
    ];

    pub fn watch(on_change: impl Fn()) {
        let Some(path) = socket(".socket2.sock") else {
            return;
        };
        let Ok(stream) = UnixStream::connect(path) else {
            return;
        };
        for line in BufReader::new(stream).lines() {
            let Ok(line) = line else { return };
            let name = line.split(">>").next().unwrap_or("");
            if EVENTS.contains(&name) {
                on_change();
            }
        }
    }
}

#[cfg(unix)]
mod sway {
    use super::*;
    use std::process::{Command, Stdio};

    fn find<'a>(node: &'a Value, pid: i64, title: &str) -> Option<&'a Value> {
        if node["pid"].as_i64() == Some(pid) && node["name"].as_str() == Some(title) {
            return Some(node);
        }
        ["nodes", "floating_nodes"]
            .iter()
            .filter_map(|k| node[*k].as_array())
            .flatten()
            .find_map(|n| find(n, pid, title))
    }

    pub fn main_visible(title: &str) -> Option<bool> {
        let out = Command::new("swaymsg")
            .args(["-r", "-t", "get_tree"])
            .output()
            .ok()?;
        let tree: Value = serde_json::from_slice(&out.stdout).ok()?;
        let pid = i64::from(std::process::id());
        // Sway ya calcula `visible` (escritorio activo, pantalla completa, pestañas…).
        find(&tree, pid, title)?["visible"].as_bool()
    }

    pub fn watch(on_change: impl Fn()) {
        let child = Command::new("swaymsg")
            .args([
                "-r",
                "-m",
                "-t",
                "subscribe",
                r#"["workspace","window","output"]"#,
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn();
        let Ok(mut child) = child else { return };
        let Some(out) = child.stdout.take() else {
            return;
        };
        for line in BufReader::new(out).lines() {
            let Ok(line) = line else { break };
            // Un evento por objeto JSON (puede venir en varias líneas).
            if line.starts_with('{') {
                on_change();
            }
        }
        let _ = child.kill();
        let _ = child.wait();
    }
}
