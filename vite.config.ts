import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import { fileURLToPath } from "node:url";

// @ts-expect-error process is a nodejs global
const host = process.env.TAURI_DEV_HOST;

// https://vite.dev/config/
export default defineConfig(async () => ({
  plugins: [react(), tailwindcss()],

  resolve: {
    alias: {
      "@": fileURLToPath(new URL("./src", import.meta.url)),
    },
  },

  // V0.3 D1 · 预打包 Milkdown 子包,避免 `pnpm tauri dev` 冷启动时
  // 按需发现 ESM 子路径触发整页 reload / 偶发 interop 报错。
  optimizeDeps: {
    include: [
      "@milkdown/kit/core",
      "@milkdown/kit/preset/commonmark",
      "@milkdown/kit/preset/gfm",
      "@milkdown/kit/plugin/history",
      "@milkdown/kit/plugin/listener",
      "@milkdown/kit/utils",
      "@milkdown/react",
      "@milkdown/theme-nord",
    ],
  },

  // Vite options tailored for Tauri development and only applied in `tauri dev` or `tauri build`
  //
  // 1. prevent Vite from obscuring rust errors
  clearScreen: false,
  // 2. tauri expects a fixed port, fail if that port is not available
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      // 3. tell Vite to ignore watching `src-tauri`
      ignored: ["**/src-tauri/**"],
    },
  },
}));
