#!/usr/bin/env node

import { spawn, spawnSync } from 'node:child_process';
import { existsSync } from 'node:fs';
import path from 'node:path';

const APP_BINARY_NAME = 'verenu';

const args = process.argv.slice(2);

if (process.platform !== 'darwin' || args[0] !== 'run') {
  const result = spawnSync('cargo', args, {
    env: process.env,
    stdio: 'inherit',
  });
  process.exit(result.status ?? 1);
}

const runArgs = args.slice(1);
const separatorIndex = runArgs.indexOf('--');
const cargoArgs = separatorIndex === -1 ? runArgs : runArgs.slice(0, separatorIndex);
const appArgs = separatorIndex === -1 ? [] : runArgs.slice(separatorIndex + 1);

const buildResult = spawnSync('cargo', ['build', ...cargoArgs], {
  env: process.env,
  stdio: 'inherit',
});

if (buildResult.status !== 0) {
  process.exit(buildResult.status ?? 1);
}

const sourceBinary = path.join(
  resolveTargetDir(cargoArgs),
  ...targetSubdirs(cargoArgs),
  resolveProfile(cargoArgs),
  resolveBinaryName(cargoArgs),
);

if (!existsSync(sourceBinary)) {
  console.error(`Expected built binary was not found: ${sourceBinary}`);
  process.exit(1);
}

const child = spawn(sourceBinary, appArgs, {
  env: process.env,
  stdio: 'inherit',
});

for (const signal of ['SIGINT', 'SIGTERM', 'SIGHUP']) {
  process.on(signal, () => {
    child.kill(signal);
  });
}

child.on('exit', (code, signal) => {
  if (signal) {
    process.kill(process.pid, signal);
    return;
  }
  process.exit(code ?? 0);
});

function resolveTargetDir(cargoArgs) {
  const targetDir = optionValue(cargoArgs, '--target-dir') ?? process.env.CARGO_TARGET_DIR;
  if (targetDir) {
    return path.resolve(targetDir);
  }

  const metadata = spawnSync('cargo', ['metadata', '--no-deps', '--format-version=1'], {
    encoding: 'utf8',
    env: process.env,
  });

  if (metadata.status === 0 && metadata.stdout) {
    try {
      return JSON.parse(metadata.stdout).target_directory;
    } catch {
      // Fall through to Cargo's default target directory.
    }
  }

  return path.resolve('target');
}

function targetSubdirs(cargoArgs) {
  const target = optionValue(cargoArgs, '--target');
  return target ? [target] : [];
}

function resolveProfile(cargoArgs) {
  return cargoArgs.includes('--release') ? 'release' : optionValue(cargoArgs, '--profile') ?? 'debug';
}

function resolveBinaryName(cargoArgs) {
  return optionValue(cargoArgs, '--bin') ?? APP_BINARY_NAME;
}

function optionValue(args, name) {
  const inlinePrefix = `${name}=`;
  for (let i = 0; i < args.length; i += 1) {
    const arg = args[i];
    if (arg.startsWith(inlinePrefix)) {
      return arg.slice(inlinePrefix.length);
    }
    if (arg === name) {
      return args[i + 1];
    }
  }
  return undefined;
}
