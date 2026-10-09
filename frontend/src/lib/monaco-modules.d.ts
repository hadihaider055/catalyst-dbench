// Monaco's ESM entry points ship without their own declarations; they expose the same
// API as the package root.
declare module "monaco-editor/esm/vs/editor/edcore.main" {
  export * from "monaco-editor";
}
