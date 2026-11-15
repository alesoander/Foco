// Modelo de datos y operaciones, compartido por la ventana principal y el widget.
import * as B from "./bridge";

export type Task = {
  id: string;
  listId: string;
  title: string;
  /** Código interno opcional (p. ej. "PRJ-142"). */
  code: string;
  notes: string;
  done: boolean;
  createdAt: number;
  doneAt: number | null;
  /** Tiempo acumulado en foco (ms), sin contar el tramo en curso. */
  spentMs: number;
  order: number;
};

export type List = { id: string; name: string; color: string };

export type Settings = {
  autoWidget: boolean;
  glass: number;
  theme: "system" | "light" | "dark";
  /** Escala del widget: 1 = 320×84. */
  widgetScale: number;
};

export const WIDGET_SCALE = { min: 0.6, max: 1.4, step: 0.05 };
export const clampScale = (v: number) =>
  Number.isFinite(v)
    ? Math.min(WIDGET_SCALE.max, Math.max(WIDGET_SCALE.min, +(Math.round(v / WIDGET_SCALE.step) * WIDGET_SCALE.step).toFixed(2)))
    : 1;

export type State = {
  v: 1;
  rev: number;
  lists: List[];
  tasks: Task[];
  selectedListId: string | null;
  /** Tarea en foco; `since` != null mientras el cronómetro corre. */
  active: { taskId: string | null; since: number | null };
  settings: Settings;
};

// Colores del sistema de Apple
export const COLORS = ["#0a84ff", "#ff9f0a", "#30d158", "#bf5af2", "#ff375f", "#64d2ff", "#ffd60a", "#8e8e93"];

export const uid = () => Date.now().toString(36) + Math.random().toString(36).slice(2, 8);

function task(listId: string, title: string, order: number): Task {
  return { id: uid(), listId, title, code: "", notes: "", done: false, createdAt: Date.now(), doneAt: null, spentMs: 0, order };
}

export function defaults(): State {
  const id = uid();
  return {
    v: 1,
    rev: 0,
    lists: [{ id, name: "Personal", color: COLORS[0] }],
    tasks: [
      task(id, "Bienvenido a Foco", 0),
      task(id, "Pulsa ◎ en una tarea para ponerla en foco", 1),
      task(id, "Usa el botón amarillo para minimizar al widget", 2),
    ],
    selectedListId: id,
    active: { taskId: null, since: null },
    settings: { autoWidget: true, glass: 0.72, theme: "system", widgetScale: 1 },
  };
}

export function normalize(raw: unknown): State {
  const d = defaults();
  if (!raw || typeof raw !== "object") return d;
  const r = raw as Partial<State>;
  if (!Array.isArray(r.lists) || !Array.isArray(r.tasks)) return d;
  const settings = { ...d.settings, ...(r.settings ?? {}) };
  settings.widgetScale = clampScale(Number(settings.widgetScale));
  return {
    v: 1,
    rev: Number(r.rev) || 0,
    lists: r.lists,
    // Las tareas creadas antes de existir el código no lo tienen.
    tasks: r.tasks.map((t) => ({ ...t, code: typeof t.code === "string" ? t.code : "" })),
    selectedListId: r.selectedListId ?? r.lists[0]?.id ?? null,
    active: { taskId: r.active?.taskId ?? null, since: r.active?.since ?? null },
    settings,
  };
}

// ───────────── Estado en memoria + sincronización ─────────────

let state: State = defaults();
const subs = new Set<() => void>();
let saveTimer = 0;

export const get = () => state;
export const subscribe = (fn: () => void) => void subs.add(fn);
const notify = () => subs.forEach((fn) => fn());

export async function init(): Promise<void> {
  let raw = "";
  try {
    raw = await B.loadRaw();
  } catch (e) {
    console.error(e);
  }
  let parsed: unknown = null;
  try {
    parsed = raw ? JSON.parse(raw) : null;
  } catch {
    /* archivo corrupto: empezamos de cero */
  }
  state = normalize(parsed);
  if (!raw) void persist();
  B.onRemote((json) => {
    try {
      const next = normalize(JSON.parse(json));
      if (next.rev > state.rev) {
        state = next;
        notify();
      }
    } catch {
      /* ignorar */
    }
  });
}

function persist(): Promise<void> {
  clearTimeout(saveTimer);
  saveTimer = 0;
  return B.saveRaw(JSON.stringify(state)).catch((e) => console.error(e));
}

/**
 * Aplica un cambio. `render: false` evita repintar (p. ej. mientras se escribe)
 * y `debounce` agrupa escrituras a disco.
 */
export function update(fn: (s: State) => void, opts: { render?: boolean; debounce?: boolean } = {}): Promise<void> {
  fn(state);
  state.rev++;
  let p: Promise<void>;
  if (opts.debounce) {
    clearTimeout(saveTimer);
    saveTimer = window.setTimeout(persist, 400);
    p = Promise.resolve();
  } else p = persist();
  if (opts.render !== false) notify();
  return p;
}

export const flush = () => (saveTimer ? persist() : Promise.resolve());
window.addEventListener("beforeunload", () => void flush());

// ───────────────────────── Consultas ─────────────────────────

export const listTasks = (s: State, listId: string) =>
  s.tasks.filter((t) => t.listId === listId).sort((a, b) => a.order - b.order);
export const pending = (s: State, listId: string) => listTasks(s, listId).filter((t) => !t.done);
export const activeTask = (s: State) => s.tasks.find((t) => t.id === s.active.taskId) ?? null;
export const findTask = (s: State, id: string) => s.tasks.find((t) => t.id === id);

export function elapsed(s: State, t: Task): number {
  const live = s.active.taskId === t.id && s.active.since ? Math.max(0, Date.now() - s.active.since) : 0;
  return t.spentMs + live;
}

export function fmt(ms: number): string {
  const total = Math.floor(ms / 1000);
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  const sec = total % 60;
  const p = (n: number) => String(n).padStart(2, "0");
  return h ? `${h}:${p(m)}:${p(sec)}` : `${p(m)}:${p(sec)}`;
}

// ───────────────────────── Operaciones ─────────────────────────

export function pause(s: State): void {
  const t = activeTask(s);
  if (t && s.active.since) t.spentMs += Math.max(0, Date.now() - s.active.since);
  s.active.since = null;
}

export function play(s: State): void {
  if (activeTask(s) && !s.active.since) s.active.since = Date.now();
}

export function toggle(s: State): void {
  if (s.active.since) pause(s);
  else play(s);
}

export function focusTask(s: State, id: string, run = true): void {
  if (s.active.taskId === id) {
    if (run) play(s);
    return;
  }
  pause(s);
  s.active = { taskId: id, since: run ? Date.now() : null };
}

/** Siguiente tarea pendiente de la misma lista (circular). */
export function nextAfter(s: State, t: Task): Task | null {
  const rest = pending(s, t.listId).filter((x) => x.id !== t.id);
  return rest.find((x) => x.order > t.order) ?? rest[0] ?? null;
}

export function complete(s: State, id: string): void {
  const t = findTask(s, id);
  if (!t) return;
  if (t.done) {
    t.done = false;
    t.doneAt = null;
    return;
  }
  const wasActive = s.active.taskId === id;
  const running = wasActive && !!s.active.since;
  if (wasActive) pause(s);
  t.done = true;
  t.doneAt = Date.now();
  if (wasActive) {
    const n = nextAfter(s, t);
    s.active = { taskId: n?.id ?? null, since: n && running ? Date.now() : null };
  }
}

export function skip(s: State): void {
  const t = activeTask(s);
  if (!t) return;
  const n = nextAfter(s, t);
  if (n) focusTask(s, n.id, !!s.active.since);
}

export function addTask(s: State, listId: string, title: string): void {
  const order = Math.max(-1, ...s.tasks.filter((t) => t.listId === listId).map((t) => t.order)) + 1;
  s.tasks.push(task(listId, title, order));
}

export function removeTask(s: State, id: string): void {
  if (s.active.taskId === id) s.active = { taskId: null, since: null };
  s.tasks = s.tasks.filter((t) => t.id !== id);
}

export function addList(s: State): List {
  const l: List = { id: uid(), name: "Nueva lista", color: COLORS[s.lists.length % COLORS.length] };
  s.lists.push(l);
  s.selectedListId = l.id;
  return l;
}

export function removeList(s: State, id: string): void {
  if (s.lists.length <= 1) return;
  if (activeTask(s)?.listId === id) s.active = { taskId: null, since: null };
  s.tasks = s.tasks.filter((t) => t.listId !== id);
  s.lists = s.lists.filter((l) => l.id !== id);
  if (s.selectedListId === id) s.selectedListId = s.lists[0].id;
}

export function cycleColor(l: List): void {
  l.color = COLORS[(COLORS.indexOf(l.color) + 1) % COLORS.length];
}
