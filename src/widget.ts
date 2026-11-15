// Widget flotante (estilo PiP): muestra la tarea en foco y su cronómetro.
import "./styles/base.css";
import "./styles/widget.css";
import * as B from "./bridge";
import * as S from "./store";
import { applyAppearance, color, esc, icon, syncTicker } from "./ui";

const root = document.getElementById("app")!;
let appliedScale = 0;

/** Tarea en foco o, si no hay, la siguiente pendiente para empezar con un clic. */
function candidate(s: S.State): S.Task | null {
  const t = S.activeTask(s);
  if (t) return t;
  const fromList = s.selectedListId ? S.pending(s, s.selectedListId)[0] : undefined;
  return fromList ?? s.tasks.find((x) => !x.done) ?? null;
}

function render(): void {
  const s = S.get();
  applyAppearance(s);
  if (s.settings.widgetScale !== appliedScale) {
    appliedScale = s.settings.widgetScale;
    B.setWidgetScale(appliedScale);
  }
  const active = S.activeTask(s);
  const t = candidate(s);
  const list = t ? s.lists.find((l) => l.id === t.listId) : undefined;
  const running = !!(active && s.active.since);

  const label = active ? esc(list?.name ?? "") : t ? "Siguiente" : "Foco";
  const title = t ? esc(t.title) || "Sin título" : "Nada pendiente";
  const playAct = active ? "timer" : t ? "start" : "open";
  const playTitle = running ? "Pausar" : active ? "Reanudar" : t ? "Empezar" : "Abrir Foco";

  root.innerHTML = `<div class="pip${running ? " running" : ""}${active ? "" : " idle"}" style="--c:${color(list?.color ?? "")}">
    <button class="pip-play" data-act="${playAct}" title="${playTitle}">${icon(running ? "pause" : "play", 18)}</button>
    <div class="pip-meta">
      <div class="pip-list">${label}</div>
      <div class="pip-title">${title}</div>
    </div>
    <div class="pip-time" data-timer>${active ? S.fmt(S.elapsed(s, active)) : ""}</div>
    <div class="pip-actions">
      ${
        active
          ? `<button data-act="complete" title="Completar">${icon("check", 15)}</button>
             <button data-act="skip" title="Siguiente tarea">${icon("skip", 15)}</button>`
          : ""
      }
      <button data-act="open" title="Abrir Foco">${icon("expand", 15)}</button>
    </div>
  </div>`;
  syncTicker(running, tick);
}

function tick(): void {
  const s = S.get();
  const t = S.activeTask(s);
  const el = root.querySelector("[data-timer]");
  if (t && el) el.textContent = S.fmt(S.elapsed(s, t));
}

root.addEventListener("click", (e) => {
  const el = (e.target as HTMLElement).closest<HTMLElement>("[data-act]");
  if (!el) return;
  switch (el.dataset.act) {
    case "timer":
      return void S.update(S.toggle);
    case "start": {
      const t = candidate(S.get());
      if (t) void S.update((s) => S.focusTask(s, t.id, true));
      return;
    }
    case "complete":
      return void S.update((s) => {
        const t = S.activeTask(s);
        if (t) S.complete(s, t.id);
      });
    case "skip":
      return void S.update(S.skip);
    case "open":
      return B.showMain();
  }
});

// Todo el widget es arrastrable salvo los botones; doble clic abre la app.
root.addEventListener("mousedown", (e) => {
  if (e.button === 0 && !(e.target as HTMLElement).closest("button")) B.startDrag();
});
root.addEventListener("dblclick", (e) => {
  if (!(e.target as HTMLElement).closest("button")) B.showMain();
});

// Ctrl + rueda: cambia el tamaño del widget (se guarda en Ajustes).
// Un paso por movimiento de rueda (cada motor web da un `deltaY` distinto:
// WebKitGTK ≈ 14 px, Chromium 100 px); el límite de frecuencia evita que el
// flujo continuo de un touchpad lo dispare.
let lastWheel = 0;
root.addEventListener(
  "wheel",
  (e) => {
    if (!e.ctrlKey) return;
    e.preventDefault();
    if (!e.deltaY || e.timeStamp - lastWheel < 90) return;
    lastWheel = e.timeStamp;
    const step = e.deltaY < 0 ? S.WIDGET_SCALE.step : -S.WIDGET_SCALE.step;
    const cur = S.get().settings.widgetScale;
    const next = S.clampScale(cur + step);
    if (next === cur) return;
    appliedScale = next;
    B.setWidgetScale(next);
    void S.update((s) => void (s.settings.widgetScale = next), { render: false, debounce: true });
  },
  { passive: false },
);

S.subscribe(render);
void S.init().then(render);
