import type { State } from "./store";

const ESC: Record<string, string> = { "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" };
export const esc = (s: string) => String(s).replace(/[&<>"']/g, (c) => ESC[c]);

/** Solo acepta colores hex (el archivo de estado podría editarse a mano). */
export const color = (c: string) => (/^#[0-9a-f]{3,8}$/i.test(c) ? c : "var(--accent)");

export function applyAppearance(s: State): void {
  const root = document.documentElement;
  if (s.settings.theme === "system") delete root.dataset.theme;
  else root.dataset.theme = s.settings.theme;
  root.style.setProperty("--glass-a", String(s.settings.glass));
}

// Un único intervalo de 1 s, activo solo mientras el cronómetro corre.
let interval = 0;
export function syncTicker(running: boolean, fn: () => void): void {
  if (running && !interval) interval = window.setInterval(fn, 1000);
  else if (!running && interval) {
    clearInterval(interval);
    interval = 0;
  }
}

const PATHS: Record<string, string> = {
  plus: '<path d="M12 5v14M5 12h14"/>',
  check: '<path d="M5 12.5l4.5 4.5L19 7.5"/>',
  play: '<path d="M8 5.8v12.4a1 1 0 0 0 1.5.86l10-6.2a1 1 0 0 0 0-1.72l-10-6.2A1 1 0 0 0 8 5.8z" fill="currentColor" stroke="none"/>',
  pause:
    '<rect x="6.5" y="5" width="4" height="14" rx="1.3" fill="currentColor" stroke="none"/><rect x="13.5" y="5" width="4" height="14" rx="1.3" fill="currentColor" stroke="none"/>',
  skip: '<path d="M5 6.3v11.4a1 1 0 0 0 1.5.85l8.6-5.7a1 1 0 0 0 0-1.7L6.5 5.45A1 1 0 0 0 5 6.3z" fill="currentColor" stroke="none"/><path d="M19 5.5v13"/>',
  expand: '<path d="M14 4h6v6M10 20H4v-6M20 4l-6.5 6.5M4 20l6.5-6.5"/>',
  pip: '<rect x="3" y="5" width="18" height="14" rx="3"/><rect x="11.5" y="11.5" width="7" height="5" rx="1.3" fill="currentColor" stroke="none"/>',
  trash: '<path d="M4 7h16M10 11v6M14 11v6M6 7l.9 11.2A2 2 0 0 0 8.9 20h6.2a2 2 0 0 0 2-1.8L18 7M9 7V4.5h6V7"/>',
  target: '<circle cx="12" cy="12" r="8"/><circle cx="12" cy="12" r="3" fill="currentColor" stroke="none"/>',
  sliders: '<path d="M4 7h9M17 7h3M4 17h3M11 17h9"/><circle cx="15" cy="7" r="2"/><circle cx="9" cy="17" r="2"/>',
  list: '<path d="M9 7h10M9 12h10M9 17h10"/><circle cx="5" cy="7" r=".6"/><circle cx="5" cy="12" r=".6"/><circle cx="5" cy="17" r=".6"/>',
  notes: '<path d="M5 6.5h14M5 11.5h14M5 16.5h9"/>',
  chevron: '<path d="M9 6l6 6-6 6"/>',
  chat: '<path d="M5 5.5h14a1.5 1.5 0 0 1 1.5 1.5v8.5A1.5 1.5 0 0 1 19 17h-7.5L7 20.5V17H5a1.5 1.5 0 0 1-1.5-1.5V7A1.5 1.5 0 0 1 5 5.5z"/>',
  x: '<path d="M7 7l10 10M17 7L7 17"/>',
};

export function icon(name: keyof typeof PATHS, size = 16): string {
  return `<svg class="i" width="${size}" height="${size}" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">${PATHS[name]}</svg>`;
}
