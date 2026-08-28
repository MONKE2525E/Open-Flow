#!/usr/bin/env node

import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const read = (relative) => readFileSync(path.join(repoRoot, relative), 'utf8');
const json = (relative) => JSON.parse(read(relative));
const failures = [];
const assert = (condition, message) => { if (!condition) failures.push(message); };

const production = json('src-tauri/tauri.conf.json');
const development = json('src-tauri/tauri.dev.conf.json');
const sourcePlist = read('src-tauri/Info.plist');
const runner = read('scripts/tauri-macos-dev-runner.mjs');
const cli = read('scripts/tauri-cli.mjs');
const relaunch = read('src-tauri/src/commands/permissions.rs');

assert(production.identifier === 'com.verenu.app', 'Production bundle ID must be com.verenu.app.');
assert(production.productName === 'Verenu', 'Production product name must be Verenu.');
assert(production.bundle?.macOS?.bundleName === 'Verenu', 'Production macOS bundle name must be Verenu.');
assert(production.bundle?.macOS?.signingIdentity !== '-', 'Production must not allow ad-hoc signing.');
assert(development.identifier === 'com.verenu.app.dev', 'Development bundle ID must be com.verenu.app.dev.');
assert(development.productName === 'Verenu Development', 'Development product name must be Verenu Development.');
assert(development.bundle?.macOS?.bundleName === 'Verenu Development', 'Development bundle name must be Verenu Development.');
assert(sourcePlist.includes('<string>com.verenu.app</string>'), 'Source Info.plist must retain the production bundle ID.');
assert(sourcePlist.includes('<string>Verenu</string>'), 'Source Info.plist must retain the production name.');
assert(cli.includes("'--config', macDevConfig"), 'Normal tauri dev must merge the canonical development config.');
assert(cli.includes('Refusing an ad-hoc macOS production build'), 'macOS production builds must fail without a signing identity.');
assert(runner.includes("spawn('/usr/bin/open'"), 'Development must launch its app through /usr/bin/open.');
assert(!runner.includes('spawn(bundledBinary'), 'Development must never spawn Contents/MacOS/Verenu directly.');
assert(!runner.includes("const APP_BUNDLE_NAME = 'Verenu.app'"), 'Development must not create an ambiguously named Verenu.app.');
assert(runner.includes("configured !== '-'"), 'Development signing must reject ad-hoc identity selection.');
assert(relaunch.includes('exec /usr/bin/open -n'), 'macOS Relaunch must use LaunchServices.');

if (failures.length > 0) {
  console.error('macOS identity contract failed:');
  for (const failure of failures) console.error(`- ${failure}`);
  process.exit(1);
}

console.log('macOS identity contract verified: prod=com.verenu.app, dev=com.verenu.app.dev.');
