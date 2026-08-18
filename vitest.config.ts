import { defineConfig } from "vitest/config";
import { sveltekit } from "@sveltejs/kit/vite";

// Separate from vite.config.js on purpose — that one is tailored for Tauri
// dev/build (fixed port, ignores src-tauri, etc.), none of which applies
// to running unit tests.
export default defineConfig({
  plugins: [sveltekit()],
  test: {
    include: ["src/**/*.test.ts"],
  },
});
