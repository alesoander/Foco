// Puente con Rust. Fuera de Tauri (navegador) usa localStorage +
// BroadcastChannel, así la interfaz se puede previsualizar con `npm run dev`.
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";

export const isTauri = "__TAURI_INTERNALS__" in window;
if (!isTauri) document.documentElement.classList.add("web");

const KEY = "foco-state";
const chan = !isTauri && "BroadcastChannel" in window ? new BroadcastChannel("foco") : null;

export async function loadRaw(): Promise<string> {
  if (isTauri) return invoke<string>("get_state");
  try {
    return localStorage.getItem(KEY) ?? "";
  } catch {
    return "";
  }
}

export async function saveRaw(json: string): Promise<void> {
  if (isTauri) return invoke("set_state", { state: json });
  try {
    localStorage.setItem(KEY, json);
  } catch {
    /* sin almacenamiento: seguimos en memoria */
  }
  chan?.postMessage(json);
}

export function onRemote(cb: (json: string) => void): void {
  if (isTauri) void listen<string>("state", (e) => cb(e.payload));
  else chan?.addEventListener("message", (e) => cb(String(e.data)));
}

export function showWidget(): void {
  if (isTauri) void invoke("show_widget");
  else window.open("widget.html", "foco-widget", "width=320,height=84");
}

export function showMain(): void {
  if (isTauri) void invoke("show_main");
  else window.opener?.focus();
}

export function quit(): void {
  if (isTauri) void invoke("quit");
}

export function startDrag(): void {
  if (isTauri) void getCurrentWindow().startDragging();
}

export function toggleMaximize(): void {
  if (isTauri) void getCurrentWindow().toggleMaximize();
}
