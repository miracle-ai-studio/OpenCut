import { defineConfig, type ProxyOptions } from "vite";
import react from "@vitejs/plugin-react";
const target = process.env.OPENCUT_SERVER_URL ?? "http://127.0.0.1:8788";
const proxy: ProxyOptions = {
  target,
  changeOrigin: true,
  configure(proxy) {
    proxy.on("proxyReq", (request, incoming) => {
      if (
        ["http://127.0.0.1:5173", "http://localhost:5173"].includes(
          String(incoming.headers.origin),
        )
      ) {
        request.setHeader("origin", new URL(target).origin);
        request.setHeader("sec-fetch-site", "same-origin");
      }
    });
  },
};
export default defineConfig({
  plugins: [react()],
  resolve: {
    alias: [
      "fullscreen",
      "layer",
      "blend",
      "mask",
      "jfa_init",
      "jfa_step",
      "jfa_distance",
        "gaussian_blur",
    ].map((name) => ({
      find: `/renderer/vendor/classic-v31/${name}.wgsl`,
      replacement:
        new URL(
          `../../packages/classic-visual-renderer/vendor/classic-v31/${name}.wgsl`,
          import.meta.url,
        ).pathname + "?raw",
    })),
  },
  server: {
    host: "127.0.0.1",
    port: 5173,
    strictPort: true,
    // Vite discovers imported renderer modules, but the worker URL is fetched
    // separately. Allow that one source file without exposing the repository.
    fs: {
      allow: [
        new URL(".", import.meta.url).pathname,
        new URL("../../packages/classic-visual-renderer/src/font-hash-worker.mjs", import.meta.url).pathname,
      ],
    },
    proxy: { "/api": proxy, "/app": proxy },
  },
  build: { outDir: "dist", emptyOutDir: true, sourcemap: false },
});
