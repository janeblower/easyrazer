import { execSync } from "node:child_process";
import { defineConfig } from "vite";
import vue from "@vitejs/plugin-vue";
import unocss from "unocss/vite";

const git = (args: string) =>
  execSync(`git ${args}`, { stdio: ["ignore", "pipe", "ignore"] })
    .toString()
    .trim();

// The "nightly" tag marks the rolling pre-release, so only version tags count.
function version() {
  try {
    return git('describe --tags --exact-match --match "[0-9]*" HEAD');
  } catch {
    return `Nightly ${git("rev-parse --short HEAD")}`;
  }
}

export default defineConfig({
  plugins: [vue(), unocss()],
  clearScreen: false,
  server: { port: 5173, strictPort: true },
  define: { __APP_VERSION__: JSON.stringify(version()) },
});
