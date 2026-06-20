#!/usr/bin/env bash
#
# Verify the stable macOS code-signing identity is available before a signed
# build. See docs/macos-code-signing.md.
#
# WHY THIS MATTERS
# ----------------
# macOS TCC ties Accessibility and Input Monitoring grants to the app's code
# signature (its "designated requirement"), NOT its path or bundle id. Ad-hoc
# signing (`signingIdentity: "-"`) has no stable identity, so every rebuild gets a
# different CDHash and the OS treats it as a brand-new app — silently dropping
# every permission the user granted. Symptom: System Settings shows the app as
# enabled, but the hotkey/injection stop working after a rebuild.
#
# The fix is to sign every local/release build with ONE reused self-signed
# certificate ("Open Flow Self-Signed"). The certificate is the long-lived secret:
# it must be the SAME cert forever, because end users' grants (and yours) are tied
# to that exact cert's designated requirement. Generating a fresh certificate
# would change the DR and break every existing grant — so this script NEVER
# generates one. It only verifies the canonical cert is present, optionally
# importing it from a p12 you provide.
#
# SECRETS: this committed script contains none. Passwords come from your
# environment (or an untracked `~/.openflow-signing/env.sh` that it sources).
set -euo pipefail

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "ensure-signing-identity: not macOS, nothing to do."
  exit 0
fi

# Optional untracked file with local secrets (never committed):
#   export OPENFLOW_SIGNING_KEYCHAIN_PASSWORD=...
#   export OPENFLOW_SIGNING_P12_PASSWORD=...
LOCAL_ENV="${OPENFLOW_SIGNING_ENV:-$HOME/.openflow-signing/env.sh}"
# shellcheck source=/dev/null
[[ -f "$LOCAL_ENV" ]] && source "$LOCAL_ENV"

IDENTITY_NAME="${APPLE_SIGNING_IDENTITY:-Open Flow Self-Signed}"
KEYCHAIN_PATH="${OPENFLOW_SIGNING_KEYCHAIN:-$HOME/Library/Keychains/openflow-build.keychain-db}"
P12_PATH="${OPENFLOW_SIGNING_P12:-$HOME/.openflow-signing/openflow-signing.p12}"

identity_present() {
  # find-identity searches all keychains in the list; also probe the specific
  # build keychain directly in case it isn't in the default search list.
  security find-identity -p codesigning 2>/dev/null | grep -qF "$IDENTITY_NAME" \
    || security find-identity -p codesigning "$KEYCHAIN_PATH" 2>/dev/null | grep -qF "$IDENTITY_NAME"
}

if identity_present; then
  echo "ensure-signing-identity: '$IDENTITY_NAME' is available."
  exit 0
fi

# Not present — try to import the canonical p12 (never generate a new cert).
if [[ -f "$P12_PATH" && -n "${OPENFLOW_SIGNING_P12_PASSWORD:-}" ]]; then
  echo "ensure-signing-identity: importing '$IDENTITY_NAME' from $P12_PATH..."
  if [[ ! -f "$KEYCHAIN_PATH" ]]; then
    echo "  creating keychain $KEYCHAIN_PATH"
    security create-keychain -p "${OPENFLOW_SIGNING_KEYCHAIN_PASSWORD:-}" "$KEYCHAIN_PATH"
  fi
  security unlock-keychain -p "${OPENFLOW_SIGNING_KEYCHAIN_PASSWORD:-}" "$KEYCHAIN_PATH" 2>/dev/null || true
  security import "$P12_PATH" -k "$KEYCHAIN_PATH" -P "$OPENFLOW_SIGNING_P12_PASSWORD" \
    -T /usr/bin/codesign -T /usr/bin/security >/dev/null
  # Pre-authorise codesign so it doesn't prompt on every build.
  security set-key-partition-list -S apple-tool:,apple:,codesign: -s \
    -k "${OPENFLOW_SIGNING_KEYCHAIN_PASSWORD:-}" "$KEYCHAIN_PATH" >/dev/null 2>&1 || true
  if identity_present; then
    echo "ensure-signing-identity: imported '$IDENTITY_NAME'."
    exit 0
  fi
fi

cat >&2 <<EOF
ensure-signing-identity: signing identity '$IDENTITY_NAME' was not found and
could not be imported.

Do NOT generate a new certificate — that changes the TCC designated requirement
and breaks every permission grant already issued to this app (yours and end
users'). Instead import the canonical certificate:

  security import <openflow-signing.p12> \\
    -k "$KEYCHAIN_PATH" -P <p12-password> \\
    -T /usr/bin/codesign -T /usr/bin/security

or set these (e.g. in $LOCAL_ENV) and re-run:

  export OPENFLOW_SIGNING_P12=<path to openflow-signing.p12>
  export OPENFLOW_SIGNING_P12_PASSWORD=<p12 password>
  export OPENFLOW_SIGNING_KEYCHAIN_PASSWORD=<build keychain password>

See docs/macos-code-signing.md.
EOF
exit 1
