import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import { fileURLToPath, URL } from "node:url";

// https://vite.dev/config/
export default defineConfig({
  plugins: [react(), tailwindcss()],

  // Rust compile errors scroll past if Vite clears the screen.
  clearScreen: false,

  server: {
    port: 1420,
    // tauri.conf.json hardcodes devUrl at 1420; a silent port bump would
    // leave the webview pointed at nothing.
    strictPort: true,
    watch: { ignored: ["**/src-tauri/**", "**/crates/**", "**/target/**"] },
  },

  envPrefix: ["VITE_", "TAURI_ENV_"],

  build: {
    // WebView2 floor on supported Windows versions.
    target: "chrome105",
    minify: process.env.TAURI_ENV_DEBUG ? false : "esbuild",
    sourcemap: !!process.env.TAURI_ENV_DEBUG,
  },

  resolve: {
    alias: { "@": fileURLToPath(new URL("./src", import.meta.url)) },
  },
});
