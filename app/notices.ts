import { execFileSync } from "node:child_process";
import { mkdtempSync, readFileSync, readdirSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import type { Plugin } from "vite";

const ROOT = resolve(import.meta.dirname, "..");
const FILE = "LICENSES.txt";
const RULE = "=".repeat(80);

interface Package {
  name: string;
  version: string;
  manifest_path: string;
}

interface About {
  licenses: { name: string; text: string; used_by: { crate: Package }[] }[];
  crates: { package: Package }[];
}

const files = (dir: string, pattern: RegExp) =>
  readdirSync(dir, { withFileTypes: true })
    .filter((f) => f.isFile() && pattern.test(f.name))
    .map((f) => join(dir, f.name))
    .toSorted();

const section = (title: string, body: string) => `${RULE}\n${title}\n${RULE}\n\n${body.trim()}\n\n`;

function cargoAbout(): About {
  // cargo-about refuses to write JSON to a pipe when it detects PowerShell.
  const dir = mkdtempSync(join(tmpdir(), "easyrazer-about-"));
  try {
    const out = join(dir, "about.json");
    execFileSync(
      "cargo",
      ["about", "generate", "--format", "json", "-c", join(ROOT, "about.toml"), "-m", join(ROOT, "app/src-tauri/Cargo.toml"), "-o", out],
      { cwd: ROOT, stdio: ["ignore", "ignore", "inherit"] },
    );
    return JSON.parse(readFileSync(out, "utf8")) as About;
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}

function rust(): string {
  const about = cargoAbout();
  const root = ROOT.toLowerCase();
  const thirdParty = (p: Package) => !p.manifest_path.toLowerCase().startsWith(root);
  const licenses = about.licenses.map((l) => {
    const crates = [...new Set(l.used_by.filter((u) => thirdParty(u.crate)).map((u) => `${u.crate.name} ${u.crate.version}`))];
    return crates.length > 0 ? section(`${l.name}\nUsed by: ${crates.join(", ")}`, l.text) : "";
  });
  // Apache-2.0 NOTICE files, and notices of works a crate derives from, such as HIDMaestro's.
  const notices = about.crates
    .filter((c) => thirdParty(c.package))
    .flatMap((c) =>
      files(dirname(c.package.manifest_path), /^(?:notice|third[-_]party[-_]notices)/i).map((f) =>
        section(`${c.package.name} ${c.package.version}`, readFileSync(f, "utf8")),
      ),
    );
  return [...licenses, ...notices].join("");
}

function javascript(moduleIds: Iterable<string>): string {
  const dirs = new Set<string>();
  for (const id of moduleIds) {
    const m = /^\0?(.*[\\/]node_modules[\\/](?:@[^\\/]+[\\/])?[^\\/]+)[\\/]/.exec(id);
    if (m) dirs.add(m[1]);
  }
  return Array.from(dirs, (dir) => {
    const pkg = JSON.parse(readFileSync(join(dir, "package.json"), "utf8")) as { name: string; version: string; license?: string };
    const texts = files(dir, /^(?:licen[cs]e|copying|notice)/i);
    if (texts.length === 0) throw new Error(`${pkg.name} ships no license file`);
    return section(`${pkg.name} ${pkg.version} (${pkg.license})`, texts.map((f) => readFileSync(f, "utf8")).join("\n\n"));
  })
    .toSorted()
    .join("");
}

const notices = (moduleIds: Iterable<string>) =>
  section("EasyRazer", readFileSync(join(ROOT, "LICENSE"), "utf8")) + rust() + javascript(moduleIds);

/** Emits LICENSES.txt: EasyRazer's license and those of every bundled crate and npm package. */
export function licenses(): Plugin {
  return {
    name: "easyrazer-licenses",
    generateBundle(_options, bundle) {
      // Tree-shaken modules stay in the module graph, so only rendered code counts.
      const rendered = Object.values(bundle).flatMap((chunk) =>
        chunk.type === "chunk" ? Object.entries(chunk.modules).flatMap(([id, m]) => (m.renderedLength ? [id] : [])) : [],
      );
      this.emitFile({ type: "asset", fileName: FILE, source: notices(rendered) });
    },
    // The dev server has no bundle to read npm packages from, so it lists the crates alone.
    configureServer(server) {
      server.middlewares.use(`/${FILE}`, (_req, res) => {
        res.setHeader("content-type", "text/plain; charset=utf-8");
        res.end(notices([]));
      });
    },
  };
}
