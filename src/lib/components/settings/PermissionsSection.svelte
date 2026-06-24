<script lang="ts">
  import { invoke } from '../../tauri';
  import MacPermissions from '../MacPermissions.svelte';
  import type { ProviderId } from '../../settings';

  // Surface the Keychain row for whichever provider's key is configured.
  let provider = $state<ProviderId | null>(null);

  invoke<string | null>('get_setting', { key: 'transcription_provider' })
    .then((p) => { if (p) provider = p as ProviderId; })
    .catch(() => {});
</script>

<h2 class="settings-h">Permissions</h2>
<p class="panel-note">
  Verenu needs these macOS permissions to capture your voice and type into other
  apps. Anything not granted will stop dictation from working everywhere.
</p>

<MacPermissions variant="settings" {provider} />

<style>
  h2 { margin-bottom: 6px; }
</style>
