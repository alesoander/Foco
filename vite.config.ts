import { defineConfig } from "vite";
// @ts-expect-error tipos de node no instalados (no hacen falta)
import process from "node:process";

const host = process.env.TAURI_DEV_HOST;

export default defineConfig({
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host ? { protocol: "ws", host, port: 1421 } : undefined,
    watch: { ignored: ["**/src-tauri/**"] },
  },
  build: {
    // WebKitGTK reciente: no hace falta transpilar a ES5
    target: "safari16",
    cssMinify: true,
    // Dos páginas: ventana principal y widget flotante
    rollupOptions: {
      input: { main: "index.html", widget: "widget.html" },
    },
  },
});
