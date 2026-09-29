import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// https://vitejs.dev/config/
export default defineConfig({
  plugins: [react()],

  // Vite options tailored for Tauri development
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
  },
  envPrefix: ["VITE_", "TAURI_"],
  build: {
    target: "es2021",
    outDir: "dist",
    // Vite 8 起不再内置 esbuild（改为可选 peer 依赖），minify 用默认的 oxc 压缩器：
    // minify: true 会被规范化为 "oxc"，写 "esbuild" 会去加载未安装的 esbuild 而失败
    minify: !process.env.TAURI_DEBUG,
    sourcemap: !!process.env.TAURI_DEBUG,
  },
});
