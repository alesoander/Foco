// Foco — organizador de tareas con widget flotante.
//
// Arquitectura (pensada para gastar lo mínimo):
// - El estado es un único JSON que vive aquí (memoria + archivo en disco).
//   Ambas ventanas lo leen/escriben con `get_state` / `set_state` y reciben
//   el evento "state" cuando cambia, así nunca se desincronizan.
// - La ventana principal se oculta (no se destruye) al pasar a modo widget.
// - El widget se crea bajo demanda y se DESTRUYE al volver a la ventana
//   principal, para no mantener un segundo webview en memoria.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod mcp;
mod ops;
mod visibility;

use std::{
    fs,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering::Relaxed},
        Mutex,
    },
    thread,
    time::Duration,
};

use tauri::{
    AppHandle, Emitter, LogicalSize, Manager, WebviewUrl, WebviewWindowBuilder, Window, WindowEvent,
};

// Escritura atómica: nunca deja el archivo a medias si se corta la luz.
use mcp::write_atomic;
use visibility::Backend;

const MAIN: &str = "main";
/// Título de la ventana principal (tauri.conf.json). Sirve para encontrarla
/// en la lista de ventanas del compositor.
const MAIN_TITLE: &str = "Foco";
const WIDGET: &str = "widget";
/// Título fijo del widget: lo usan las reglas de Hyprland/Sway para fijarlo.
const WIDGET_TITLE: &str = "Foco Widget";
/// Tamaño del widget a escala 1. El usuario lo escala (Ajustes o Ctrl+rueda).
const WIDGET_SIZE: (f64, f64) = (320.0, 84.0);
const WIDGET_SCALE: (f64, f64) = (0.6, 1.4);
/// Espera antes de mostrar el widget al perder el foco. Evita parpadeos con
/// focus-follows-mouse o al abrir diálogos.
const AUTO_DELAY: Duration = Duration::from_millis(650);
/// Espera tras un evento del compositor (agrupa ráfagas de eventos).
const EVENT_DELAY: Duration = Duration::from_millis(250);

struct Store {
    path: PathBuf,
    json: Mutex<String>,
    /// Ajuste "widget automático al cambiar de ventana".
    auto_widget: AtomicBool,
    /// true cuando el widget se abrió a propósito (ventana principal oculta).
    pinned: AtomicBool,
    /// Se incrementa en cada cambio de foco; cancela temporizadores viejos.
    focus_epoch: AtomicU64,
    /// Serializa crear/destruir el widget.
    widget_lock: Mutex<()>,
    /// Escala del widget (ajuste `widgetScale`).
    widget_scale: Mutex<f64>,
    /// Cómo averiguar si la ventana principal sigue visible.
    backend: Backend,
}

fn auto_widget_flag(v: &serde_json::Value) -> bool {
    v.pointer("/settings/autoWidget")
        .and_then(|b| b.as_bool())
        .unwrap_or(true)
}

fn widget_scale_of(v: &serde_json::Value) -> f64 {
    let s = v
        .pointer("/settings/widgetScale")
        .and_then(|n| n.as_f64())
        .unwrap_or(1.0);
    if s.is_finite() {
        s.clamp(WIDGET_SCALE.0, WIDGET_SCALE.1)
    } else {
        1.0
    }
}

// ───────────────────────── Comandos ─────────────────────────

#[tauri::command]
fn get_state(store: tauri::State<'_, Store>) -> String {
    store.json.lock().unwrap().clone()
}

#[tauri::command]
fn set_state(app: AppHandle, store: tauri::State<'_, Store>, state: String) -> Result<(), String> {
    let v: serde_json::Value = serde_json::from_str(&state).map_err(|e| e.to_string())?;
    {
        let mut cur = store.json.lock().unwrap();
        let cur_v = serde_json::from_str::<serde_json::Value>(&cur).unwrap_or_default();
        // Cada cambio sube `rev`. Si llega uno que no supera el actual, se hizo
        // sobre un estado viejo (p. ej. el MCP cambió algo a la vez): gana el
        // actual, que se reenvía con `rev` nueva para que las ventanas se pongan al día.
        if !cur.is_empty() && ops::rev(&v) <= ops::rev(&cur_v) {
            let mut fresh = cur_v;
            ops::bump_rev(&mut fresh);
            let fresh = fresh.to_string();
            write_atomic(&store.path, &fresh).map_err(|e| e.to_string())?;
            cur.clone_from(&fresh);
            drop(cur);
            return app.emit("state", fresh).map_err(|e| e.to_string());
        }
        write_atomic(&store.path, &state).map_err(|e| e.to_string())?;
        cur.clone_from(&state);
    }
    store.auto_widget.store(auto_widget_flag(&v), Relaxed);
    let scale = widget_scale_of(&v);
    let changed = {
        let mut cur = store.widget_scale.lock().unwrap();
        let changed = (*cur - scale).abs() > f64::EPSILON;
        *cur = scale;
        changed
    };
    if changed {
        apply_widget_scale(&app, scale);
    }
    app.emit("state", state).map_err(|e| e.to_string())
}

// Los comandos que crean ventanas son async para no bloquear el hilo principal.
#[tauri::command]
async fn show_widget(app: AppHandle) {
    enter_widget(&app, true);
}

#[tauri::command]
async fn show_main(app: AppHandle) {
    enter_main(&app);
}

/// Cambia el tamaño del widget al momento (Ctrl+rueda sobre él). El widget
/// también lo llama al cargar, para asegurar el zoom tras crear el webview.
#[tauri::command]
fn set_widget_scale(app: AppHandle, store: tauri::State<'_, Store>, scale: f64) {
    if !scale.is_finite() {
        return;
    }
    let scale = scale.clamp(WIDGET_SCALE.0, WIDGET_SCALE.1);
    *store.widget_scale.lock().unwrap() = scale;
    apply_widget_scale(&app, scale);
}

#[tauri::command]
fn quit(app: AppHandle) {
    app.exit(0);
}

// ───────────────────────── Ventanas ─────────────────────────

/// Muestra el widget. Con `hide_main` además oculta la ventana principal
/// (equivale a "minimizar"); sin él, la principal se queda donde está
/// (modo automático, como el PiP de Google Meet).
fn enter_widget(app: &AppHandle, hide_main: bool) {
    let store = app.state::<Store>();
    if hide_main {
        store.pinned.store(true, Relaxed);
        if let Some(main) = app.get_webview_window(MAIN) {
            let _ = main.hide();
        }
    }

    let _guard = store.widget_lock.lock().unwrap();
    if let Some(w) = app.get_webview_window(WIDGET) {
        let _ = w.show();
        return;
    }
    let scale = *store.widget_scale.lock().unwrap();
    let (w, h) = (WIDGET_SIZE.0 * scale, WIDGET_SIZE.1 * scale);
    // Tamaño fijo mediante mínimo = máximo. Con `.resizable(false)` GTK impone
    // un alto mínimo de 200 px e ignora el tamaño pedido.
    let built = WebviewWindowBuilder::new(app, WIDGET, WebviewUrl::App("widget.html".into()))
        .title(WIDGET_TITLE)
        .inner_size(w, h)
        .min_inner_size(w, h)
        .max_inner_size(w, h)
        .decorations(false)
        .transparent(true)
        .always_on_top(true) // X11/XWayland. En Hyprland/Sway lo hace la regla `pin`/`sticky`.
        .visible_on_all_workspaces(true)
        .skip_taskbar(true)
        .focused(false)
        .build();
    match built {
        Ok(w) => {
            let _ = w.set_zoom(scale);
        }
        Err(e) => eprintln!("foco: no se pudo crear el widget: {e}"),
    }
}

/// Escala el widget: la ventana cambia de tamaño y el contenido se amplía o
/// reduce con el zoom del webview, así el diseño (320×84) es siempre el mismo.
fn apply_widget_scale(app: &AppHandle, scale: f64) {
    let Some(w) = app.get_webview_window(WIDGET) else {
        return;
    };
    let size = LogicalSize::new(WIDGET_SIZE.0 * scale, WIDGET_SIZE.1 * scale);
    // Se quitan los límites, se cambia el tamaño y se vuelven a fijar.
    let _ = w.set_min_size(None::<LogicalSize<f64>>);
    let _ = w.set_max_size(None::<LogicalSize<f64>>);
    let _ = w.set_size(size);
    let _ = w.set_min_size(Some(size));
    let _ = w.set_max_size(Some(size));
    let _ = w.set_zoom(scale);
}

/// Destruye el widget (libera su webview). Se usa al volver a la app a propósito.
fn close_widget(app: &AppHandle) {
    let store = app.state::<Store>();
    let _guard = store.widget_lock.lock().unwrap();
    if let Some(w) = app.get_webview_window(WIDGET) {
        let _ = w.destroy();
    }
}

/// Solo lo oculta. Se usa en el modo automático: con focus-follows-mouse el
/// foco entra y sale a menudo y recrear el webview cada vez sería más caro.
fn hide_widget(app: &AppHandle) {
    let store = app.state::<Store>();
    let _guard = store.widget_lock.lock().unwrap();
    if let Some(w) = app.get_webview_window(WIDGET) {
        let _ = w.hide();
    }
}

fn enter_main(app: &AppHandle) {
    let store = app.state::<Store>();
    store.pinned.store(false, Relaxed);
    store.focus_epoch.fetch_add(1, Relaxed);
    if let Some(main) = app.get_webview_window(MAIN) {
        let _ = main.unminimize();
        let _ = main.show();
        let _ = main.set_focus();
    }
    close_widget(app);
}

/// Programa `auto_check` tras `delay`. Cualquier cambio de foco posterior
/// (o otra llamada) lo cancela.
fn schedule_check(app: &AppHandle, delay: Duration) {
    let store = app.state::<Store>();
    if !store.auto_widget.load(Relaxed) || store.pinned.load(Relaxed) {
        return;
    }
    let epoch = store.focus_epoch.fetch_add(1, Relaxed) + 1;
    let app = app.clone();
    thread::spawn(move || {
        thread::sleep(delay);
        if app.state::<Store>().focus_epoch.load(Relaxed) == epoch {
            auto_check(&app);
        }
    });
}

/// Modo automático: con la ventana principal abierta pero sin foco, muestra
/// el widget solo si Foco ya no se ve (p. ej. no se muestra si sigue a la
/// vista en el otro monitor) y lo oculta si vuelve a verse.
fn auto_check(app: &AppHandle) {
    let store = app.state::<Store>();
    if !store.auto_widget.load(Relaxed) || store.pinned.load(Relaxed) {
        return;
    }
    let Some(main) = app.get_webview_window(MAIN) else {
        return;
    };
    if main.is_focused().unwrap_or(false) || !main.is_visible().unwrap_or(false) {
        return;
    }
    if visibility::main_visible(app, store.backend, MAIN, MAIN_TITLE) == Some(true) {
        hide_widget(app);
    } else {
        enter_widget(app, false);
    }
}

/// `foco --toggle` (pensado para un atajo de Hyprland/Sway).
fn toggle(app: &AppHandle) {
    let main_shown = app
        .get_webview_window(MAIN)
        .and_then(|m| m.is_visible().ok())
        .unwrap_or(false);
    if main_shown && app.get_webview_window(WIDGET).is_none() {
        enter_widget(app, true);
    } else {
        enter_main(app);
    }
}

fn on_window_event(window: &Window, event: &WindowEvent) {
    // La ventana principal existe antes de que `setup` registre el estado.
    let Some(store) = window.try_state::<Store>() else {
        return;
    };
    let app = window.app_handle().clone();

    // Si el compositor cierra el widget (p. ej. Super+Q) con la ventana
    // principal oculta, la app se quedaría sin ventanas: volvemos a la principal.
    if window.label() == WIDGET {
        if let WindowEvent::CloseRequested { api, .. } = event {
            if store.pinned.load(Relaxed) {
                api.prevent_close();
                thread::spawn(move || enter_main(&app));
            }
        }
        return;
    }
    if window.label() != MAIN {
        return;
    }
    // Las operaciones de ventana se lanzan desde otro hilo: crear un webview
    // desde dentro de un callback del bucle de eventos puede bloquearlo.
    match event {
        // Cerrar = minimizar al widget. Para salir: Ctrl+Q o "Salir" en Ajustes.
        WindowEvent::CloseRequested { api, .. } => {
            api.prevent_close();
            thread::spawn(move || enter_widget(&app, true));
        }
        WindowEvent::Focused(true) => {
            store.focus_epoch.fetch_add(1, Relaxed);
            if !store.pinned.load(Relaxed) {
                thread::spawn(move || hide_widget(&app));
            }
        }
        WindowEvent::Focused(false) => {
            store.focus_epoch.fetch_add(1, Relaxed);
            schedule_check(&app, AUTO_DELAY);
        }
        // Minimizar nativo (X11, KDE, GNOME…). Hyprland no minimiza.
        WindowEvent::Resized(_) => {
            if window.is_minimized().unwrap_or(false) {
                thread::spawn(move || enter_widget(&app, true));
            }
        }
        _ => {}
    }
}

/// Recibe las operaciones del MCP (`foco --mcp`) mientras la app está abierta.
/// La app es la única que escribe el estado; así no se pisan los cambios.
#[cfg(unix)]
fn start_mcp_socket(app: AppHandle) {
    use std::os::unix::{fs::PermissionsExt, net::UnixListener};
    let Some(path) = mcp::socket_path() else {
        return;
    };
    let _ = fs::remove_file(&path);
    let listener = match UnixListener::bind(&path) {
        Ok(l) => l,
        Err(e) => {
            eprintln!(
                "foco: no se pudo abrir el socket del MCP ({}): {e}",
                path.display()
            );
            return;
        }
    };
    let _ = fs::set_permissions(&path, fs::Permissions::from_mode(0o600));
    thread::spawn(move || {
        for conn in listener.incoming().flatten() {
            let app = app.clone();
            thread::spawn(move || serve_mcp(&app, conn));
        }
    });
}

#[cfg(unix)]
fn serve_mcp(app: &AppHandle, conn: std::os::unix::net::UnixStream) {
    use std::io::{BufRead, BufReader, Write};
    let mut line = String::new();
    if BufReader::new(&conn).read_line(&mut line).is_err() {
        return;
    }
    let store = app.state::<Store>();
    let (resp, new) = {
        let mut cur = store.json.lock().unwrap();
        let (resp, new) = mcp::handle_socket_request(&line, &cur);
        if let Some(new) = &new {
            if let Err(e) = write_atomic(&store.path, new) {
                eprintln!("foco: no se pudo guardar el estado: {e}");
            }
            cur.clone_from(new);
        }
        (resp, new)
    };
    if let Some(new) = new {
        let _ = app.emit("state", new);
    }
    let _ = (&conn).write_all(resp.as_bytes());
}

fn main() {
    // `foco --mcp`: servidor MCP por stdin/stdout, sin interfaz.
    if std::env::args().any(|a| a == "--mcp") {
        mcp::run();
        return;
    }
    tauri::Builder::default()
        // Una sola instancia: volver a lanzar `foco` reutiliza la que ya corre.
        .plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            let app = app.clone();
            let has = |flag: &str| args.iter().any(|a| a == flag);
            let (toggle_req, widget_req) = (has("--toggle"), has("--widget"));
            thread::spawn(move || {
                if toggle_req {
                    toggle(&app)
                } else if widget_req {
                    enter_widget(&app, true)
                } else {
                    enter_main(&app)
                }
            });
        }))
        .setup(|app| {
            let dir = app.path().app_data_dir()?;
            fs::create_dir_all(&dir)?;
            let path = dir.join("state.json");
            let mut json = fs::read_to_string(&path).unwrap_or_default();
            let (mut auto, mut scale) = (true, 1.0);
            if let Ok(mut v) = serde_json::from_str::<serde_json::Value>(&json) {
                auto = auto_widget_flag(&v);
                scale = widget_scale_of(&v);
                // Si se apagó el equipo con el cronómetro en marcha, no sumar
                // todo ese tiempo: se pausa al arrancar. El frontend guarda un
                // punto de control cada pocos minutos, así se pierde poco.
                if let Some(since) = v.pointer_mut("/active/since") {
                    if !since.is_null() {
                        *since = serde_json::Value::Null;
                        json = v.to_string();
                    }
                }
            }
            app.manage(Store {
                path,
                json: Mutex::new(json),
                auto_widget: AtomicBool::new(auto),
                pinned: AtomicBool::new(false),
                focus_epoch: AtomicU64::new(0),
                widget_lock: Mutex::new(()),
                widget_scale: Mutex::new(scale),
                backend: visibility::backend(),
            });

            #[cfg(unix)]
            start_mcp_socket(app.handle().clone());

            // Eventos del compositor (Hyprland/Sway): cambiar de escritorio en
            // otro monitor puede tapar o destapar Foco sin cambiar su foco.
            let handle = app.handle().clone();
            visibility::watch(app.state::<Store>().backend, move || {
                schedule_check(&handle, EVENT_DELAY)
            });

            if std::env::args().any(|a| a == "--widget") {
                let handle = app.handle().clone();
                thread::spawn(move || enter_widget(&handle, true));
            } else if let Some(main) = app.get_webview_window(MAIN) {
                main.show()?;
                main.set_focus()?;
            }
            Ok(())
        })
        .on_window_event(on_window_event)
        .invoke_handler(tauri::generate_handler![
            get_state,
            set_state,
            show_widget,
            show_main,
            set_widget_scale,
            quit
        ])
        .run(tauri::generate_context!())
        .expect("error al iniciar Foco");
}
