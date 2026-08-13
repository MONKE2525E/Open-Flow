#!/usr/bin/env node

import { spawn } from 'node:child_process';
import { fileURLToPath } from 'node:url';

const DEV_PORT = 1420;
const SERVER_PROBE_ATTEMPTS = 3;
const SERVER_PROBE_TIMEOUT_MS = 3000;
const DEV_URLS = [
  `http://127.0.0.1:${DEV_PORT}/`,
  `http://localhost:${DEV_PORT}/`,
  `http://[::1]:${DEV_PORT}/`,
];

const runningUrl = await findReachableServerUrl();

if (runningUrl) {
  console.log(`[vite-dev-server] Reusing existing dev server at ${runningUrl}`);
  process.exit(0);
}

const viteEntry = fileURLToPath(new URL('../node_modules/vite/bin/vite.js', import.meta.url));
const child = spawn(process.execPath, [viteEntry], {
  stdio: 'inherit',
  shell: false,
  env: process.env,
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

async function findReachableServerUrl() {
  // Vite can take a moment to answer while dependency optimization is in
  // progress. A single short probe can miss an already-running server and
  // then launch a second strictPort instance, which makes beforeDevCommand
  // fail with "Port 1420 is already in use".
  for (let attempt = 0; attempt < SERVER_PROBE_ATTEMPTS; attempt += 1) {
    for (const url of DEV_URLS) {
      if (await isServerReachable(url)) {
        return url;
      }
    }
    if (attempt < SERVER_PROBE_ATTEMPTS - 1) {
      await new Promise((resolve) => setTimeout(resolve, 250));
    }
  }

  return null;
}

async function isServerReachable(url) {
  const controller = new AbortController();
  const timeout = setTimeout(() => controller.abort(), SERVER_PROBE_TIMEOUT_MS);
  try {
    const response = await fetch(url, {
      method: 'GET',
      signal: controller.signal,
    });
    return response.ok || response.status < 500;
  } catch {
    return false;
  } finally {
    clearTimeout(timeout);
  }
}
