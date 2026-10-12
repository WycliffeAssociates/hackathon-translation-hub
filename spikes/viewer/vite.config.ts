import { fileURLToPath } from "node:url";
import { defineConfig } from "vite";

// `#core/*` is Sefer's alias for its core layer. The fork keeps its imports
// verbatim; here the alias points at the viewer's host shims (src/host), which
// is the seam the spike measures.
export default defineConfig({
  resolve: {
    alias: [
      // ENGINE=onion boots the onion-only wasm instead of the galley handle.
      {
        find: "virtual:engine",
        replacement: fileURLToPath(
          new URL(`./src/host/engine.${process.env.ENGINE === "onion" ? "onion" : "galley"}.ts`, import.meta.url),
        ),
      },
      // LAYERS=split builds the viewer against the proposed reading-only module.
      {
        find: "virtual:viewer-layers",
        replacement: fileURLToPath(
          new URL(`./src/viewer/layers.${process.env.LAYERS === "split" ? "split" : "verbatim"}.ts`, import.meta.url),
        ),
      },{ find: /^#core\/(.*)$/, replacement: fileURLToPath(new URL("./src/host/$1", import.meta.url)) }],
  },
  build: {
    target: "es2022",
    sourcemap: true,
    rollupOptions: {
      output: {
        manualChunks(id) {
          if (id.includes("/node_modules/@codemirror/") || id.includes("/node_modules/@lezer/") || id.includes("style-mod") || id.includes("w3c-keyname") || id.includes("crelt") || id.includes("@marijn")) return "codemirror";
          if (id.includes("/node_modules/effect/")) return "effect";
          if (id.includes("/node_modules/@wycliffeassociates/")) return "galley-js";
          if (id.includes("/src/editor-fork/")) return "editor-fork";
          if (id.includes("/src/host/")) return "host";
          return undefined;
        },
      },
    },
  },
});
