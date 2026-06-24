<script lang="ts">
  import { invoke } from '../tauri';
  import { onMount } from 'svelte';
  import {
    cleanAppName,
    getAppDisplayName,
    normalizeExe,
    type AppMapping,
    type InstalledApp,
  } from '../appMappings';
  import { motionMs } from '../motion';
  import AppMappingsAddForm from './appMappings/AppMappingsAddForm.svelte';
  import AppMappingsList from './appMappings/AppMappingsList.svelte';

  let {
    showHeading = true,
    intro = 'Use a different tone automatically when you type in specific apps.',
    emptyText = 'No app tones yet. Add one below.',
    addLabel = 'Add App Tone',
  }: {
    showHeading?: boolean;
    intro?: string;
    emptyText?: string;
    addLabel?: string;
  } = $props();

  let mappings = $state<AppMapping[]>([]);
  let installedApps = $state<InstalledApp[]>([]);
  let areAppsLoaded = $state(false);
  let mappingError = $state('');
  let leavingExes = $state<Set<string>>(new Set());

  const mappedExes = $derived(new Set(mappings.map((mapping) => normalizeExe(mapping.exe))));

  onMount(() => {
    loadMappings();
    loadInstalledApps();
  });

  async function loadMappings() {
    try {
      mappings = normalizeMappings(await invoke<AppMapping[]>('get_app_mappings'));
    } catch (err) {
      console.error('get_app_mappings failed:', err);
    }
  }

  async function loadInstalledApps() {
    if (areAppsLoaded) return;

    try {
      installedApps = (await invoke<InstalledApp[]>('get_installed_apps')).map((app) => ({
        exe: normalizeExe(app.exe),
        name: cleanAppName(app.name || app.exe),
      }));
      areAppsLoaded = true;
      mappings = normalizeMappings(mappings);
    } catch (err) {
      console.error('get_installed_apps failed:', err);
    }
  }

  async function saveMappings(updated: AppMapping[]): Promise<boolean> {
    mappings = normalizeMappings(updated);
    try {
      await invoke('save_app_mappings', { mappings });
      mappingError = '';
      return true;
    } catch (err) {
      console.error('save_app_mappings failed:', err);
      mappingError = 'Could not save app tones.';
      return false;
    }
  }

  async function deleteMapping(exe: string) {
    const normalizedExe = normalizeExe(exe);
    if (leavingExes.has(normalizedExe)) return;

    const previousMappings = mappings;
    leavingExes = new Set(leavingExes).add(normalizedExe);
    window.setTimeout(async () => {
      try {
        const saved = await saveMappings(mappings.filter((mapping) => normalizeExe(mapping.exe) !== normalizedExe));
        if (!saved) {
          mappings = normalizeMappings(previousMappings);
        }
      } finally {
        const nextLeaving = new Set(leavingExes);
        nextLeaving.delete(normalizedExe);
        leavingExes = nextLeaving;
      }
    }, motionMs(200));
  }

  async function updateMappingField(exe: string, patch: Partial<AppMapping>) {
    const previousMappings = mappings;
    const updated = mappings.map((mapping) =>
      mapping.exe === exe ? normalizeMapping({ ...mapping, ...patch }) : mapping,
    );
    const saved = await saveMappings(updated);
    if (!saved) {
      mappings = normalizeMappings(previousMappings);
    }
  }

  async function addMapping(entry: AppMapping) {
    return saveMappings([...mappings.filter((mapping) => mapping.exe !== normalizeExe(entry.exe)), normalizeMapping(entry)]);
  }

  function normalizeMappings(entries: AppMapping[]) {
    const seen = new Set<string>();
    return entries
      .map(normalizeMapping)
      .filter((entry) => {
        if (!entry.exe || seen.has(entry.exe)) return false;
        seen.add(entry.exe);
        return true;
      });
  }

  function normalizeMapping(entry: AppMapping): AppMapping {
    const exe = normalizeExe(entry.exe);
    const mapping: AppMapping = {
      exe,
      profile: entry.profile || 'casual',
      name: getAppDisplayName({ exe, name: entry.name }, installedApps),
    };
    if (entry.cleanup_intensity) {
      mapping.cleanup_intensity = entry.cleanup_intensity;
    }
    return mapping;
  }
</script>

{#if showHeading}
  <h2 class="settings-h">App Mappings</h2>
{/if}
<p class="panel-note">{intro}</p>

<AppMappingsList
  {mappings}
  {installedApps}
  {emptyText}
  {leavingExes}
  onDelete={deleteMapping}
  onUpdateField={updateMappingField}
/>

{#if mappingError}
  <div class="mapping-error">{mappingError}</div>
{/if}

<AppMappingsAddForm
  {installedApps}
  {areAppsLoaded}
  {mappedExes}
  {addLabel}
  onAdd={addMapping}
/>

<style>
  .panel-note {
    font-size: 12px;
    color: var(--ink-mute);
    margin: 0 0 16px;
    line-height: 1.5;
  }

  .mapping-error {
    font-size: 12px;
    color: var(--accent);
    padding: 4px 0 12px;
  }
</style>
