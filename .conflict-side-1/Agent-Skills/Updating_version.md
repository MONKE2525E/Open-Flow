# Updating Version

When you are asked to update the application version (e.g., bump the version to v0.2.0, update version, etc.), you **MUST** update the version string synchronously across the following three files:

1. `package.json`
   - Update the `"version"` field at the root of the JSON object.
2. `src-tauri/tauri.conf.json`
   - Update the `"version"` field at the root of the JSON object.
3. `src-tauri/Cargo.toml`
   - Update the `version` field under the `[package]` section.

**Crucial:** Ensure the version numbers match exactly in all three files to prevent build issues and keep the frontend and backend in sync.

4. `AGENTS.md`
   - Update the version note under "Notes from user" (the "Latest release is X.Y.Z" line). This drifted before (stuck at 0.15.0 while the app shipped 0.17.0), so it is part of this checklist now.

**Note:** The version is now read dynamically from `@tauri-apps/api/app` via `getVersion()` in `Settings.svelte` and `Home.svelte`, so hardcoded version strings in those files are no longer used. The three version files above are the only source of truth for versioning; the AGENTS.md note is documentation, not a build input.
