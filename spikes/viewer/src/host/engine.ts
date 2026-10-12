// Which engine the demo boots: ENGINE=onion (stateless parser wasm) or the
// default galley handle. Resolved by the `virtual:engine` alias in vite.config.ts.
export { bootEngine, type Engine } from "virtual:engine";
