import { expect, test } from "bun:test";
import { createServer } from "vite";
import { type Component, createSSRApp } from "vue";
import { renderToString } from "vue/server-renderer";
import { createI18n } from "vue-i18n";
import { defaultGamepad } from "./gamepad";
import en from "./locales/en.json";

test("settings offers the boot switch only when hidshim has a boot task", async () => {
  const server = await createServer({ server: { middlewareMode: true }, appType: "custom" });
  try {
    const { default: SettingsTab } = (await server.ssrLoadModule("/src/components/SettingsTab.vue")) as { default: Component };
    const render = async (boot: boolean | null, bootChanging = false) => {
      const driverStatus = { installed: true, ready: true, version: "0.2.0", legacy: false, boot };
      const app = createSSRApp(SettingsTab, { driverStatus, driverOperation: null, driverError: null, bootChanging });
      app.use(createI18n({ legacy: false, locale: "en", messages: { en } }));
      return renderToString(app);
    };
    const unsupported = await render(null);
    expect(unsupported.includes(en.settings.driverBoot)).toBe(false);
    const off = await render(false);
    expect(off.includes(en.settings.driverBoot)).toBe(true);
    expect(/role="switch"(?![^>]*checked)[^>]*>/.test(off)).toBe(true);
    expect(/role="switch"[^>]*checked/.test(await render(true))).toBe(true);
    expect(/role="switch"[^>]*disabled/.test(await render(true, true))).toBe(true);
  } finally {
    await server.close();
  }
});

test("settings offers install or uninstall and prevents duplicate driver operations", async () => {
  const server = await createServer({ server: { middlewareMode: true }, appType: "custom" });
  try {
    const { default: SettingsTab } = (await server.ssrLoadModule("/src/components/SettingsTab.vue")) as { default: Component };
    const render = async (
      driverStatus: { installed: boolean; ready: boolean; legacy?: boolean } | null,
      driverOperation: "install" | "uninstall" | null = null,
    ) => {
      const app = createSSRApp(SettingsTab, { driverStatus, driverOperation, driverError: null });
      app.use(createI18n({ legacy: false, locale: "en", messages: { en } }));
      return renderToString(app);
    };
    const missing = await render({ installed: false, ready: false });
    expect(missing.includes("Install hidshim")).toBe(true);
    expect(missing.includes("Uninstall hidshim")).toBe(false);
    const installed = await render({ installed: true, ready: true });
    expect(installed.includes("Uninstall hidshim")).toBe(true);
    const partial = await render({ installed: true, ready: false });
    expect(partial.includes("Uninstall hidshim")).toBe(true);
    expect(installed.includes("HIDMaestro")).toBe(false);
    const legacy = await render({ installed: true, ready: true, legacy: true });
    expect(legacy.includes(en.gamepad.legacyInstalled)).toBe(true);
    for (const operation of ["install", "uninstall"] as const) {
      const busy = await render({ installed: true, ready: true }, operation);
      expect(busy.includes('aria-busy="true"')).toBe(true);
      expect(/<button[^>]*disabled/.test(busy)).toBe(true);
    }
    expect(/<button[^>]*disabled/.test(await render(null))).toBe(true);
  } finally {
    await server.close();
  }
});

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
        driverOperation: installing ? "install" : null,
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
    expect(hardware.includes("Install hidshim")).toBe(false);
    expect(hardware.includes("Switch to driver mode")).toBe(true);
    const missing = await render(true, false);
    expect(inactive(missing)).toBe(true);
    expect(missing.includes("Install hidshim")).toBe(true);
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
