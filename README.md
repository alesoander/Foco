# Foco

Organizador de tareas para escritorio Linux (pensado para **Arch + Hyprland/Sway**) con un **widget flotante siempre visible** que muestra la tarea en la que estás trabajando, al estilo del picture‑in‑picture de Google Meet. Interfaz de vidrio esmerilado inspirada en macOS.

- **Listas y tareas**: crear, renombrar y colorear listas; tareas con notas; completadas plegables.
- **Tarea en foco**: pulsa ◎ en una tarea y empieza el cronómetro. El tiempo se acumula por tarea.
- **Widget flotante (320×84)**: muestra la tarea y el tiempo, y se puede arrastrar. Al pasar el ratón aparecen los botones *completar*, *siguiente* y *abrir*. Con doble clic vuelves a la app.
- **Cuándo aparece el widget**:
  - con el botón amarillo, el rojo o `Ctrl+M`;
  - con `foco --toggle` desde un atajo del compositor;
  - automáticamente al cambiar de ventana o de escritorio (se puede desactivar en Ajustes).

## Por qué consume poco

Estas cifras son **las esperadas por diseño, no mediciones**. La app aún no se ha ejecutado en Arch.

| Decisión | Efecto |
|---|---|
| **Tauri 2** (Rust + WebKitGTK del sistema) en vez de Electron | No incluye su propio Chromium. Binario de unos pocos MB y mucha menos RAM. |
| Frontend **sin framework** (TypeScript puro) | ≈35 KB de JS + 15 KB de CSS en total. |
| El widget se **crea al mostrarse y se destruye** al volver a la app | No queda un segundo webview ocupando memoria. En modo automático solo se oculta, para no recrearlo con cada cambio de foco. |
| El desenfoque lo hace el **compositor**, no la app | Sin `backdrop-filter` permanente (es caro en WebKitGTK). |
| Ningún bucle de animación; un único `setInterval` de 1 s **solo mientras el cronómetro corre** | Prácticamente 0 % de CPU en reposo. |
| Estado = un JSON pequeño con escritura atómica | Sin base de datos ni plugins extra. |

## Requisitos (Arch)

```bash
sudo pacman -S --needed webkit2gtk-4.1 gtk3 base-devel rustup npm inter-font
```

```bash
rustup default stable
```

## Desarrollo

```bash
npm install
```

```bash
npm run tauri dev
```

Para previsualizar solo la interfaz en el navegador, sin Rust, usa `npm run dev` y abre `http://localhost:1420` (y `/widget.html`). En ese modo el estado se guarda en `localStorage`.

## Instalar como paquete

```bash
cd packaging/arch && makepkg -si
```

Instala `/usr/bin/foco`, el lanzador `.desktop` y los iconos.

## Configurar el compositor (importante)

En Wayland una aplicación **no puede** ponerse "siempre encima" ni elegir su posición por sí misma. De eso se encarga el compositor mediante reglas que reconocen el widget por su título, `Foco Widget`.

- **Hyprland con configuración Lua** (versiones actuales) usa [`packaging/compositor/hyprland.lua`](packaging/compositor/hyprland.lua):
  - `float` + `pin` dejan el widget encima de todo y visible en todos los escritorios;
  - `no_initial_focus` evita que te robe el foco;
  - incluye además el atajo `Super+T` y el arranque automático como widget.
- **Hyprland con el formato `.conf` antiguo** usa [`packaging/compositor/hyprland.conf`](packaging/compositor/hyprland.conf), con `windowrulev2`. Mira cuál usa tu versión con `hyprctl version`.
- **Sway** usa [`packaging/compositor/sway.conf`](packaging/compositor/sway.conf). Ahí `sticky` cumple la función de `pin`. Sway no tiene blur (SwayFX sí); sube la *Opacidad del vidrio* en Ajustes.

Para el vidrio esmerilado hace falta el blur del compositor. En Hyprland viene activado por defecto (`decoration.blur.enabled`). Sin blur la interfaz sigue viéndose bien: solo es más translúcida.

> La sintaxis Lua se tomó de la wiki de Hyprland (rama en desarrollo, noviembre 2026), pero no la he probado en un Hyprland real. Si algo no encaja, comprueba el título de las ventanas con `hyprctl clients`.

## Uso

| Acción | Cómo |
|---|---|
| Nueva tarea | `Ctrl+N`, o escribe en "Nueva tarea" y pulsa Enter |
| Poner en foco / iniciar / pausar | ◎ en la tarea · `Espacio` |
| Notas | Clic en la fila o en el icono de notas |
| Renombrar lista | Doble clic en su nombre · clic en el icono para cambiar el color |
| Eliminar | Papelera y luego "Eliminar" (dos clics) |
| Minimizar a widget | Botón amarillo o rojo · `Ctrl+M` · botón "Widget" |
| Volver a la app | Doble clic en el widget o su botón ⤢ |
| Ajustes | `Ctrl+,` o el icono de ajustes en la barra lateral |
| Salir | `Ctrl+Q` o "Salir de Foco" en Ajustes. Cerrar la ventana solo la minimiza al widget. |

Parámetros de línea de comandos (si Foco ya está abierto, los recibe la instancia existente):

- `foco --toggle`: alterna entre la ventana y el widget.
- `foco --widget`: arranca o pasa directamente al widget.

Los datos se guardan en `~/.local/share/io.github.foco/state.json`.

## Estado de las pruebas

- **Probado**: la interfaz, la lógica de tareas, el cronómetro y la sincronización con el widget. Se compiló el frontend y se probó en un navegador.
- **Compilado y probado en Windows**:
  - el lado Rust compila sin errores ni avisos;
  - el estado se guarda a través de Rust, lo que confirma que funcionan la comunicación con el frontend, los permisos y la política de seguridad (CSP);
  - `--toggle` alterna entre ventana y widget, y el widget queda siempre encima;
  - hay una sola instancia.
- **Sin probar en Linux**: WebKitGTK, la transparencia, el arrastre de ventanas, las reglas de Hyprland/Sway y el PKGBUILD. La primera prueba real es `npm run tauri dev` en Arch.
- **Cronómetro**: si apagas el equipo con el cronómetro en marcha, al arrancar se pausa. Se pierden como mucho los últimos 5 minutos, porque se guarda un punto de control cada 5 minutos.

## Problemas conocidos

- **Pantalla en blanco o parpadeos con NVIDIA**: lanza la app con `WEBKIT_DISABLE_DMABUF_RENDERER=1 foco`. En Hyprland, por ejemplo: `env = WEBKIT_DISABLE_DMABUF_RENDERER,1`.
- **El widget no queda encima**: faltan las reglas del compositor (ver arriba). Otra opción es forzar XWayland con `GDK_BACKEND=x11 foco`; ahí *always‑on‑top* funciona sin reglas, pero el escalado HiDPI puede verse peor.
- **Hyprland no tiene "minimizar"**: por eso existen el botón amarillo, el atajo `--toggle` y el modo automático al cambiar de ventana.

## Estructura

```
src/                    Interfaz (TypeScript + CSS, sin framework)
  main.ts               Ventana principal
  widget.ts             Widget flotante
  store.ts              Modelo de datos y operaciones (compartido)
  bridge.ts             Puente con Rust (o localStorage en el navegador)
  ui.ts                 Iconos SVG, apariencia, temporizador
  styles/               base.css (tokens claro/oscuro), main.css, widget.css
src-tauri/
  src/main.rs           Estado, ventanas, widget, instancia única
  tauri.conf.json       Ventana principal transparente y sin decoraciones
  capabilities/         Permisos de las dos ventanas
packaging/
  arch/                 PKGBUILD + .desktop
  compositor/           Reglas para Hyprland y Sway
```
