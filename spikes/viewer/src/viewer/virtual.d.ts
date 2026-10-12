declare module "virtual:viewer-layers" {
  export { readingLayer, viewLayer } from "../editor-fork/core/compose";
}
declare module "virtual:engine" {
  export { bootEngine, type Engine } from "../host/engine.galley";
}
