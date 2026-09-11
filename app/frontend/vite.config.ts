import { defineConfig } from "vite";
import vue from "@vitejs/plugin-vue";

// Tauri 约定：固定端口且不自动换端口，devUrl 才能稳定指向它。
export default defineConfig({
  plugins: [vue()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
  },
  build: {
    // 桌面端只跑一个 WebView，不需要为老浏览器降级。
    target: "chrome120",
  },
});
