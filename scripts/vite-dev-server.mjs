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

const running = await findReachableServer();

if (running?.kind === 'dev') {
  console.log(`[vite-dev-server] Reusing existing dev server at ${running.url}`);
  process.exit(0);
}

if (running?.kind === 'not-dev') {
  // Bailing out beats letting Vite fail on strictPort with a generic
  // "port in use", because the cause is specific and the fix is one command.
  console.error(
    `[vite-dev-server] ${running.url} is serving Verenu, but it is not a dev server ` +
      '(no HMR client in the HTML) - most likely `vite preview`, which serves whatever ' +
      'was last built into dist/.',
  );
  console.error(
    '[vite-dev-server] Tauri would load that stale build, so every source edit would ' +
      'appear to do nothing. Stop that server, then start `npm run tauri dev` again.',
  );
  process.exit(1);
}

const viteEntry = fileURLToPath(new URL('../node_modules/vite/bin/vite.js', import.meta.url));
const child = spawn(process.execPath, [viteEntry], {
  stdio: 'inherit',
  shell: false,
  env: process.env,
  windowsHide: true,
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

async function findReachableServer() {
  // Vite can take a moment to answer while dependency optimization is in
  // progress. A single short probe can miss an already-running server and
  // then launch a second strictPort instance, which makes beforeDevCommand
  // fail with "Port 1420 is already in use".
  for (let attempt = 0; attempt < SERVER_PROBE_ATTEMPTS; attempt += 1) {
    for (const url of DEV_URLS) {
      const kind = await classifyServer(url);
      if (kind) {
        return { url, kind };
      }
    }
    if (attempt < SERVER_PROBE_ATTEMPTS - 1) {
      await new Promise((resolve) => setTimeout(resolve, 250));
    }
  }

  return null;
}

/** `'dev'`, `'not-dev'`, or null when nothing Verenu-shaped answers. */
async function classifyServer(url) {
  const controller = new AbortController();
  const timeout = setTimeout(() => controller.abort(), SERVER_PROBE_TIMEOUT_MS);
  try {
    const response = await fetch(url, {
      method: 'GET',
      signal: controller.signal,
    });
    if (!response.ok) {
      return null;
    }

    // Port 1420 is a convention, not proof that this project's Vite server is
    // running. Reusing an unrelated local service makes Tauri load the wrong
    // page and leaves the desktop window looking blank or transparent.
    const html = await response.text();
    if (!html.includes('<title>Verenu</title>')) {
      return null;
    }

    // The title alone is not enough: `vite preview` serves the built dist/ under
    // the same title, so reusing it pins the desktop window to the last build
    // and makes every source edit look like it did nothing. Only a dev server
    // injects the HMR client, so that is what "dev server" has to mean here.
    return html.includes('/@vite/client') ? 'dev' : 'not-dev';
  } catch {
    return null;
  } finally {
    clearTimeout(timeout);
  }
}
