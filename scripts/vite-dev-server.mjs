#!/usr/bin/env node

import { spawn } from 'node:child_process';

const DEV_PORT = 1420;
const DEV_URLS = [
  `http://localhost:${DEV_PORT}/`,
  `http://127.0.0.1:${DEV_PORT}/`,
  `http://[::1]:${DEV_PORT}/`,
];

const runningUrl = await findReachableServerUrl();

if (runningUrl) {
  console.log(`[vite-dev-server] Reusing existing dev server at ${runningUrl}`);
  process.exit(0);
}

const child = spawn('vite', [], {
  stdio: 'inherit',
  shell: true,
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
  for (const url of DEV_URLS) {
    if (await isServerReachable(url)) {
      return url;
    }
  }

  return null;
}

async function isServerReachable(url) {
  const controller = new AbortController();
  const timeout = setTimeout(() => controller.abort(), 1000);
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
