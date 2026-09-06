# macOS code signing & permission stability

## TL;DR

On macOS, build locally with:

```bash
npm run tauri:build:signed
```

This signs the app with a **stable self-signed identity** ("Open Flow Self-Signed")
so that the Accessibility permission you grant **survives rebuilds**. Building
with plain `npm run tauri build` (ad-hoc signing) will make the app forget its
permission every time you rebuild.

> The global hotkey now uses Carbon `RegisterEventHotKey` (no Input Monitoring
> permission), so **Accessibility** (for Cmd+V injection / AX reads) is the only
> rebuild-sensitive TCC grant left. Microphone self-heals (see below).

## Why this matters

macOS TCC (Transparency, Consent & Control) ties the **Accessibility** grant to
the application's *code-signature identity*, not its path or bundle id.

- An **ad-hoc** signature (`signingIdentity: "-"` in `tauri.conf.json`) has no
  stable identity. TCC falls back to keying the grant on the binary's `CDHash`,
  which is a hash of the compiled executable.
- **Every rebuild produces a different `CDHash`.** So a permission you granted to
  build A is not recognised by build B.

The symptom is confusing: **System Settings → Privacy & Security → Accessibility
shows the app as enabled, but the app still behaves as if permission is missing**
(clipboard injection silently stops working). That is because the System Settings
list matches the entry by path for *display*, while the runtime
`AXIsProcessTrusted()` check compares the *running* binary's signature against
what TCC actually authorised — and they don't match after a rebuild.

> Microphone is the exception: the mic path re-prompts through AVFoundation
> (re-establishing the grant against the current binary) and also has an empirical
> "I actually captured audio" latch, so it self-heals. Accessibility cannot
> re-prompt the same way.

Signing every local/release build with **one reused certificate** gives the
bundle a stable designated requirement, so a single grant persists across all
future rebuilds.

> **Never regenerate the certificate.** The cert is a long-lived secret. Every
> grant already issued (yours *and* end users' across app updates) is tied to that
> exact cert's designated requirement. A fresh cert = a different DR = every grant
> breaks. `scripts/macos/ensure-signing-identity.sh` therefore only *verifies* or
> *imports* the canonical cert — it will never create a new one.

## Setup

The canonical cert ("Open Flow Self-Signed") and its keychain already exist on the
primary dev machine:

- Cert material: `~/.openflow-signing/` (`openflow-signing.p12`, `cert.pem`, `key.pem`).
- Imported into a dedicated keychain `~/Library/Keychains/openflow-build.keychain-db`.

Secrets (keychain + p12 passwords) are **never committed**. Put them in an
untracked file the scripts source automatically, `~/.openflow-signing/env.sh`:

```bash
# ~/.openflow-signing/env.sh  (do NOT commit)
export OPENFLOW_SIGNING_KEYCHAIN_PASSWORD="…"   # build keychain password
export OPENFLOW_SIGNING_P12_PASSWORD="…"        # only needed to (re)import the p12
```

To verify the identity is present (and import it from the p12 if missing):

```bash
npm run macos:signing-identity
```

`npm run tauri:build:signed` runs this automatically, so you usually don't need
to call it directly.

**On a new machine:** copy `~/.openflow-signing/` over (or restore the p12 from
your password manager), create the `env.sh` above, then run
`npm run macos:signing-identity` to import it. Do **not** generate a new cert.

## Building

| Command | Signing | Use for |
|---|---|---|
| `npm run tauri:build:signed` | Stable self-signed identity | **Local macOS builds** — permissions persist |
| `npm run tauri build` | Ad-hoc (`-`) | Quick throwaway builds; permissions reset each rebuild |

To target a specific architecture, forward args after `--`:

```bash
npm run tauri:build:signed -- --target aarch64-apple-darwin
```

## CI / distributed installers

The `build-installers.yml` workflow keeps **ad-hoc** signing — GitHub runners have
no certificate, and the produced DMGs are non-notarized by design. End users
grant permissions once for the version they install; the per-rebuild churn only
affects developers iterating locally, which is exactly what the stable identity
above fixes. Proper distribution (a real Developer ID certificate + notarization)
is tracked separately and is out of scope here.

## Recovering from stale grants

If you previously installed ad-hoc builds and the OS is holding a stale entry,
the in-app **Settings → Permissions → "Reset stale grants"** button runs
`tccutil reset Accessibility com.verenu.app`, then walks you through re-adding the
app. After switching to signed builds you should only have to grant once.

## Runtime resilience

Independently of signing, the permission snapshot uses empirical latches so a
confirmed-working capability is trusted even if the raw TCC status query is stale:

- **Microphone** — a successful capture (`mark_microphone_verified`).
- **Accessibility** — a successful cross-process AX read in the caret probe
  (`mark_accessibility_verified`).

The raw OS check is still surfaced separately in the snapshot's
`diagnostics.accessibilityTrusted` field for debugging.
