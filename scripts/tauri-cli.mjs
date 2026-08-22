#!/usr/bin/env node

import { spawn } from 'node:child_process';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const repoRoot = path.resolve(__dirname, '..');
const tauriCli = path.join(repoRoot, 'node_modules', '@tauri-apps', 'cli', 'tauri.js');
const macDevRunner = path.join(__dirname, 'tauri-macos-dev-runner.mjs');

const args = process.argv.slice(2);

if (
  process.platform === 'darwin' &&
  args[0] === 'dev' &&
  !hasRunnerOption(args.slice(1))
) {
  args.splice(1, 0, '--runner', macDevRunner);
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
