import "./styles/base.css";
import "./styles/main.css";
import * as B from "./bridge";
import * as S from "./store";
import { applyAppearance, color, esc, icon, syncTicker } from "./ui";

const root = document.getElementById("app")!;

// Estado de interfaz (no se guarda)
let expanded: string | null = null;
let editingList: string | null = null;
let showDone = false;
let settingsOpen = false;
let confirmId: string | null = null;
let confirmTimer = 0;
let rendering = false;

// ───────────────────────── Vistas ─────────────────────────

const lights = `<div class="lights">
  <button class="light close" data-act="widget" title="Cerrar al widget"></button>
  <button class="light min" data-act="widget" title="Minimizar al widget (Ctrl+M)"></button>
  <button class="light max" data-act="max" title="Maximizar"></button>
</div>`;

function sidebar(s: S.State): string {
  const items = s.lists
    .map((l) => {
      const n = S.pending(s, l.id).length;
      const name =
        editingList === l.id
          ? `<input class="list-input" data-key="list:${l.id}" data-id="${l.id}" value="${esc(l.name)}" spellcheck="false">`
          : `<span class="list-name" data-id="${l.id}">${esc(l.name)}</span>`;
      const del =
        s.lists.length > 1
          ? `<button class="list-del${confirmId === l.id ? " confirm" : ""}" data-act="del-list" data-id="${l.id}" title="Eliminar lista">${confirmId === l.id ? "Eliminar" : icon("trash", 13)}</button>`
          : "";
      return `<div class="list-item${l.id === s.selectedListId ? " sel" : ""}" data-act="select-list" data-id="${l.id}">
        <span class="list-icon" style="--c:${color(l.color)}" data-act="cycle-color" data-id="${l.id}" title="Cambiar color">${icon("list", 12)}</span>
        ${name}<span class="list-count">${n || ""}</span>${del}
      </div>`;
    })
    .join("");
  return `<aside class="sidebar">
    <div class="titlebar drag">${lights}</div>
    <div class="side-label">Listas</div>
    <nav class="lists">${items}</nav>
    <div class="side-foot">
      <button class="ghost" data-act="new-list">${icon("plus", 14)}<span>Nueva lista</span></button>
      <button class="icon-btn" data-act="settings" title="Ajustes (Ctrl+,)">${icon("sliders", 16)}</button>
    </div>
  </aside>`;
}

function focusCard(s: S.State): string {
  const t = S.activeTask(s);
  if (!t) {
    return `<section class="focus-card empty">${icon("target", 16)}<span>Pulsa ◎ en una tarea para ponerla en foco. Se mostrará en el widget flotante.</span></section>`;
  }
  const list = s.lists.find((l) => l.id === t.listId);
  const running = !!s.active.since;
  return `<section class="focus-card${running ? " running" : ""}" style="--c:${color(list?.color ?? "")}">
    <div class="fc-info">
      <div class="fc-label"><span class="fc-dot"></span>En foco${list ? ` · ${esc(list.name)}` : ""}</div>
      <div class="fc-title">${esc(t.title) || "Sin título"}</div>
    </div>
    <div class="fc-timer" data-timer>${S.fmt(S.elapsed(s, t))}</div>
    <div class="fc-actions">
      <button class="round primary" data-act="timer" title="${running ? "Pausar" : "Iniciar"} (Espacio)">${icon(running ? "pause" : "play", 16)}</button>
      <button class="round" data-act="complete-active" title="Completar">${icon("check", 16)}</button>
      <button class="round" data-act="skip" title="Siguiente tarea">${icon("skip", 16)}</button>
    </div>
  </section>`;
}

function row(s: S.State, t: S.Task): string {
  const active = s.active.taskId === t.id;
  const open = expanded === t.id;
  const ms = S.elapsed(s, t);
  const preview = t.notes.split("\n")[0];
  const inotes = t.internalNotes;
  const del = confirmId === t.id;
  return `<div class="task${t.done ? " done" : ""}${active ? " active" : ""}${open ? " open" : ""}" data-id="${t.id}">
    <button class="check" data-act="check" data-id="${t.id}" aria-label="${t.done ? "Marcar pendiente" : "Completar"}">${icon("check", 11)}</button>
    <div class="task-main">
      <input class="task-title" data-key="title:${t.id}" data-id="${t.id}" value="${esc(t.title)}" placeholder="Sin título" spellcheck="false">
      ${
        open
          ? `<label class="task-code">
               <span>Código</span>
               <input data-key="code:${t.id}" data-id="${t.id}" value="${esc(t.code)}" placeholder="Código interno (p. ej. PRJ-142)" spellcheck="false" autocomplete="off" maxlength="40">
             </label>
             ${inotes.length ? internalNotes(t) : ""}
             ${inotes.length ? `<div class="reply-label">Tu respuesta</div>` : ""}
             <textarea class="task-notes" data-key="notes:${t.id}" data-id="${t.id}" placeholder="${inotes.length ? "Responder a las notas internas" : "Añadir notas"}" rows="3">${esc(t.notes)}</textarea>`
          : preview || t.code || inotes.length
            ? `<div class="task-preview">${t.code ? `<span class="code-chip">${esc(t.code)}</span>` : ""}${
                inotes.length ? `<span class="inote-badge" title="Notas internas">${icon("chat", 11)}${inotes.length}</span>` : ""
              }${esc(preview)}</div>`
            : ""
      }
    </div>
    <span class="task-time"${active ? " data-timer" : ""}>${ms >= 1000 ? S.fmt(ms) : ""}</span>
    <div class="task-tools">
      <button class="icon-btn${open || t.notes || t.code || inotes.length ? " on" : ""}" data-act="notes" data-id="${t.id}" title="Código y notas">${icon("notes", 15)}</button>
      ${t.done ? "" : `<button class="icon-btn${active ? " on" : ""}" data-act="focus" data-id="${t.id}" title="Poner en foco">${icon("target", 15)}</button>`}
      <button class="icon-btn del${del ? " confirm" : ""}" data-act="del-task" data-id="${t.id}" title="Eliminar">${del ? "Eliminar" : icon("trash", 15)}</button>
    </div>
  </div>`;
}

const when = (ms: number) =>
  new Date(ms).toLocaleString("es", { day: "numeric", month: "short", hour: "2-digit", minute: "2-digit" });

/** Apartado de notas internas (las que añade Claude por MCP). */
function internalNotes(t: S.Task): string {
  const items = t.internalNotes
    .map((n) => {
      const key = `inote:${n.id}`;
      const del = confirmId === key;
      return `<div class="inote">
        <div class="inote-meta"><span class="inote-author">${esc(n.author || "Nota")}</span> · ${esc(when(n.at))}
          <button class="inote-del${del ? " confirm" : ""}" data-act="del-inote" data-id="${t.id}" data-note="${esc(n.id)}" title="Eliminar nota">${del ? "Eliminar" : icon("x", 11)}</button>
        </div>
        <div class="inote-text">${esc(n.text)}</div>
      </div>`;
    })
    .join("");
  return `<div class="inotes"><div class="inotes-head">${icon("chat", 12)}Notas internas</div>${items}</div>`;
}

function content(s: S.State): string {
  const list = s.lists.find((l) => l.id === s.selectedListId);
  if (!list) return `<main class="content"><div class="head drag"></div><p class="empty-state">Crea una lista para empezar.</p></main>`;
  const all = S.listTasks(s, list.id);
  const todo = all.filter((t) => !t.done);
  const done = all.filter((t) => t.done).sort((a, b) => (b.doneAt ?? 0) - (a.doneAt ?? 0));
  const plural = (n: number, w: string) => `${n} ${w}${n === 1 ? "" : "s"}`;
  return `<main class="content">
    <header class="head drag">
      <div class="head-text drag">
        <h1 class="drag" style="color:${color(list.color)}">${esc(list.name)}</h1>
        <p class="sub drag">${plural(todo.length, "pendiente")}${done.length ? ` · ${plural(done.length, "completada")}` : ""}</p>
      </div>
      <button class="btn" data-act="widget" title="Minimizar al widget (Ctrl+M)">${icon("pip", 15)}<span>Widget</span></button>
    </header>
    <div class="scroll">
      ${focusCard(s)}
      <label class="add-row">
        <span class="add-icon">${icon("plus", 13)}</span>
        <input class="add-input" data-key="add" placeholder="Nueva tarea" spellcheck="false" autocomplete="off">
      </label>
      <div class="tasks">${todo.map((t) => row(s, t)).join("")}</div>
      ${todo.length === 0 ? `<p class="empty-state">Todo listo por aquí.</p>` : ""}
      ${
        done.length
          ? `<button class="done-toggle${showDone ? " open" : ""}" data-act="toggle-done">${icon("chevron", 12)}Completadas<span>${done.length}</span></button>
             ${showDone ? `<div class="tasks">${done.map((t) => row(s, t)).join("")}</div>` : ""}`
          : ""
      }
    </div>
  </main>`;
}

const scaleLabel = (v: number) => `${Math.round(v * 100)} % · ${Math.round(320 * v)}×${Math.round(84 * v)}`;

function settings(s: S.State): string {
  const st = s.settings;
  const seg = (v: S.Settings["theme"], label: string) =>
    `<button class="${st.theme === v ? "on" : ""}" data-act="set-theme" data-id="${v}">${label}</button>`;
  return `<div class="sheet-backdrop" data-act="close-settings">
    <div class="sheet glass" role="dialog" aria-label="Ajustes">
      <h2>Ajustes</h2>
      <div class="set-row">
        <div><div class="set-title">Widget automático</div><div class="set-desc">Aparece al cambiar de ventana o de escritorio, como el PiP de Meet. No aparece mientras Foco siga a la vista, por ejemplo en otro monitor.</div></div>
        <button class="switch${st.autoWidget ? " on" : ""}" data-act="set-auto" role="switch" aria-checked="${st.autoWidget}"><span></span></button>
      </div>
      <div class="set-row col">
        <div class="set-head"><span class="set-title">Tamaño del widget</span><span class="set-value" data-scale-label>${scaleLabel(st.widgetScale)}</span></div>
        <input class="slider" type="range" data-key="widget-scale" min="${S.WIDGET_SCALE.min}" max="${S.WIDGET_SCALE.max}" step="${S.WIDGET_SCALE.step}" value="${st.widgetScale}">
        <div class="set-desc">También con <kbd>Ctrl</kbd> + rueda del ratón sobre el widget.</div>
      </div>
      <div class="set-row">
        <div class="set-title">Apariencia</div>
        <div class="segmented">${seg("system", "Sistema")}${seg("light", "Claro")}${seg("dark", "Oscuro")}</div>
      </div>
      <div class="set-row col">
        <div class="set-title">Opacidad del vidrio</div>
        <input class="slider" type="range" data-key="glass" min="0.35" max="0.98" step="0.01" value="${st.glass}">
        <div class="set-desc">Súbela si tu compositor no desenfoca el fondo (Sway, GNOME).</div>
      </div>
      <div class="set-row col">
        <div class="set-title">Atajos</div>
        <div class="keys">
          <span><kbd>Ctrl</kbd><kbd>N</kbd> Nueva tarea</span>
          <span><kbd>Ctrl</kbd><kbd>M</kbd> Widget</span>
          <span><kbd>Espacio</kbd> Iniciar/pausar</span>
          <span><kbd>Ctrl</kbd><kbd>Q</kbd> Salir</span>
        </div>
        <div class="set-desc">Desde el compositor: <code>foco --toggle</code> alterna ventana y widget.</div>
      </div>
      <div class="sheet-foot">
        <button class="btn danger" data-act="quit">Salir de Foco</button>
        <button class="btn primary" data-act="close-settings">Listo</button>
      </div>
    </div>
  </div>`;
}

function render(): void {
  const s = S.get();
  applyAppearance(s);

  // Conservar foco, cursor y scroll entre repintados
  const ae = document.activeElement as HTMLInputElement | null;
  const key = ae?.dataset?.key;
  let sel: [number | null, number | null] | null = null;
  try {
    if (key && ae && ae.type !== "range") sel = [ae.selectionStart, ae.selectionEnd];
  } catch {
    /* elemento sin selección */
  }
  const scroll = root.querySelector(".scroll")?.scrollTop ?? 0;

  rendering = true;
  root.innerHTML = `<div class="window">${sidebar(s)}${content(s)}${settingsOpen ? settings(s) : ""}</div>`;
  rendering = false;

  const sc = root.querySelector(".scroll");
  if (sc) sc.scrollTop = scroll;
  if (key) {
    const el = root.querySelector<HTMLInputElement>(`[data-key="${key}"]`);
    if (el) {
      el.focus({ preventScroll: true });
      if (sel && sel[0] !== null) el.setSelectionRange(sel[0], sel[1]);
    }
  }
  syncTicker(!!s.active.since, tickTimers);
}

function tickTimers(): void {
  const s = S.get();
  const t = S.activeTask(s);
  if (!t) return;
  const txt = S.fmt(S.elapsed(s, t));
  root.querySelectorAll("[data-timer]").forEach((n) => (n.textContent = txt));
}

// ───────────────────────── Acciones ─────────────────────────

function focusKey(key: string, select = false): void {
  const el = root.querySelector<HTMLInputElement>(`[data-key="${key}"]`);
  el?.focus();
  if (select) el?.select();
}

function confirmThen(id: string, action: () => void): void {
  clearTimeout(confirmTimer);
  if (confirmId === id) {
    confirmId = null;
    action();
    return;
  }
  confirmId = id;
  confirmTimer = window.setTimeout(() => {
    confirmId = null;
    render();
  }, 2500);
  render();
}

async function goWidget(): Promise<void> {
  await S.flush();
  B.showWidget();
}

async function quit(): Promise<void> {
  await S.update(S.pause, { render: false });
  B.quit();
}

const isTyping = (t: EventTarget | null) =>
  t instanceof HTMLElement && (t.tagName === "INPUT" || t.tagName === "TEXTAREA") && (t as HTMLInputElement).type !== "range";

root.addEventListener("click", (e) => {
  const target = e.target as HTMLElement;
  const el = target.closest<HTMLElement>("[data-act]");
  if (!el) {
    // Clic en el cuerpo de una tarea (no en el texto): abre/cierra notas
    const task = target.closest<HTMLElement>(".task");
    if (task && !target.closest("input, textarea, .task-code, .inotes")) {
      expanded = expanded === task.dataset.id ? null : task.dataset.id!;
      render();
    }
    return;
  }
  const id = el.dataset.id ?? "";
  switch (el.dataset.act) {
    case "widget":
      return void goWidget();
    case "max":
      return B.toggleMaximize();
    case "select-list":
      if (editingList === id) return;
      expanded = null;
      return void S.update((s) => void (s.selectedListId = id));
    case "cycle-color":
      return void S.update((s) => {
        const l = s.lists.find((x) => x.id === id);
        if (l) S.cycleColor(l);
      });
    case "new-list": {
      let nid = "";
      void S.update((s) => void (nid = S.addList(s).id), { render: false });
      editingList = nid;
      render();
      return focusKey(`list:${nid}`, true);
    }
    case "del-list":
      return confirmThen(id, () => void S.update((s) => S.removeList(s, id)));
    case "check":
      return void S.update((s) => S.complete(s, id));
    case "focus":
      return void S.update((s) => {
        if (s.active.taskId === id) S.toggle(s);
        else S.focusTask(s, id, true);
      });
    case "notes":
      expanded = expanded === id ? null : id;
      render();
      if (expanded) focusKey(`notes:${id}`);
      return;
    case "del-task":
      return confirmThen(id, () => void S.update((s) => S.removeTask(s, id)));
    case "del-inote": {
      const note = el.dataset.note ?? "";
      return confirmThen(`inote:${note}`, () => void S.update((s) => S.removeInternalNote(s, id, note)));
    }
    case "timer":
      return void S.update(S.toggle);
    case "complete-active":
      return void S.update((s) => {
        const t = S.activeTask(s);
        if (t) S.complete(s, t.id);
      });
    case "skip":
      return void S.update(S.skip);
    case "toggle-done":
      showDone = !showDone;
      return render();
    case "settings":
      settingsOpen = true;
      return render();
    case "close-settings":
      if (el.classList.contains("sheet-backdrop") && target !== el) return;
      settingsOpen = false;
      return render();
    case "set-auto":
      return void S.update((s) => void (s.settings.autoWidget = !s.settings.autoWidget));
    case "set-theme":
      return void S.update((s) => void (s.settings.theme = id as S.Settings["theme"]));
    case "quit":
      return void quit();
  }
});

root.addEventListener("dblclick", (e) => {
  const name = (e.target as HTMLElement).closest<HTMLElement>(".list-name");
  if (name?.dataset.id) {
    editingList = name.dataset.id;
    render();
    focusKey(`list:${editingList}`, true);
  }
});

// Arrastrar la ventana desde la barra de título / cabecera
root.addEventListener("mousedown", (e) => {
  const t = e.target as HTMLElement;
  if (e.button === 0 && t.closest(".drag") && !t.closest("button, input, textarea, [data-act]")) B.startDrag();
});

root.addEventListener("input", (e) => {
  const el = e.target as HTMLInputElement;
  const key = el.dataset.key ?? "";
  const id = el.dataset.id ?? "";
  const setField = (field: "title" | "code" | "notes") =>
    void S.update(
      (s) => {
        const t = S.findTask(s, id);
        if (t) t[field] = el.value;
      },
      { render: false, debounce: true },
    );
  if (key.startsWith("title:")) {
    setField("title");
    if (S.get().active.taskId === id) {
      const fc = root.querySelector(".fc-title");
      if (fc) fc.textContent = el.value || "Sin título";
    }
  } else if (key.startsWith("code:")) {
    setField("code");
  } else if (key.startsWith("notes:")) {
    setField("notes");
  } else if (key === "glass") {
    void S.update((s) => void (s.settings.glass = Number(el.value)), { render: false, debounce: true });
    applyAppearance(S.get());
  } else if (key === "widget-scale") {
    const v = S.clampScale(Number(el.value));
    void S.update((s) => void (s.settings.widgetScale = v), { render: false, debounce: true });
    const label = root.querySelector("[data-scale-label]");
    if (label) label.textContent = scaleLabel(v);
  }
});

root.addEventListener("keydown", (e) => {
  const el = e.target as HTMLInputElement;
  const key = el.dataset?.key ?? "";
  if (key === "add" && e.key === "Enter") {
    e.preventDefault();
    const title = el.value.trim();
    const listId = S.get().selectedListId;
    if (!title || !listId) return;
    el.value = "";
    void S.update((s) => S.addTask(s, listId, title));
  } else if (key.startsWith("code:") && e.key === "Enter") {
    e.preventDefault();
    focusKey(`notes:${key.slice(5)}`);
  } else if ((key.startsWith("title:") || key.startsWith("code:")) && (e.key === "Enter" || e.key === "Escape")) {
    e.preventDefault();
    e.stopPropagation();
    el.blur();
  } else if (key.startsWith("list:")) {
    if (e.key === "Enter") {
      e.preventDefault();
      el.blur();
    } else if (e.key === "Escape") {
      e.stopPropagation();
      editingList = null;
      render();
    }
  }
});

root.addEventListener("focusout", (e) => {
  if (rendering) return;
  const el = e.target as HTMLInputElement;
  const key = el.dataset?.key ?? "";
  const id = el.dataset?.id ?? "";
  if (key.startsWith("list:") && editingList === id) {
    editingList = null;
    void S.update((s) => {
      const l = s.lists.find((x) => x.id === id);
      if (l) l.name = el.value.trim() || "Sin título";
    });
  } else if (key.startsWith("title:") || key.startsWith("code:") || key.startsWith("notes:")) {
    void S.flush();
  }
});

document.addEventListener("keydown", (e) => {
  const mod = e.ctrlKey || e.metaKey;
  const k = e.key.toLowerCase();
  if (mod && k === "n") {
    e.preventDefault();
    settingsOpen = false;
    render();
    focusKey("add");
  } else if (mod && (k === "m" || k === "w")) {
    e.preventDefault();
    void goWidget();
  } else if (mod && k === "q") {
    e.preventDefault();
    void quit();
  } else if (mod && k === ",") {
    e.preventDefault();
    settingsOpen = !settingsOpen;
    render();
  } else if (e.key === "Escape") {
    if (settingsOpen) settingsOpen = false;
    else if (expanded) expanded = null;
    else return (document.activeElement as HTMLElement | null)?.blur();
    render();
  } else if (e.key === " " && !isTyping(e.target) && !mod) {
    e.preventDefault();
    void S.update(S.toggle);
  }
});

// ───────────────────────── Arranque ─────────────────────────

// Punto de control cada 5 min mientras el cronómetro corre: si el equipo se
// apaga sin cerrar Foco, como mucho se pierden esos minutos (Rust pausa al arrancar).
window.setInterval(() => {
  if (S.get().active.since)
    void S.update(
      (s) => {
        S.pause(s);
        S.play(s);
      },
      { render: false },
    );
}, 5 * 60_000);

S.subscribe(render);
void S.init().then(render);
