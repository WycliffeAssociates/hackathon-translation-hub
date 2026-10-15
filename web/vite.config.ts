import { defineConfig } from "vite";
import solid from "vite-plugin-solid";

// The USFM viewer is Sefer's editor, installed as packages (`@sefer/editor`,
// `@sefer/text`; TypeScript source, compiled here by Vite). Their peers
// (CodeMirror, effect, scripture-kitchen) are this app's own dependencies, so
// there is one copy of each: two copies of CodeMirror's state would split the
// lint state and the rail would see nothing.

const api = process.env.HUB_API ?? "http://127.0.0.1:8788";

export default defineConfig({
  plugins: [solid()],
  server: {
    proxy: { "/v1": api },
  },
  build: {
    target: "es2022",
    outDir: "dist",
    rollupOptions: {
      output: {
        // The viewer is lazy-loaded; keep CodeMirror in its own chunk too.
        manualChunks(id) {
          if (id.includes("/@codemirror/") || id.includes("style-mod") || id.includes("w3c-keyname") || id.includes("crelt")) return "codemirror";
          return undefined;
        },
      },
    },
  },
});
