#!/usr/bin/env bash
#
# Build a macOS release bundle signed with the stable, reused self-signed identity
# so TCC permission grants (Accessibility / Input Monitoring) survive rebuilds.
# See scripts/macos/ensure-signing-identity.sh and docs/macos-code-signing.md.
#
# Extra args are forwarded to `tauri build`, e.g.:
#   npm run tauri:build:signed -- --target aarch64-apple-darwin
#
# SECRETS: none are committed here. The build-keychain password is read from your
# environment (or an untracked `~/.openflow-signing/env.sh` that this sources).
set -euo pipefail

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "build-signed: not macOS; run 'npm run tauri build' instead." >&2
  exit 1
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

LOCAL_ENV="${OPENFLOW_SIGNING_ENV:-$HOME/.openflow-signing/env.sh}"
# shellcheck source=/dev/null
[[ -f "$LOCAL_ENV" ]] && source "$LOCAL_ENV"

export APPLE_SIGNING_IDENTITY="${APPLE_SIGNING_IDENTITY:-Open Flow Self-Signed}"
KEYCHAIN_PATH="${OPENFLOW_SIGNING_KEYCHAIN:-$HOME/Library/Keychains/openflow-build.keychain-db}"

# 1. Verify the canonical identity exists (imports it if you've provided a p12).
"$SCRIPT_DIR/ensure-signing-identity.sh"

# 2. Make sure the signing keychain is in the search list and unlocked, so the
#    Tauri bundler's codesign call runs without a GUI prompt.
if [[ -f "$KEYCHAIN_PATH" ]]; then
  if ! security list-keychains -d user | grep -qF "$KEYCHAIN_PATH"; then
    # Prepend the build keychain to the user search list without dropping others.
    EXISTING="$(security list-keychains -d user | sed -e 's/^[[:space:]]*"//' -e 's/"$//')"
    # shellcheck disable=SC2086
    security list-keychains -d user -s "$KEYCHAIN_PATH" $EXISTING >/dev/null 2>&1 || true
  fi
  if [[ -n "${OPENFLOW_SIGNING_KEYCHAIN_PASSWORD:-}" ]]; then
    security unlock-keychain -p "$OPENFLOW_SIGNING_KEYCHAIN_PASSWORD" "$KEYCHAIN_PATH" \
      || echo "build-signed: could not unlock $KEYCHAIN_PATH; codesign may prompt."
    # Keep the keychain unlocked for the whole build (it can otherwise auto-relock
    # mid-compile, making the final codesign fail with errSecInternalComponent).
    security set-keychain-settings "$KEYCHAIN_PATH" >/dev/null 2>&1 || true
    # Authorise codesign to use the private key non-interactively. Without this the
    # bundler's codesign call fails with errSecInternalComponent in a non-GUI shell.
    security set-key-partition-list -S apple-tool:,apple:,codesign: -s \
      -k "$OPENFLOW_SIGNING_KEYCHAIN_PASSWORD" "$KEYCHAIN_PATH" >/dev/null 2>&1 || true
  else
    echo "build-signed: OPENFLOW_SIGNING_KEYCHAIN_PASSWORD not set; macOS may prompt to unlock the keychain / allow codesign (click 'Always Allow' once)."
  fi
fi

echo "build-signed: building with APPLE_SIGNING_IDENTITY='$APPLE_SIGNING_IDENTITY'"

# 3. Build. APPLE_SIGNING_IDENTITY overrides tauri.conf.json's "-" (ad-hoc).
npm run tauri build -- "$@"
