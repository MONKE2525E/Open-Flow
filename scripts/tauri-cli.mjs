#!/usr/bin/env node

import { spawn } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const repoRoot = path.resolve(__dirname, '..');
const tauriCli = path.join(repoRoot, 'node_modules', '@tauri-apps', 'cli', 'tauri.js');
const macDevRunner = path.join(__dirname, 'tauri-macos-dev-runner.mjs');
const macDevConfig = path.join(repoRoot, 'src-tauri', 'tauri.dev.conf.json');

const args = process.argv.slice(2);

if (process.platform === 'win32' && isReleaseBuild(args)) {
  assertWindowsBuildSpace();
  removeStaleNsisInstallers();
}

if (
  process.platform === 'darwin' &&
  args[0] === 'dev' &&
  !hasRunnerOption(args.slice(1))
) {
  const configArgs = hasConfigOption(args.slice(1)) ? [] : ['--config', macDevConfig];
  args.splice(1, 0, ...configArgs, '--runner', macDevRunner);
}

if (
  process.platform === 'darwin' &&
  args[0] === 'build' &&
  !process.env.APPLE_SIGNING_IDENTITY?.trim()
) {
  console.error(
    'Refusing an ad-hoc macOS production build. Set APPLE_SIGNING_IDENTITY or run npm run tauri:build:signed.',
  );
  process.exit(1);
}

const child = spawn(process.execPath, [tauriCli, ...args], {
  cwd: repoRoot,
  env: process.env,
  stdio: 'inherit',
  windowsHide: true,
});

child.on('exit', (code, signal) => {
  if (signal) {
    process.kill(process.pid, signal);
    return;
  }

  process.exit(code ?? 0);
});

function hasRunnerOption(args) {
  for (const arg of args) {
    if (arg === '--') {
      return false;
    }
    if (arg === '-r' || arg === '--runner' || arg.startsWith('--runner=')) {
      return true;
    }
  }
  return false;
}

function hasConfigOption(args) {
  for (const arg of args) {
    if (arg === '--') return false;
    if (arg === '-c' || arg === '--config' || arg.startsWith('--config=')) return true;
  }
  return false;
}

function isReleaseBuild(args) {
  return args[0] === 'build' && !args.includes('--debug') && !args.includes('-d');
}

function assertWindowsBuildSpace() {
  const systemRoot = process.env.SystemDrive || path.parse(process.env.LOCALAPPDATA || 'C:\\').root;
  const minimumFreeBytes = 512 * 1024 * 1024;
  const stats = fs.statfsSync(systemRoot);
  const freeBytes = Number(stats.bavail) * Number(stats.bsize);

  if (freeBytes < minimumFreeBytes) {
    const freeMiB = Math.floor(freeBytes / 1024 / 1024);
    console.error(
      `Refusing to build Windows installers: ${systemRoot} has only ${freeMiB} MiB free. ` +
        'Free at least 512 MiB before building; NSIS can otherwise leave a corrupt installer.',
    );
    process.exit(1);
  }
}

function removeStaleNsisInstallers() {
  const nsisBundleDir = path.join(repoRoot, 'src-tauri', 'target', 'release', 'bundle', 'nsis');
  if (!fs.existsSync(nsisBundleDir)) return;

  for (const entry of fs.readdirSync(nsisBundleDir, { withFileTypes: true })) {
    if (entry.isFile() && entry.name.endsWith('.exe')) {
      fs.rmSync(path.join(nsisBundleDir, entry.name));
    }
  }
}
