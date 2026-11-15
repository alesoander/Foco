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

use std::{
    fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering::Relaxed},
        Mutex,
    },
    thread,
    time::Duration,
};

use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder, Window, WindowEvent};

const MAIN: &str = "main";
const WIDGET: &str = "widget";
/// Título fijo del widget: lo usan las reglas de Hyprland/Sway para fijarlo.
const WIDGET_TITLE: &str = "Foco Widget";
const WIDGET_SIZE: (f64, f64) = (320.0, 84.0);
/// Espera antes de mostrar el widget al perder el foco. Evita parpadeos con
/// focus-follows-mouse o al abrir diálogos.
const AUTO_DELAY: Duration = Duration::from_millis(650);

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
}

fn auto_widget_flag(v: &serde_json::Value) -> bool {
    v.pointer("/settings/autoWidget")
        .and_then(|b| b.as_bool())
        .unwrap_or(true)
}

/// Escritura atómica: nunca deja el archivo a medias si se corta la luz.
fn write_atomic(path: &Path, data: &str) -> std::io::Result<()> {
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, data)?;
    fs::rename(&tmp, path)
}

// ───────────────────────── Comandos ─────────────────────────

#[tauri::command]
fn get_state(store: tauri::State<'_, Store>) -> String {
    store.json.lock().unwrap().clone()
}

#[tauri::command]
fn set_state(app: AppHandle, store: tauri::State<'_, Store>, state: String) -> Result<(), String> {
    let v: serde_json::Value = serde_json::from_str(&state).map_err(|e| e.to_string())?;
    store.auto_widget.store(auto_widget_flag(&v), Relaxed);
    {
        let mut cur = store.json.lock().unwrap();
        write_atomic(&store.path, &state).map_err(|e| e.to_string())?;
        cur.clone_from(&state);
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
    let built = WebviewWindowBuilder::new(app, WIDGET, WebviewUrl::App("widget.html".into()))
        .title(WIDGET_TITLE)
        .inner_size(WIDGET_SIZE.0, WIDGET_SIZE.1)
        .resizable(false)
        .decorations(false)
        .transparent(true)
        .always_on_top(true) // X11/XWayland. En Hyprland/Sway lo hace la regla `pin`/`sticky`.
        .visible_on_all_workspaces(true)
        .skip_taskbar(true)
        .focused(false)
        .build();
    if let Err(e) = built {
        eprintln!("foco: no se pudo crear el widget: {e}");
    }
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
            let epoch = store.focus_epoch.fetch_add(1, Relaxed) + 1;
            if !store.auto_widget.load(Relaxed) {
                return;
            }
            thread::spawn(move || {
                thread::sleep(AUTO_DELAY);
                let store = app.state::<Store>();
                let main_focused = app
                    .get_webview_window(MAIN)
                    .and_then(|m| m.is_focused().ok())
                    .unwrap_or(false);
                if store.focus_epoch.load(Relaxed) == epoch && !main_focused {
                    enter_widget(&app, false);
                }
            });
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

fn main() {
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
            let mut auto = true;
            if let Ok(mut v) = serde_json::from_str::<serde_json::Value>(&json) {
                auto = auto_widget_flag(&v);
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
            quit
        ])
        .run(tauri::generate_context!())
        .expect("error al iniciar Foco");
}
