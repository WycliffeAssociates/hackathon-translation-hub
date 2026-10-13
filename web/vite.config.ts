import { fileURLToPath } from "node:url";
import { defineConfig } from "vite";
import solid from "vite-plugin-solid";

// The USFM viewer is imported from the spike's source, not copied: `@viewer`
// is spikes/viewer/src. The spike's own aliases come along so its files build
// unchanged, set to its "build C" (ENGINE=onion, LAYERS=split): the onion-only
// parser wasm and the reading-only editor layers. The spike's bare imports
// (@codemirror/*, scripture-kitchen) resolve from spikes/viewer/node_modules.
const spike = (p: string) => fileURLToPath(new URL(`../spikes/viewer/src/${p}`, import.meta.url));

const api = process.env.HUB_API ?? "http://127.0.0.1:8788";

export default defineConfig({
  plugins: [solid()],
  resolve: {
    alias: [
      { find: /^@viewer\/(.*)$/, replacement: spike("$1") },
      { find: "virtual:viewer-layers", replacement: spike("viewer/layers.split.ts") },
      { find: /^#core\/(.*)$/, replacement: spike("host/$1") },
    ],
  },
  server: {
    proxy: { "/v1": api },
    fs: { allow: [".", "../spikes/viewer"] },
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
