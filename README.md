# Foco

Organizador de tareas para escritorio Linux (pensado para **Arch + Hyprland/Sway**) con un **widget flotante siempre visible** que muestra la tarea en la que estás trabajando, al estilo del picture‑in‑picture de Google Meet. Interfaz de vidrio esmerilado inspirada en macOS.

- **Listas y tareas**: crear, renombrar y colorear listas; tareas con código interno y notas; completadas plegables.
- **Notas internas y respuestas**: cada tarea tiene un apartado *Notas internas* (las añade Claude por MCP). Tus notas comunes son tu *respuesta* a esas notas, y Claude puede leerlas.
- **MCP para Claude**: Claude puede crear listas y tareas, asignar códigos, marcar tareas como concluidas, consultar su estado y añadir notas internas (ver [MCP para Claude](#mcp-para-claude)).
- **Código interno**: cada tarea puede llevar un código propio (p. ej. `PRJ-142`). Se edita al abrir las notas de la tarea y se ve como etiqueta junto a la vista previa.
- **Tarea en foco**: pulsa ◎ en una tarea y empieza el cronómetro. El tiempo se acumula por tarea.
- **Widget flotante (320×84 al 100 %)**: muestra la tarea y el tiempo, y se puede arrastrar. Al pasar el ratón aparecen los botones *completar*, *siguiente* y *abrir*. Con doble clic vuelves a la app.
- **Tamaño del widget**: de 60 % (192×50) a 140 % (448×118), en *Ajustes → Tamaño del widget* o con `Ctrl` + rueda del ratón sobre el widget. Se guarda y se mantiene al reiniciar.
- **Cuándo aparece el widget**:
  - con el botón amarillo, el rojo o `Ctrl+M`;
  - con `foco --toggle` desde un atajo del compositor;
  - automáticamente al cambiar de ventana o de escritorio (se puede desactivar en Ajustes).
- **Varios monitores**: el modo automático **no** muestra el widget mientras la ventana de Foco siga a la vista, por ejemplo en el otro monitor mientras trabajas en este. Si después cambias de escritorio en el monitor de Foco, o una ventana a pantalla completa lo tapa, el widget aparece; si Foco vuelve a verse, se oculta. Cómo lo detecta:
  - **Hyprland**: pregunta al compositor por su socket de IPC y escucha sus eventos (sin lanzar procesos ni sondear).
  - **Sway**: mediante `swaymsg` (`get_tree` y `subscribe`).
  - **X11 y Windows**: si el puntero está en un monitor distinto del de Foco al perder el foco, se da por hecho que Foco sigue visible.
  - **GNOME/KDE en Wayland**: no hay forma de saberlo; el widget aparece como antes.

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
  - no fija el tamaño: lo pone Foco según tu ajuste;
  - `no_initial_focus` evita que te robe el foco;
  - incluye además el atajo `Super+T` y el arranque automático como widget.
- **Hyprland con el formato `.conf` antiguo** usa [`packaging/compositor/hyprland.conf`](packaging/compositor/hyprland.conf), con `windowrulev2`. Mira cuál usa tu versión con `hyprctl version`.
- **Sway** usa [`packaging/compositor/sway.conf`](packaging/compositor/sway.conf). Ahí `sticky` cumple la función de `pin`. Sway no tiene blur (SwayFX sí); sube la *Opacidad del vidrio* en Ajustes.

Para el vidrio esmerilado hace falta el blur del compositor. En Hyprland viene activado por defecto (`decoration.blur.enabled`). Sin blur la interfaz sigue viéndose bien: solo es más translúcida.

> **Si ya copiaste las reglas antes**: quita la regla de tamaño del widget (`size = { 320, 84 }`, `windowrulev2 = size 320 84, …` o `resize set 320 84` en Sway). Si no, el compositor impondría siempre 320×84 y el ajuste de tamaño no tendría efecto.

> La sintaxis Lua se tomó de la wiki de Hyprland (rama en desarrollo, noviembre 2026), pero no la he probado en un Hyprland real. Si algo no encaja, comprueba el título de las ventanas con `hyprctl clients`.

## MCP para Claude

El mismo binario hace de servidor MCP con `foco --mcp` (por stdin/stdout). No hace falta instalar nada más.

**Claude Code:**

```bash
claude mcp add --scope user foco -- foco --mcp
```

**Claude Desktop** (`~/.config/Claude/claude_desktop_config.json` en Linux):

```json
{
  "mcpServers": {
    "foco": { "command": "foco", "args": ["--mcp"] }
  }
}
```

| Herramienta | Qué hace |
|---|---|
| `listar_listas` | Listas con sus tareas pendientes y concluidas |
| `crear_lista` | Crea una lista (nombre y color opcional) |
| `listar_tareas` | Tareas con su estado (concluida o pendiente), código, tiempo, nº de notas internas y si respondiste. Filtra por lista y estado |
| `ver_tarea` | Detalle: estado, notas internas y tu respuesta |
| `crear_tarea` | Crea una tarea, con código y primera nota interna opcionales |
| `asignar_codigo` | Asigna o cambia el código interno (son únicos) |
| `marcar_concluida` | Concluye una tarea o la devuelve a pendiente |
| `agregar_nota_interna` | Añade una nota al apartado *Notas internas* |

Las tareas se pueden indicar por su id o por su código interno, y las listas por id o por nombre.

**Cómo se sincroniza**: si Foco está abierto, el MCP le envía cada operación por un socket local (`$XDG_RUNTIME_DIR/foco-mcp.sock`) y es la app quien la aplica, la guarda y actualiza sus ventanas al momento. Si Foco está cerrado, el MCP edita directamente `state.json`. Así nunca hay dos procesos escribiendo el mismo archivo. En Windows no hay socket: con la app abierta, sus cambios podrían sobrescribir los del MCP.

## Uso

| Acción | Cómo |
|---|---|
| Nueva tarea | `Ctrl+N`, o escribe en "Nueva tarea" y pulsa Enter |
| Poner en foco / iniciar / pausar | ◎ en la tarea · `Espacio` |
| Notas y código interno | Clic en la fila o en el icono de notas. `Enter` en el código pasa a las notas |
| Notas internas | Se ven al abrir las notas; el contador azul de la fila indica cuántas hay. Tus notas comunes son la respuesta. Para borrar una, pasa el ratón y pulsa la ✕ dos veces |
| Tamaño del widget | `Ctrl` + rueda sobre el widget · *Ajustes → Tamaño del widget* |
| Renombrar lista | Doble clic en su nombre · clic en el icono para cambiar el color |
| Eliminar | Papelera y luego "Eliminar" (dos clics) |
| Minimizar a widget | Botón amarillo o rojo · `Ctrl+M` · botón "Widget" |
| Volver a la app | Doble clic en el widget o su botón ⤢ |
| Ajustes | `Ctrl+,` o el icono de ajustes en la barra lateral |
| Salir | `Ctrl+Q` o "Salir de Foco" en Ajustes. Cerrar la ventana solo la minimiza al widget. |

Parámetros de línea de comandos (si Foco ya está abierto, los recibe la instancia existente):

- `foco --toggle`: alterna entre la ventana y el widget.
- `foco --widget`: arranca o pasa directamente al widget.
- `foco --mcp`: servidor MCP para Claude (no abre ninguna ventana).

Los datos se guardan en `~/.local/share/io.github.foco/state.json`.

## Estado de las pruebas

- **Probado**: la interfaz, la lógica de tareas, el cronómetro y la sincronización con el widget. Se compiló el frontend y se probó en un navegador.
- **Compilado y probado en Windows**:
  - el lado Rust compila sin errores ni avisos;
  - el estado se guarda a través de Rust, lo que confirma que funcionan la comunicación con el frontend, los permisos y la política de seguridad (CSP);
  - `--toggle` alterna entre ventana y widget, y el widget queda siempre encima;
  - hay una sola instancia.
- **Probado en Linux (X11 virtual, WebKitGTK 2.x)**:
  - el tamaño del widget: se crea con el tamaño guardado, `Ctrl` + rueda lo cambia paso a paso y el contenido escala proporcionalmente;
  - la detección con Hyprland, contra un servidor que imita su IPC: no aparece si Foco sigue visible; aparece al cambiar de escritorio o con una ventana a pantalla completa encima; se oculta al volver a verse Foco.
- **MCP**: pruebas unitarias de las operaciones (`cargo test`) y prueba completa con la app abierta en X11 virtual: los cambios del MCP aparecen al momento en la ventana, la respuesta escrita en la app se lee por MCP y, con la app cerrada, el MCP edita el archivo.
- **Sin probar**: Claude Code o Claude Desktop conectados de verdad (el protocolo se probó a mano), Hyprland y Sway reales, la detección X11 con dos monitores reales (el servidor X virtual solo ofrece uno), la transparencia, el arrastre de ventanas y el PKGBUILD.
- **Cronómetro**: si apagas el equipo con el cronómetro en marcha, al arrancar se pausa. Se pierden como mucho los últimos 5 minutos, porque se guarda un punto de control cada 5 minutos.

## Problemas conocidos

- **`Error 71 (Error de protocolo)`, pantalla en blanco o parpadeos (sobre todo con NVIDIA)**: los causa el renderizador DMABUF de WebKitGTK. Foco lo desactiva por su cuenta (`WEBKIT_DISABLE_DMABUF_RENDERER=1`). Si quieres probar con él activado, lanza `WEBKIT_DISABLE_DMABUF_RENDERER=0 foco`.
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
  src/visibility.rs     ¿Sigue Foco a la vista? (Hyprland, Sway, X11/Windows)
  src/mcp.rs            Servidor MCP (`foco --mcp`) y socket con la app abierta
  src/ops.rs            Operaciones del MCP sobre el estado
  tauri.conf.json       Ventana principal transparente y sin decoraciones
  capabilities/         Permisos de las dos ventanas
packaging/
  arch/                 PKGBUILD + .desktop
  compositor/           Reglas para Hyprland y Sway
```
