import { defineConfig } from "tsup";

export default defineConfig([
  {
    entry: {
      index: "src/index.ts",
      component: "src/component.ts",
    },
    format: ["esm", "cjs"],
    dts: true,
    sourcemap: true,
    clean: true,
    minify: false,
    treeshake: true,
    target: "es2022",
    splitting: false,
    cjsInterop: true,
  },
  {
    entry: {
      "index.global": "src/index.ts",
    },
    format: ["iife"],
    globalName: "PVG",
    minify: true,
    sourcemap: false,
    target: "es2022",
  },
]);