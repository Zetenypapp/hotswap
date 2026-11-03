// Checks the UI the way the WebView loads it: every script must parse, and the scripts of one
// page share a global scope, so a name declared twice (say `const ago` in two files) breaks the
// whole page. Also checks that tauri.conf.json and Cargo.toml carry the same version.
// Run: node .github/scripts/check-ui.mjs
import fs from 'node:fs';
import path from 'node:path';
import vm from 'node:vm';
import { fileURLToPath } from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..', '..');
const ui = path.join(root, 'ui');
let failed = 0;
const fail = (msg) => { console.error('✗ ' + msg); failed++; };

// Every script on its own.
for (const f of fs.readdirSync(ui).filter((f) => f.endsWith('.js'))) {
  try {
    new vm.Script(fs.readFileSync(path.join(ui, f), 'utf8'), { filename: f });
  } catch (e) {
    fail(`${f}: ${e.message}`);
  }
}

// Each page's scripts together, in load order: external ones and inline <script> blocks.
for (const page of fs.readdirSync(ui).filter((f) => f.endsWith('.html'))) {
  const html = fs.readFileSync(path.join(ui, page), 'utf8');
  const parts = [];
  for (const m of html.matchAll(/<script(?:\s+src="([\w.-]+\.js)")?[^>]*>([\s\S]*?)<\/script>/g)) {
    if (m[1]) parts.push({ name: m[1], code: fs.readFileSync(path.join(ui, m[1]), 'utf8') });
    else if (m[2].trim()) parts.push({ name: `${page} (inline script ${parts.length + 1})`, code: m[2] });
  }
  let code = '';
  for (const p of parts) {
    try {
      new vm.Script(p.code, { filename: p.name });
    } catch {
      continue; // already reported above, or reported here for inline scripts below
    }
    try {
      new vm.Script(code + '\n' + p.code);
      code += '\n' + p.code;
    } catch (e) {
      fail(`${page}: ${p.name} clashes with an earlier script: ${e.message}`);
    }
  }
  for (const p of parts.filter((p) => p.name.includes('inline'))) {
    try { new vm.Script(p.code, { filename: p.name }); } catch (e) { fail(`${p.name}: ${e.message}`); }
  }
}

const conf = JSON.parse(fs.readFileSync(path.join(root, 'src-tauri', 'tauri.conf.json'), 'utf8')).version;
const cargo = /^version\s*=\s*"([^"]+)"/m.exec(fs.readFileSync(path.join(root, 'src-tauri', 'Cargo.toml'), 'utf8'))?.[1];
if (conf !== cargo) fail(`version mismatch: tauri.conf.json ${conf}, Cargo.toml ${cargo}`);

if (failed) process.exit(1);
console.log('✓ UI scripts parse and share their pages cleanly; version ' + conf);
