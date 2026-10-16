import { expect, test } from "bun:test";
import { createServer } from "vite";
import { type Component, createSSRApp } from "vue";
import { renderToString } from "vue/server-renderer";
import { createI18n } from "vue-i18n";
import { defaultGamepad } from "./gamepad";
import en from "./locales/en.json";

test("gamepad disables the model and editor until driver mode and both packages are available", async () => {
  const server = await createServer({ server: { middlewareMode: true }, appType: "custom" });
  try {
    const { default: GamepadTab } = (await server.ssrLoadModule("/src/components/GamepadTab.vue")) as { default: Component };
    const render = async (driver: boolean, driverInstalled: boolean, installing = false) => {
      const app = createSSRApp(GamepadTab, {
        selected: "left",
        config: defaultGamepad(),
        depth: [],
        layout: [],
        editable: true,
        driver,
        driverInstalled,
        installing,
        installError: null,
        running: false,
        error: null,
      });
      app.use(createI18n({ legacy: false, locale: "en", messages: { en } }));
      return renderToString(app);
    };
    const inactive = (html: string) => /<div[^>]*gamepad-content[^>]* inert(?:\s|>)/.test(html);
    const hardware = await render(false, true);
    expect(inactive(hardware)).toBe(true);
    expect(hardware.includes("Install HIDMaestro")).toBe(false);
    const missing = await render(true, false);
    expect(inactive(missing)).toBe(true);
    expect(missing.includes("Install HIDMaestro")).toBe(true);
    expect(missing.includes('viewBox="0 0 24 24"')).toBe(true);
    const installing = await render(true, false, true);
    expect(installing.includes('aria-busy="true"')).toBe(true);
    expect(/<button disabled[^>]*><span class="install-spinner"/.test(installing)).toBe(true);
    const ready = await render(true, true);
    expect(inactive(ready)).toBe(false);
    expect(ready.includes("unavailable")).toBe(false);
  } finally {
    await server.close();
  }
});
