#!/usr/bin/env node

import { spawn, spawnSync } from 'node:child_process';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const repoRoot = path.resolve(__dirname, '..');
const scriptPath = path.join(repoRoot, 'tests', 'OnePyFone.py');
const extraArgs = process.argv.slice(2);

const python = findPython();

if (!python) {
  console.error('Unable to find a Python 3 interpreter. Install Python 3 and retry.');
  process.exit(1);
}

const child = spawn(python.command, [...python.prefixArgs, scriptPath, ...extraArgs], {
  cwd: repoRoot,
  env: process.env,
  stdio: 'inherit',
});

child.on('error', (err) => {
  console.error('Failed to start OnePyFone process:', err);
  process.exit(1);
});

child.on('exit', (code, signal) => {
  if (signal) {
    process.kill(process.pid, signal);
    return;
  }
  process.exit(code ?? 0);
});

function findPython() {
  const candidates = process.platform === 'win32'
    ? [
        { command: 'py', prefixArgs: ['-3'] },
        { command: 'python', prefixArgs: [] },
        { command: 'python3', prefixArgs: [] },
      ]
    : [
        { command: 'python3', prefixArgs: [] },
        { command: 'python', prefixArgs: [] },
      ];

  for (const candidate of candidates) {
    try {
      const result = spawnSync(candidate.command, [...candidate.prefixArgs, '--version'], {
        cwd: repoRoot,
        env: process.env,
        stdio: 'ignore',
      });
      if (result.status === 0) {
        return candidate;
      }
    } catch {
      // Try the next interpreter candidate.
    }
  }

  return null;
}
