#!/usr/bin/env node

import { existsSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

if (process.platform !== 'darwin') {
  console.error('This diagnostic requires macOS.');
  process.exit(1);
}

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const mode = process.argv.includes('--prod') ? 'prod' : 'dev';
const expected = mode === 'prod'
  ? { id: 'com.verenu.app', name: 'Verenu', relative: 'src-tauri/target/release/bundle/macos/Verenu.app' }
  : { id: 'com.verenu.app.dev', name: 'Verenu Development', relative: 'src-tauri/target/debug/bundle/macos-dev/Verenu Development.app' };
const suppliedPath = process.argv.find((arg, index) => index > 1 && !arg.startsWith('--'));
const app = path.resolve(repoRoot, suppliedPath ?? expected.relative);
const plist = path.join(app, 'Contents', 'Info.plist');

if (!existsSync(plist)) {
  console.error(`App bundle not found: ${app}`);
  process.exit(1);
}

const run = (command, args, allowFailure = false) => {
  const result = spawnSync(command, args, { encoding: 'utf8' });
  const output = `${result.stdout ?? ''}${result.stderr ?? ''}`.trim();
  if (!allowFailure && result.status !== 0) {
    console.error(output);
    process.exit(result.status ?? 1);
  }
  return { status: result.status ?? 1, output };
};
const plistValue = (key) => {
  const value = run('/usr/bin/plutil', ['-extract', key, 'raw', plist], true);
  if (value.status === 0) return value.output;
  // CFBundleDisplayName is optional; macOS falls back to CFBundleName.
  if (key === 'CFBundleDisplayName') {
    const fallback = run('/usr/bin/plutil', ['-extract', 'CFBundleName', 'raw', plist], true);
    if (fallback.status === 0) return fallback.output;
  }
  return '';
};
const bundleId = plistValue('CFBundleIdentifier');
const displayName = plistValue('CFBundleDisplayName');
const executableName = plistValue('CFBundleExecutable');
const executable = executableName ? path.join(app, 'Contents', 'MacOS', executableName) : '';
const signingResult = run('/usr/bin/codesign', ['-dv', '--verbose=4', app], true);
const requirementResult = run('/usr/bin/codesign', ['-dr', '-', app], true);
const entitlementsResult = run('/usr/bin/codesign', ['-d', '--entitlements', ':-', app], true);
const signing = signingResult.output;
const requirement = requirementResult.output;
const entitlements = entitlementsResult.output;
const verify = run('/usr/bin/codesign', ['--verify', '--deep', '--strict', '--verbose=2', app], true);
const spctl = run('/usr/sbin/spctl', ['--assess', '--type', 'execute', '--verbose=4', app], true);
const metadata = run('/usr/bin/mdls', ['-name', 'kMDItemCFBundleIdentifier', '-name', 'kMDItemDisplayName', app], true);
const processes = executable
  ? run('/bin/ps', ['-axww', '-o', 'pid=,ppid=,command=']).output.split('\n').filter((line) => line.includes(executable))
  : [];
const registered = run('/usr/bin/mdfind', [`kMDItemCFBundleIdentifier == '${expected.id}'`], true).output;

console.log(`MODE: ${mode}`);
console.log(`APP: ${app}`);
console.log(`BUNDLE ID: ${bundleId}`);
console.log(`DISPLAY NAME: ${displayName}`);
console.log(`EXECUTABLE: ${executable}`);
console.log('\nCODESIGN\n' + signing);
console.log('\nDESIGNATED REQUIREMENT\n' + requirement);
console.log('\nENTITLEMENTS\n' + entitlements);
console.log(`\nVERIFY (${verify.status})\n${verify.output}`);
console.log(`\nSPCTL (${spctl.status})\n${spctl.output}`);
console.log('\nMETADATA\n' + metadata.output);
console.log('\nRUNNING PROCESSES\n' + (processes.join('\n') || 'none'));
console.log('\nINDEXED COPIES\n' + (registered || 'none'));

const failures = [];
if (bundleId !== expected.id) failures.push(`expected bundle ID ${expected.id}`);
if (displayName !== expected.name) failures.push(`expected display name ${expected.name}`);
if (!executableName) failures.push('CFBundleExecutable is missing');
else if (!existsSync(executable)) failures.push(`bundle executable is missing: ${executable}`);
if (verify.status !== 0) failures.push('codesign verification failed');
if (signingResult.status !== 0) failures.push('could not inspect signing identity');
if (requirementResult.status !== 0) failures.push('could not inspect designated requirement');
if (entitlementsResult.status !== 0) failures.push('could not inspect entitlements');
if (mode === 'dev' && !signing.includes('Authority=Apple Development:')) failures.push('development app is not Apple Development signed');
if (signing.includes('Signature=adhoc')) failures.push('app is ad-hoc signed');
if (failures.length > 0) {
  console.error('\nIDENTITY FAILURE: ' + failures.join('; '));
  process.exit(1);
}
console.log('\nIDENTITY VERIFIED');
