# Updating Version

When you are asked to update the application version (e.g., bump the version to v0.2.0, update version, etc.), you **MUST** update the version string synchronously across the following three files:

1. `package.json`
   - Update the `"version"` field at the root of the JSON object.
2. `src-tauri/tauri.conf.json`
   - Update the `"version"` field at the root of the JSON object.
3. `src-tauri/Cargo.toml`
   - Update the `version` field under the `[package]` section.

**Crucial:** Ensure the version numbers match exactly in all three files to prevent build issues and keep the frontend and backend in sync.