#!/usr/bin/env node
/**
 * preset-licenses: the family licence gate and third-party notices generator.
 * Run from an app's repo root. Plain ESM, Node built-ins only.
 *
 *   preset-licenses check
 *   preset-licenses notices --out <file.html> [--text <file>]
 *
 * Two scopes, on purpose:
 *
 *   - The GATE covers the WHOLE tree: `cargo metadata --all-features` (every
 *     crate Cargo could resolve, dev and build included) and
 *     `pnpm licenses list --json --prod=false` (dev dependencies too). A
 *     copyleft licence in the build tooling is still a decision someone should
 *     make, not an accident, so the gate is strict.
 *   - The NOTICES cover only what SHIPS: `cargo tree -e normal` for the app
 *     crate (normal dependencies for this machine's target) and
 *     `pnpm licenses list --json --prod`. Listing dev tooling would claim the
 *     app contains software it doesn't.
 *
 * Policy: guidance/projects/oblique/design/licensing.md (permissive-only).
 * Changes to ALLOWED below happen there in the same commit. Per-package
 * exceptions every Tauri + Tailwind app carries are FAMILY_EXCEPTIONS below;
 * anything particular to one app lives in its `licenses.config.json`, each
 * with a written reason, and is recorded in the policy doc too:
 *
 *   {
 *     "exceptions": { "<package name>": "<reason>" },
 *     "own": ["crate or package names to skip"],
 *     "cargoPackage": "<binary crate name>",
 *     "cargoManifest": "src-tauri/Cargo.toml"
 *   }
 *
 * `cargoManifest` defaults to src-tauri/Cargo.toml, else Cargo.toml; Rust is
 * skipped when neither exists. `cargoPackage` defaults to the manifest's
 * package, or the only workspace member with a binary target.
 */
import { execFileSync } from 'node:child_process';
import { existsSync, readFileSync, readdirSync, statSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';

const ALLOWED = new Set([
  'MIT',
  'MIT-0',
  'ISC',
  'Apache-2.0',
  'BSD-2-Clause',
  'BSD-3-Clause',
  '0BSD',
  'Zlib',
  'Unlicense',
  'CC0-1.0',
  'BlueOak-1.0.0',
  'Python-2.0',
  'MIT-CMU',
  'HPND',
  'OFL-1.1', // fonts only
  'CC-BY-4.0', // data and docs assets only, keep attribution
  // Unicode data tables (ICU4X crates via Tauri, unicode-ident via syn).
  'Unicode-3.0',
]);

// Reviewed exceptions every app in the family carries, so no app repeats them.
// MPL-2.0 is file-level copyleft: it binds changes to these packages' own files,
// never our code, and they're all used unmodified. Recorded in the policy doc.
const FAMILY_EXCEPTIONS = {
  // Via Tauri (webview CSS handling, `dirs`). Reviewed in Shard 2026-09-12.
  cssparser: 'MPL-2.0, via Tauri, unmodified',
  'cssparser-macros': 'MPL-2.0, via Tauri, unmodified',
  'dtoa-short': 'MPL-2.0, via Tauri, unmodified',
  selectors: 'MPL-2.0, via Tauri, unmodified',
  'option-ext': 'MPL-2.0, via Tauri (dirs), unmodified',
  // Via Tailwind 4, build-time CSS tooling, not in the app. Reviewed in Oblique 2026-07-04.
  lightningcss: 'MPL-2.0, Tailwind build tooling, unmodified',
  'lightningcss-darwin-arm64': 'MPL-2.0, Tailwind build tooling, unmodified',
  'lightningcss-darwin-x64': 'MPL-2.0, Tailwind build tooling, unmodified',
};

const root = process.cwd();

function run(cmd, args, opts = {}) {
  return execFileSync(cmd, args, {
    encoding: 'utf8',
    maxBuffer: 512 * 1024 * 1024,
    cwd: root,
    stdio: ['ignore', 'pipe', 'pipe'],
    ...opts,
  });
}

function readJson(path) {
  return JSON.parse(readFileSync(path, 'utf8'));
}

function loadConfig() {
  const path = join(root, 'licenses.config.json');
  const cfg = existsSync(path) ? readJson(path) : {};
  return {
    exceptions: { ...FAMILY_EXCEPTIONS, ...(cfg.exceptions ?? {}) },
    own: new Set(cfg.own ?? []),
    cargoPackage: cfg.cargoPackage,
    cargoManifest: cfg.cargoManifest,
  };
}

function findManifest(cfg) {
  const candidates = cfg.cargoManifest ? [cfg.cargoManifest] : ['src-tauri/Cargo.toml', 'Cargo.toml'];
  const found = candidates.map((c) => resolve(root, c)).find((c) => existsSync(c));
  if (cfg.cargoManifest && !found) fail(`cargoManifest ${cfg.cargoManifest} does not exist`);
  return found ?? null;
}

function fail(message) {
  console.error(`preset-licenses: ${message}`);
  process.exit(2);
}

// ---------------------------------------------------------------------------
// SPDX expressions

/** Tokens: "(", ")", and words. Legacy `A/B` means `A OR B`. */
function tokenise(expr) {
  const spaced = expr.replaceAll('/', ' OR ').replace(/([()])/g, ' $1 ');
  return spaced.split(/\s+/).filter(Boolean);
}

/**
 * Recursive descent over SPDX precedence: WITH > AND > OR, parentheses
 * grouping. OR passes if any branch is allowed, AND needs all, and
 * `X WITH exception` passes when X does (an exception only grants more).
 */
export function licenceAllowed(expr) {
  if (!expr || !expr.trim()) return false;
  const tokens = tokenise(expr);
  let i = 0;
  const parseOr = () => {
    let ok = parseAnd();
    while (tokens[i]?.toUpperCase() === 'OR') {
      i++;
      const rhs = parseAnd();
      ok = ok || rhs;
    }
    return ok;
  };
  const parseAnd = () => {
    let ok = parseWith();
    while (tokens[i]?.toUpperCase() === 'AND') {
      i++;
      const rhs = parseWith();
      ok = ok && rhs;
    }
    return ok;
  };
  const parseWith = () => {
    const ok = parseAtom();
    if (tokens[i]?.toUpperCase() === 'WITH') {
      i += 2; // WITH <exception>
    }
    return ok;
  };
  const parseAtom = () => {
    const t = tokens[i++];
    if (t === '(') {
      const ok = parseOr();
      if (tokens[i] === ')') i++;
      return ok;
    }
    if (t === undefined || t === ')') return false;
    return ALLOWED.has(t.replace(/\+$/, '')) || ALLOWED.has(t);
  };
  const result = parseOr();
  return i >= tokens.length ? result : false; // trailing junk: refuse
}

// ---------------------------------------------------------------------------
// Rust

function cargoMetadata(manifest, allFeatures) {
  const args = ['metadata', '--format-version', '1', '--manifest-path', manifest];
  if (allFeatures) args.push('--all-features');
  return JSON.parse(run('cargo', args));
}

/** Gate packages: everything in the resolved tree except our own crates. */
function rustGatePackages(manifest, cfg) {
  const meta = cargoMetadata(manifest, true);
  const ours = new Set(meta.workspace_members ?? []);
  return meta.packages
    .filter((p) => !ours.has(p.id) && !cfg.own.has(p.name))
    .map((p) => ({ name: p.name, version: p.version, licence: p.license ?? null }));
}

function appCrate(meta, manifest, cfg) {
  if (cfg.cargoPackage) return cfg.cargoPackage;
  const ws = new Set(meta.workspace_members);
  const members = meta.packages.filter((p) => ws.has(p.id));
  const here = members.find((p) => resolve(p.manifest_path) === manifest);
  if (here) return here.name;
  const bins = members.filter((p) => p.targets.some((t) => t.kind.includes('bin')));
  if (bins.length === 1) return bins[0].name;
  fail('cannot tell which crate is the app; set "cargoPackage" in licenses.config.json');
}

/** Shipped crates: `cargo tree -e normal` from the app crate, with metadata for files. */
function rustShippedPackages(manifest, cfg) {
  const meta = cargoMetadata(manifest, false);
  const app = appCrate(meta, manifest, cfg);
  const tree = run('cargo', [
    'tree', '--manifest-path', manifest, '-p', app, '-e', 'normal', '--prefix', 'none', '--format', '{p}',
  ]);
  const byKey = new Map(meta.packages.map((p) => [`${p.name}@${p.version}`, p]));
  const seen = new Map();
  for (const line of tree.split('\n')) {
    const m = /^(\S+) v(\S+)/.exec(line.replace(/ \(\*\)$/, '').trim());
    if (!m) continue;
    const pkg = byKey.get(`${m[1]}@${m[2]}`);
    if (!pkg || cfg.own.has(pkg.name)) continue;
    if (meta.workspace_members.includes(pkg.id)) continue;
    seen.set(pkg.id, {
      name: pkg.name,
      version: pkg.version,
      licence: pkg.license ?? null,
      link: pkg.repository || pkg.homepage || null,
      dirs: [dirname(pkg.manifest_path)],
      licenceFile: pkg.license_file ? join(dirname(pkg.manifest_path), pkg.license_file) : null,
    });
  }
  return [...seen.values()];
}

// ---------------------------------------------------------------------------
// npm

function pnpmList(prod) {
  const raw = run('pnpm', ['licenses', 'list', '--json', prod ? '--prod' : '--prod=false']);
  return JSON.parse(raw);
}

/** Flatten pnpm's { licence: [{ name, versions[], paths[] }] } to one row per version. */
function npmRows(byLicence, own) {
  const rows = [];
  for (const [licence, pkgs] of Object.entries(byLicence)) {
    for (const p of pkgs) {
      const versions = p.versions?.length ? p.versions : [''];
      versions.forEach((version, i) => {
        if (own.has(p.name)) return;
        rows.push({
          name: p.name,
          version,
          licence,
          link: p.homepage || null,
          dirs: p.paths?.[i] ? [p.paths[i]] : [],
        });
      });
    }
  }
  return rows;
}

function npmGatePackages(cfg) {
  return npmRows(pnpmList(false), cfg.own)
    // Our own @preset.nz/* packages are MIT; skip them only when pnpm can't see a licence.
    .filter((r) => !(r.name.startsWith('@preset.nz/') && /^(unknown|undefined|)$/i.test(r.licence)))
    .map((r) => ({ name: r.name, version: r.version, licence: /^(unknown|undefined)$/i.test(r.licence) ? null : r.licence }));
}

// ---------------------------------------------------------------------------
// check

function check() {
  const cfg = loadConfig();
  const manifest = findManifest(cfg);
  const hasPnpm = existsSync(join(root, 'package.json'));
  const bad = [];
  let crates = 0;
  let npm = 0;
  const examine = (kind, rows) => {
    for (const r of rows) {
      if (cfg.exceptions[r.name]) continue;
      if (!licenceAllowed(r.licence)) bad.push({ kind, ...r });
    }
  };
  if (manifest) {
    const rows = rustGatePackages(manifest, cfg);
    crates = rows.length;
    examine('crate', rows);
  }
  if (hasPnpm) {
    const rows = npmGatePackages(cfg);
    npm = rows.length;
    examine('npm', rows);
  }
  if (bad.length > 0) {
    console.error(`licence gate: ${bad.length} package(s) outside the permissive allowlist\n`);
    const lines = bad.map((b) => `  [${b.kind}] ${b.name} ${b.version}  ->  ${b.licence ?? 'UNSPECIFIED'}`);
    console.error([...new Set(lines)].sort().join('\n'));
    console.error(
      '\nEither drop the dependency or add a reviewed exception, with the reason, to\n' +
        'licenses.config.json AND to the policy doc (guidance/projects/oblique/design/licensing.md),\n' +
        'in the same commit.',
    );
    process.exit(1);
  }
  console.log(`licence gate: ${crates} crates, ${npm} npm packages, all permissive`);
}

// ---------------------------------------------------------------------------
// notices

const LICENCE_FILE = /^(licen[cs]e|copying|notice|unlicen[cs]e)([-._].*)?$/i;

function licenceFiles(dirs, extra) {
  const files = new Set();
  if (extra && existsSync(extra)) files.add(extra);
  for (const dir of dirs) {
    let names = [];
    try {
      names = readdirSync(dir);
    } catch {
      continue;
    }
    for (const n of names) {
      const full = join(dir, n);
      if (LICENCE_FILE.test(n) && statSync(full).isFile()) files.add(full);
    }
  }
  return [...files].sort();
}

const normalise = (s) => s.replace(/\r\n/g, '\n').trim();

function escapeHtml(s) {
  return s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');
}

function safeUrl(u) {
  if (!u) return null;
  const url = u.replace(/^git\+/, '').replace(/\.git$/, '').replace(/^git:\/\//, 'https://');
  return /^https?:\/\//.test(url) ? url : null;
}

function notices(args) {
  const out = args.out;
  if (!out) fail('notices needs --out <file.html>');
  const cfg = loadConfig();
  const manifest = findManifest(cfg);
  let productName = 'This app';
  const tauriConf = join(root, 'src-tauri', 'tauri.conf.json');
  if (existsSync(tauriConf)) productName = readJson(tauriConf).productName ?? productName;
  else if (existsSync(join(root, 'package.json'))) productName = readJson(join(root, 'package.json')).name ?? productName;

  const crates = manifest ? rustShippedPackages(manifest, cfg) : [];
  const npm = existsSync(join(root, 'package.json')) ? npmRows(pnpmList(true), cfg.own) : [];
  for (const c of crates) c.licence ??= 'unknown';
  const byName = (a, b) => a.name.localeCompare(b.name) || a.version.localeCompare(b.version, undefined, { numeric: true });
  crates.sort(byName);
  npm.sort(byName);
  for (const r of npm) r.licenceFile = null;

  // Group packages that share an identical licence text.
  const all = [...crates.map((p) => ({ ...p, kind: 'crate' })), ...npm.map((p) => ({ ...p, kind: 'npm' }))];
  const groups = new Map(); // text -> { text, files, packages[] }
  const missing = [];
  for (const p of all) {
    const texts = licenceFiles(p.dirs, p.licenceFile).map((f) => normalise(readFileSync(f, 'utf8'))).filter(Boolean);
    if (texts.length === 0) {
      missing.push(p);
      continue;
    }
    for (const text of new Set(texts)) {
      if (!groups.has(text)) groups.set(text, { text, packages: [] });
      groups.get(text).packages.push(p);
    }
  }
  const blocks = [...groups.values()].sort(
    (a, b) => b.packages.length - a.packages.length || a.packages[0].name.localeCompare(b.packages[0].name),
  );
  const label = (p) => `${p.name} ${p.version}`.trim();
  const missingByLicence = new Map();
  for (const p of missing) {
    const key = p.licence ?? 'unknown';
    if (!missingByLicence.has(key)) missingByLicence.set(key, []);
    missingByLicence.get(key).push(p);
  }

  const entries = (rows) =>
    rows
      .map((p) => {
        const url = safeUrl(p.link);
        const name = url ? `<a href="${escapeHtml(url)}">${escapeHtml(label(p))}</a>` : escapeHtml(label(p));
        return `<li><span class="n">${name}</span> <span class="l">${escapeHtml(p.licence)}</span></li>`;
      })
      .join('\n');

  const html = `<!doctype html>
<html lang="en-NZ">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Open-source licences</title>
<style>
:root { color-scheme: light dark; --fg: #1d1d1f; --bg: #fff; --mute: #6e6e73; --rule: #d2d2d7; --code: #f5f5f7; }
@media (prefers-color-scheme: dark) { :root { --fg: #f5f5f7; --bg: #1c1c1e; --mute: #98989d; --rule: #3a3a3c; --code: #2c2c2e; } }
body { font: 14px/1.5 -apple-system, BlinkMacSystemFont, system-ui, sans-serif; color: var(--fg); background: var(--bg); margin: 0 auto; padding: 24px 20px 48px; max-width: 760px; }
h1 { font-size: 20px; margin: 0 0 8px; }
h2 { font-size: 16px; margin: 32px 0 8px; border-bottom: 1px solid var(--rule); padding-bottom: 4px; }
h3 { font-size: 13px; margin: 24px 0 4px; }
p.lead, .mute { color: var(--mute); }
ul { list-style: none; padding: 0; margin: 0; }
li { display: flex; justify-content: space-between; gap: 16px; padding: 2px 0; }
li .l { color: var(--mute); text-align: right; }
a { color: inherit; }
pre { background: var(--code); padding: 12px; border-radius: 6px; font: 12px/1.45 ui-monospace, Menlo, monospace; white-space: pre-wrap; overflow-wrap: anywhere; }
details summary { cursor: pointer; }
</style>
</head>
<body>
<h1>Open-source licences</h1>
<p class="lead">${escapeHtml(productName)} includes the following open-source software, each under its own licence.</p>
${crates.length ? `<h2>Rust crates (${crates.length})</h2>\n<ul>\n${entries(crates)}\n</ul>` : ''}
${npm.length ? `<h2>npm packages (${npm.length})</h2>\n<ul>\n${entries(npm)}\n</ul>` : ''}
<h2>Licence texts</h2>
${blocks
  .map(
    (b) => `<h3>${escapeHtml(b.packages.map(label).join(', '))}</h3>\n<pre>${escapeHtml(b.text)}</pre>`,
  )
  .join('\n')}
${
  missing.length
    ? `<h2>Licence text not included in the package</h2>\n${[...missingByLicence]
        .map(
          ([lic, ps]) =>
            `<p><strong>${escapeHtml(lic)}</strong>: ${escapeHtml(ps.map(label).join(', '))}. The licence text is not included in these packages.</p>`,
        )
        .join('\n')}`
    : ''
}
</body>
</html>
`;
  writeFileSync(out, html);

  if (args.text) {
    const rule = (s) => `${s}\n${'='.repeat(s.length)}\n`;
    const lines = [
      `${productName} includes the following open-source software, each under its own licence.`,
      '',
      ...(crates.length ? [rule('Rust crates'), ...crates.map((p) => `${label(p)}  ${p.licence}`), ''] : []),
      ...(npm.length ? [rule('npm packages'), ...npm.map((p) => `${label(p)}  ${p.licence}`), ''] : []),
      rule('Licence texts'),
      ...blocks.flatMap((b) => [`-- ${b.packages.map(label).join(', ')}`, '', b.text, '']),
      ...(missing.length
        ? [
            rule('Licence text not included in the package'),
            ...[...missingByLicence].map(([lic, ps]) => `${lic}: ${ps.map(label).join(', ')}\n`),
          ]
        : []),
    ];
    writeFileSync(args.text, lines.join('\n'));
  }
  console.log(
    `notices: ${crates.length} crates, ${npm.length} npm packages, ${blocks.length} distinct licence texts, ${missing.length} without a licence file -> ${out}`,
  );
}

// ---------------------------------------------------------------------------

function parseArgs(argv) {
  const args = {};
  for (let i = 0; i < argv.length; i++) {
    if (argv[i] === '--out') args.out = argv[++i];
    else if (argv[i] === '--text') args.text = argv[++i];
    else fail(`unknown argument ${argv[i]}`);
  }
  return args;
}

const [mode, ...rest] = process.argv.slice(2);
if (mode === 'check') check();
else if (mode === 'notices') notices(parseArgs(rest));
else if (mode) {
  console.error('usage: preset-licenses check | notices --out <file.html> [--text <file>]');
  process.exit(2);
}
