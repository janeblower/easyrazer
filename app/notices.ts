import { execFileSync } from "node:child_process";
import { mkdtempSync, readFileSync, readdirSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import type { Plugin } from "vite";

const ROOT = resolve(import.meta.dirname, "..");
const RULE = "=".repeat(80);

export interface Component {
  name: string;
  version: string;
}

/** Components whose copies of a license name the same holders. */
export interface Users {
  components: Component[];
  holders: string[];
}

export interface Group {
  id: string;
  name: string;
  users: Users[];
  text: string;
}

export interface Notice {
  title: string;
  text: string;
}

export interface Notices {
  groups: Group[];
  notices: Notice[];
}

interface Package {
  name: string;
  version: string;
  manifest_path: string;
}

interface License {
  id: string;
  name: string;
  text: string;
  used_by: { crate: Package }[];
}

interface About {
  licenses: License[];
  crates: { package: Package }[];
}

// Holders are named with a year or a sign; license terms also start lines with "copyright" or "(c)".
const holder = (line: string) => /^\s*(?:copyright\s*(?:\(c\)|©|\d)|\(c\)\s*\d|©\s*\d)/i.test(line);
const holders = (text: string) => [
  ...new Set(
    text
      .split(/\r?\n/)
      .filter(holder)
      .map((line) => line.trim()),
  ),
];
const wording = (text: string) => text.replaceAll(/\s+/g, " ").trim();

const files = (dir: string, pattern: RegExp) =>
  readdirSync(dir, { withFileTypes: true })
    .filter((f) => f.isFile() && pattern.test(f.name))
    .map((f) => join(dir, f.name))
    .toSorted();

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

/** Copies of one license differ in holders, whitespace and headers, so the most common wording stands for all. */
function common(texts: string[]): string {
  const bodies = texts.map((t) =>
    t
      .split(/\r?\n/)
      .filter((line) => !holder(line))
      .join("\n")
      .trim(),
  );
  const counts = new Map<string, number>();
  for (const b of bodies) counts.set(wording(b), (counts.get(wording(b)) ?? 0) + 1);
  const count = (b: string) => counts.get(wording(b)) ?? 0;
  return bodies.toSorted((a, b) => count(b) - count(a))[0];
}

function crates(about: About): { groups: Map<string, Group>; notices: Notice[] } {
  const root = ROOT.toLowerCase();
  const thirdParty = (p: Package) => !p.manifest_path.toLowerCase().startsWith(root);
  const groups = new Map<string, Group>();
  for (const id of new Set(about.licenses.map((l) => l.id))) {
    const copies = about.licenses.filter((l) => l.id === id);
    const users = copies.flatMap((l) => {
      const components = l.used_by.filter((u) => thirdParty(u.crate)).map((u) => ({ name: u.crate.name, version: u.crate.version }));
      return components.length > 0 ? [{ components, holders: holders(l.text) }] : [];
    });
    if (users.length > 0) groups.set(id, { id, name: copies[0].name, users, text: common(copies.map((l) => l.text)) });
  }
  // Apache-2.0 NOTICE files, and notices of works a crate derives from.
  const notices = about.crates
    .filter((c) => thirdParty(c.package))
    .flatMap((c) =>
      files(dirname(c.package.manifest_path), /^(?:notice|third[-_]party[-_]notices)/i).map((f) => ({
        title: `${c.package.name} ${c.package.version}`,
        text: readFileSync(f, "utf8").trim(),
      })),
    );
  return { groups, notices };
}

/** Adds each npm package to the group of the license it is used under, MIT where it offers a choice. */
function packages(moduleIds: Iterable<string>, groups: Map<string, Group>): Notice[] {
  const dirs = new Set<string>();
  for (const id of moduleIds) {
    const m = /^\0?(.*[\\/]node_modules[\\/](?:@[^\\/]+[\\/])?[^\\/]+)[\\/]/.exec(id);
    if (m) dirs.add(m[1]);
  }
  return [...dirs].flatMap((dir) => {
    const pkg = JSON.parse(readFileSync(join(dir, "package.json"), "utf8")) as { name: string; version: string; license?: string };
    const offered = (pkg.license ?? "").replaceAll(/[()]/g, "").split(/\s+OR\s+/);
    const group = groups.get(offered.includes("MIT") ? "MIT" : offered[0]);
    if (!group) throw new Error(`${pkg.name}: no license text for ${pkg.license}`);
    const texts = files(dir, /^(?:licen[cs]e|copying)(?!.*\.spdx$)/i).map((f) => readFileSync(f, "utf8"));
    group.users.push({ components: [{ name: pkg.name, version: pkg.version }], holders: holders(texts.join("\n")) });
    return files(dir, /^notice/i).map((f) => ({ title: `${pkg.name} ${pkg.version}`, text: readFileSync(f, "utf8").trim() }));
  });
}

function collect(moduleIds: Iterable<string>): Notices {
  const { groups, notices } = crates(cargoAbout());
  const fromPackages = packages(moduleIds, groups);
  const size = (g: Group) => g.users.flatMap((u) => u.components).length;
  return { groups: [...groups.values()].toSorted((a, b) => size(b) - size(a)), notices: [...notices, ...fromPackages] };
}

const named = (c: Component) => `${c.name} ${c.version}`;
const section = (title: string, body: string) => `${RULE}\n${title}\n${RULE}\n\n${body.trim()}\n\n`;

/** The release copy: EasyRazer's license, the components by license, then each text once. */
function text(n: Notices): string {
  const list = n.groups.map((g) => `${g.name}:\n  ${g.users.flatMap((u) => u.components.map(named)).join(", ")}`);
  const notices = n.notices.map((x) => `Notice: ${x.title}`);
  const licenses = n.groups.map((g) =>
    section(
      g.name,
      [...g.users.map((u) => [u.components.map(named).join(", "), ...u.holders.map((h) => `  ${h}`)].join("\n")), "", g.text].join("\n"),
    ),
  );
  return (
    section("EasyRazer", readFileSync(join(ROOT, "LICENSE"), "utf8")) +
    section("Third-party components", [...list, ...notices].join("\n\n")) +
    licenses.join("") +
    n.notices.map((x) => section(x.title, x.text)).join("")
  );
}

/** Emits licenses.json for the About window and LICENSES.txt for the release. */
export function licenses(): Plugin {
  return {
    name: "easyrazer-licenses",
    generateBundle(_options, bundle) {
      // Tree-shaken modules stay in the module graph, so only rendered code counts.
      const rendered = Object.values(bundle).flatMap((chunk) =>
        chunk.type === "chunk" ? Object.entries(chunk.modules).flatMap(([id, m]) => (m.renderedLength ? [id] : [])) : [],
      );
      const notices = collect(rendered);
      this.emitFile({ type: "asset", fileName: "licenses.json", source: JSON.stringify(notices) });
      this.emitFile({ type: "asset", fileName: "LICENSES.txt", source: text(notices) });
    },
    // The dev server has no bundle to read npm packages from, so it lists the crates alone.
    configureServer(server) {
      server.middlewares.use("/licenses.json", (_req, res) => {
        res.setHeader("content-type", "application/json");
        res.end(JSON.stringify(collect([])));
      });
    },
  };
}
