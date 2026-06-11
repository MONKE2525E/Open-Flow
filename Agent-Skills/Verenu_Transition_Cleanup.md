# Verenu Transition Cleanup

The Open Flow → Verenu rename (target version 0.12.1) shipped with temporary,
self-migrating shims so existing users' data, API keys, and OS registrations
move from the old "Open Flow" / "OpenFlow" / "open-flow" / `com.openflow.app`
identity to the new "Verenu" / `com.verenu.app` identity on first launch.

This skill describes how to remove those shims in a future patch, **once all
known users are confirmed to be on >=0.12.1** (at the time of writing there
are only 3 users — confirm with the user before proceeding).

## 1. Find every shim

```bash
grep -rn "TRANSITION(verenu)" src-tauri/src
```

Every temporary piece of code carries this exact marker in a comment or log
message. The sections below map each marker to what to delete.

## 2. Updater — drop the legacy repo from the dual-check

`src-tauri/src/api/updater.rs`:
- Remove the `"MONKE2525E/Open-Flow"` entry from `RELEASE_REPOS`, leaving only
  `"MONKE2525E/Verenu"`.
- Remove the `TRANSITION(verenu)` doc comment above `RELEASE_REPOS` (or
  simplify it to describe a single-repo check).
- `check()` can stay as-is (looping a single-element slice is harmless), or be
  simplified back to a direct single call if preferred.

## 3. Credential / Keychain migration (`src-tauri/src/data/credentials.rs`)

Remove entirely:
- The `LEGACY_SERVICE` constant (Windows Credential Manager service name
  `"open-flow"`) and its doc comment.
- The Windows `migrate_legacy_service()` function — migrates
  `{user}.open-flow` Credential Manager entries to `{user}.verenu`, deleting
  the old entry after a successful write.
- The `LEGACY_KEYCHAIN_SERVICE` constant (macOS Keychain service name
  `"com.openflow.app"`) and its doc comment.
- The macOS `migrate_legacy_service()` function — migrates `com.openflow.app`
  Keychain items to `com.verenu.app`, deleting the old item after a
  successful write.
- The no-op `migrate_legacy_service()` for other platforms.
- The call site in `src-tauri/src/main.rs` setup:
  `crate::data::credentials::migrate_legacy_service();`

After removal, confirm no `open-flow` / `com.openflow.app` credential string
remains anywhere in `credentials.rs` (the macOS legacy plaintext
`credentials.json` path checks under `Library/Application Support/OpenFlow/`
are a **separate**, older legacy-format reader and are NOT part of this
cleanup — leave those alone unless doing a separate pass).

## 4. Database migration (`src-tauri/src/main.rs`)

Remove:
- `legacy_app_data_dir()` (all three OS variants) — the old `"OpenFlow"` /
  `~/Library/Application Support/OpenFlow` / `~/.config/OpenFlow` paths.
- `migrate_legacy_db(new_dir)` — copies `openflow.db` (+ `-wal`/`-shm`
  sidecars) into `verenu.db`.
- The call site in `main()`: `migrate_legacy_db(&db_dir);` (keep the
  surrounding `create_dir_all` + `db::open(...)` calls).

## 5. Autostart / LaunchAgent migration (`src-tauri/src/commands/mod.rs`)

Remove:
- `migrate_legacy_autostart(autostart_enabled)` (Windows) — deletes the stale
  `"OpenFlow"` Run-key value and re-registers under `"Verenu"` if autostart is
  enabled.
- `migrate_legacy_launch_agent(app, autostart_enabled)` (macOS) — boots out
  and removes `~/Library/LaunchAgents/com.openflow.app.plist`, then
  re-registers `com.verenu.app` if autostart is enabled.
- The call sites in `src-tauri/src/main.rs` setup:
  ```rust
  let autostart_enabled = store
      .get(crate::data::store::AUTOSTART_ENABLED)
      .and_then(|v| v.as_bool())
      .unwrap_or(false);
  #[cfg(target_os = "windows")]
  crate::commands::migrate_legacy_autostart(autostart_enabled);
  #[cfg(target_os = "macos")]
  crate::commands::migrate_legacy_launch_agent(app.handle(), autostart_enabled);
  ```
  (the `autostart_enabled` read can be removed too if nothing else uses it).

`set_windows_autostart()` and `set_macos_autostart()` (the non-prefixed
helpers used by `set_autostart`) are **not** shims — keep them.

## 6. About page "Source" link auto-detection

`src-tauri/src/api/updater.rs`:
- Remove `SOURCE_REPO_CANDIDATES` and `resolve_source_repo()`.

`src-tauri/src/commands/mod.rs`:
- Remove the `get_source_repo` command (the "about / source link" section).

`src-tauri/src/main.rs`:
- Remove `commands::get_source_repo` from the invoke handler.

`src/lib/components/settings/AboutSection.svelte`:
- Remove the `sourceRepo` state and the `$effect` that calls `get_source_repo`.
- Hardcode the Source button + `openRepo()` URL to
  `https://github.com/MONKE2525E/Verenu` (and update the displayed text to
  `github.com/MONKE2525E/Verenu`).

`src/lib/tauri.ts`:
- Remove the `get_source_repo` dev-mock case.

After this, update the frozen smoke test `tests/smoke/playwright-test-fixes.cjs`
(lines ~84-85) to assert `github.com/MONKE2525E/Verenu` instead of
`github.com/MONKE2525E/Open-Flow` — **with the user's agreement**, since smoke
tests are normally frozen.

## 7. Final verification after cleanup

- `grep -rn "TRANSITION(verenu)" src-tauri/src` returns nothing.
- `grep -rn "open-flow\|OpenFlow\|com.openflow.app" src-tauri/src` returns
  only the intentionally-unchanged items: the Tauri `identifier`
  (`com.openflow.app`, frozen by `playwright-test-pipeline.cjs`), the macOS
  legacy plaintext `credentials.json` reader paths, and any remaining
  `Open-Flow` GitHub repo references that are still load-bearing.
- `npm run check`, `npm run lint`, `npm run test:rust`, and
  `python tests/OnePyFone.py` (fast profile) all pass.

## 8. Remaining items not covered by this cleanup (v0.13+)

- The README clone URL still points at `github.com/MONKE2525E/Open-Flow`.
  Once the GitHub repo rename to `Verenu` is confirmed stable, swap it to
  `/Verenu`. (The About page "Source" link auto-detects this already — see
  section 6 — but the static README link needs a manual update.)
- `playwright-test-pipeline.cjs` hardcodes
  `%APPDATA%\com.openflow.app\settings.json` — the Tauri `identifier` is
  intentionally staying `com.openflow.app` indefinitely (changing it would
  require a settings-store migration), so this is expected to remain
  unchanged even after this cleanup.
