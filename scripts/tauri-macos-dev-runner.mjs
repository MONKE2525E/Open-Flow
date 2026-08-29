#!/usr/bin/env node

import { spawn, spawnSync } from 'node:child_process';
import { chmodSync, copyFileSync, existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { constants as osConstants } from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const APP_BINARY_NAME = 'verenu';
const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, '..');
const devConfig = JSON.parse(readFileSync(path.join(repoRoot, 'src-tauri', 'tauri.dev.conf.json'), 'utf8'));
const BUNDLE_IDENTIFIER = devConfig.identifier;
const APP_DISPLAY_NAME = devConfig.productName;
const APP_BUNDLE_BASENAME = devConfig?.bundle?.macOS?.bundleName;

if (
  BUNDLE_IDENTIFIER !== 'com.verenu.app.dev' ||
  APP_DISPLAY_NAME !== 'Verenu Development' ||
  typeof APP_BUNDLE_BASENAME !== 'string' ||
  APP_BUNDLE_BASENAME.length === 0
) {
  console.error('Invalid macOS development identity contract in src-tauri/tauri.dev.conf.json.');
  process.exit(1);
}
const APP_BUNDLE_NAME = `${APP_BUNDLE_BASENAME}.app`;

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

const bundledBinary = prepareSignedDevBundle(sourceBinary);
if (process.env.VERENU_DEV_RUNNER_PREPARE_ONLY === '1') {
  process.exit(0);
}
const appBundle = path.dirname(path.dirname(path.dirname(bundledBinary)));
console.log(`[macOS dev runner] Launching signed bundle through LaunchServices: ${appBundle}`);
const existingPids = new Set(findBundleProcessIds(bundledBinary));
const openArgs = ['-n', appBundle];
if (appArgs.length > 0) openArgs.push('--args', ...appArgs);
const child = spawn('/usr/bin/open', openArgs, {
  env: process.env,
  stdio: 'inherit',
});
let launchedPids = [];
let sawLaunchedProcess = false;
let emptySince = 0;
const launchStartedAt = Date.now();
const launchTimeoutMs = 10_000;
const processTracker = setInterval(() => {
  launchedPids = findBundleProcessIds(bundledBinary).filter((pid) => !existingPids.has(pid));
  if (launchedPids.length > 0) {
    sawLaunchedProcess = true;
    emptySince = 0;
  } else if (!sawLaunchedProcess && Date.now() - launchStartedAt > launchTimeoutMs) {
    clearInterval(processTracker);
    console.error(`[macOS dev runner] LaunchServices did not start ${appBundle} within ${launchTimeoutMs}ms.`);
    try { child.kill(); } catch { /* open may already have exited */ }
    process.exit(1);
  } else if (sawLaunchedProcess) {
    if (emptySince === 0) emptySince = Date.now();
    // Relaunch intentionally has a short gap between the old process exiting
    // and LaunchServices creating the replacement. Keep Vite/Tauri alive
    // through that handoff, but exit normally after a real app quit.
    if (Date.now() - emptySince > 2500) {
      clearInterval(processTracker);
      console.log('[macOS dev runner] Launched app process ended; LaunchServices does not expose a termination status, so the runner is exiting normally.');
      // Let Node terminate naturally after the tracker is cleared. This
      // preserves the normal-shutdown status without masking a signal-driven
      // termination in the signal handlers above.
      return;
    }
  }
}, 250);

for (const signal of ['SIGINT', 'SIGTERM', 'SIGHUP']) {
  process.on(signal, () => {
    clearInterval(processTracker);
    launchedPids = findBundleProcessIds(bundledBinary).filter((pid) => !existingPids.has(pid));
    for (const pid of launchedPids) {
      try { process.kill(pid, signal); } catch { /* process already exited */ }
    }
    child.kill(signal);
    process.exit(128 + (osConstants.signals[signal] ?? 1));
  });
}

child.on('exit', (code, signal) => {
  if (signal) {
    clearInterval(processTracker);
    process.exit(1);
    return;
  }
  if ((code ?? 0) !== 0) {
    clearInterval(processTracker);
    process.exit(code ?? 1);
  }
});

function findBundleProcessIds(executablePath) {
  const result = spawnSync('/bin/ps', ['-axww', '-o', 'pid=,command='], { encoding: 'utf8' });
  if (result.status !== 0) return [];
  return result.stdout
    .split('\n')
    .map((line) => line.trim().match(/^(\d+)\s+(.+)$/))
    .filter((match) => match && (match[2] === executablePath || match[2].startsWith(`${executablePath} `)))
    .map((match) => Number(match[1]));
}

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

function prepareSignedDevBundle(sourceBinary) {
  const profileDir = path.dirname(sourceBinary);
  const appBundle = path.join(profileDir, 'bundle', 'macos-dev', APP_BUNDLE_NAME);
  const contentsDir = path.join(appBundle, 'Contents');
  const macosDir = path.join(contentsDir, 'MacOS');
  const resourcesDir = path.join(contentsDir, 'Resources');
  const bundledBinary = path.join(macosDir, 'Verenu');
  const infoPlist = path.join(contentsDir, 'Info.plist');
  const normalizedEntitlements = path.join(profileDir, 'verenu-dev-entitlements.plist');

  mkdirSync(macosDir, { recursive: true });
  mkdirSync(resourcesDir, { recursive: true });
  copyFileSync(sourceBinary, bundledBinary);
  chmodSync(bundledBinary, 0o755);
  copyFileSync(path.join(repoRoot, 'src-tauri', 'Info.plist'), infoPlist);
  copyFileSync(path.join(repoRoot, 'src-tauri', 'icons', 'icon.icns'), path.join(resourcesDir, 'icon.icns'));

  setPlistValue(infoPlist, 'CFBundleIdentifier', BUNDLE_IDENTIFIER);
  setPlistValue(infoPlist, 'CFBundleExecutable', 'Verenu');
  setPlistValue(infoPlist, 'CFBundleDisplayName', APP_DISPLAY_NAME);
  setPlistValue(infoPlist, 'CFBundleName', APP_DISPLAY_NAME);
  setPlistValue(infoPlist, 'CFBundlePackageType', 'APPL');
  const entitlementResult = spawnSync(
    '/usr/bin/plutil',
    [
      '-convert',
      'xml1',
      '-o',
      normalizedEntitlements,
      path.join(repoRoot, 'src-tauri', 'Entitlements.plist'),
    ],
    { encoding: 'utf8' },
  );
  if (entitlementResult.status !== 0) {
    console.error(entitlementResult.stderr || 'Could not normalize development entitlements.');
    process.exit(entitlementResult.status ?? 1);
  }

  const identity = resolveSigningIdentity();
  const signResult = spawnSync(
    '/usr/bin/codesign',
    [
      '--force',
      '--deep',
      '--options',
      'runtime',
      '--timestamp=none',
      '--entitlements',
      normalizedEntitlements,
      '--sign',
      identity,
      appBundle,
    ],
    { encoding: 'utf8', env: process.env },
  );
  if (signResult.status !== 0) {
    console.error(signResult.stderr || signResult.stdout || 'codesign failed');
    process.exit(signResult.status ?? 1);
  }

  const verifyResult = spawnSync('/usr/bin/codesign', ['--verify', '--deep', '--strict', appBundle], {
    encoding: 'utf8',
    env: process.env,
  });
  if (verifyResult.status !== 0) {
    console.error(verifyResult.stderr || 'Signed development bundle failed verification.');
    process.exit(verifyResult.status ?? 1);
  }

  verifyPreparedIdentity(appBundle, identity);
  warnAboutLegacyDevelopmentCopies(profileDir, appBundle);

  console.log(`[macOS dev runner] Prepared signed bundle: ${appBundle}`);
  return bundledBinary;
}

function setPlistValue(plistPath, key, value) {
  const result = spawnSync('/usr/bin/plutil', ['-replace', key, '-string', value, plistPath], {
    encoding: 'utf8',
  });
  if (result.status !== 0) {
    // The source plist intentionally contains only merge keys, so add fields
    // that are not already present.
    const addResult = spawnSync('/usr/bin/plutil', ['-insert', key, '-string', value, plistPath], {
      encoding: 'utf8',
    });
    if (addResult.status !== 0) {
      console.error(addResult.stderr || `Could not set ${key} in development Info.plist.`);
      process.exit(addResult.status ?? 1);
    }
  }
}

function resolveSigningIdentity() {
  const configured = (
    process.env.VERENU_DEV_SIGNING_IDENTITY ?? process.env.APPLE_SIGNING_IDENTITY
  )?.trim();
  if (configured && configured !== '-') {
    return configured;
  }

  const identities = spawnSync('/usr/bin/security', ['find-identity', '-v', '-p', 'codesigning'], {
    encoding: 'utf8',
  });
  const match = identities.stdout?.match(/"(Apple Development:[^"]+)"/);
  if (match) return match[1];

  console.error(
    'macOS development requires a stable code-signing identity so TCC permissions survive rebuilds.\n' +
      'Install an Apple Development certificate or set APPLE_SIGNING_IDENTITY, then retry.',
  );
  process.exit(1);
}

function verifyPreparedIdentity(appBundle, expectedIdentity) {
  const plist = path.join(appBundle, 'Contents', 'Info.plist');
  const bundleId = plistValue(plist, 'CFBundleIdentifier');
  const displayName = plistValue(plist, 'CFBundleDisplayName');
  const signature = spawnSync('/usr/bin/codesign', ['-dv', '--verbose=4', appBundle], {
    encoding: 'utf8',
  });
  const signingText = `${signature.stdout ?? ''}\n${signature.stderr ?? ''}`;
  const expectedAuthority = resolveSigningAuthority(expectedIdentity);
  if (
    bundleId !== BUNDLE_IDENTIFIER ||
    displayName !== APP_DISPLAY_NAME ||
    !signingText.includes(`Authority=${expectedAuthority}`) ||
    signingText.includes('Signature=adhoc')
  ) {
    console.error('Prepared development bundle failed its identity contract.');
    console.error(`bundle=${bundleId} display=${displayName}`);
    process.exit(1);
  }
}

function resolveSigningAuthority(identity) {
  // `codesign --sign` accepts either a certificate common name or its
  // 40-character SHA-1 hash. `codesign -dv` reports the common name in the
  // Authority field, so resolve hashes through the local identity catalogue
  // before comparing the signed bundle.
  if (!/^[0-9a-f]{40}$/i.test(identity)) return identity;
  const identities = spawnSync('/usr/bin/security', ['find-identity', '-v', '-p', 'codesigning'], {
    encoding: 'utf8',
  });
  const escaped = identity.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
  const match = identities.stdout?.match(new RegExp(`${escaped}\\s+\\"([^\\"]+)\\"`, 'i'));
  return match?.[1] ?? identity;
}

function plistValue(plist, key) {
  const result = spawnSync('/usr/bin/plutil', ['-extract', key, 'raw', plist], { encoding: 'utf8' });
  return result.status === 0 ? result.stdout.trim() : '';
}

function warnAboutLegacyDevelopmentCopies(profileDir, canonicalBundle) {
  const marker = path.join(profileDir, '.verenu-dev-identity-migration-v1');
  if (existsSync(marker)) return;
  const candidates = [
    path.join(profileDir, 'bundle', 'macos-dev', 'Verenu.app'),
    '/Applications/Verenu Development.app',
  ].filter((candidate) => candidate !== canonicalBundle && existsSync(candidate));
  if (candidates.length > 0) {
    console.warn('[macOS dev runner] Legacy/duplicate development app copies detected:');
    for (const candidate of candidates) console.warn(`  ${candidate}`);
    console.warn(
      'Ignore or remove those copies manually. Grant permissions only to the canonical Verenu Development app shown above.',
    );
  }
  writeFileSync(marker, `${new Date().toISOString()}\n`, { mode: 0o600 });
}
