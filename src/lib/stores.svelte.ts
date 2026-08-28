import { invoke } from './tauri';
import { extractIpcErrorMessage } from './errors';
import type { ProviderId } from './settings';
import type { SettingsSectionId } from './settingsSections';

type PageId = 'home' | 'insights' | 'contexts' | 'dictionary' | 'snippets' | 'style';
export type AppearanceMode = 'system' | 'light' | 'dark';
export type PillState = 'idle' | 'recording' | 'processing' | 'handsfree';
type FetchStatus = 'idle' | 'loading' | 'loaded' | 'error';

export interface Snippet {
  id: number;
  trigger: string;
  expansion: string;
  instructions: string;
  use_count: number;
  created_at: string;
}

export interface DictionaryEntry {
  id: number;
  term: string;
  mistake: string | null;
  auto_learned: boolean;
  correction_count: number;
  confidence_tier?: 'manual' | 'low' | 'medium' | 'high';
  last_seen_at?: string | null;
  created_at: string;
}

export interface Context {
  id: number;
  name: string;
  is_everywhere: boolean;
  icon: string | null;
  tone: string | null;
  cleanup_intensity: string | null;
  color: string | null;
  custom_instructions: string | null;
  contextual_formatting_disabled: boolean;
  /** ISO timestamp of when the user pinned this context, or null if unpinned. */
  pinned_at: string | null;
  created_at: string;
  updated_at: string;
}

export interface ContextTarget {
  id: number;
  context_id: number;
  executable: string;
  created_at: string;
}

export interface ContextWebsiteTarget {
  id: number;
  context_id: number;
  domain: string;
  created_at: string;
}

export interface UpdateInfo {
  version: string;
  downloadUrl: string;
  assetName: string;
  installMode: 'install' | 'download';
}

export interface ProviderStatusAlert {
  providerId: ProviderId;
  providerName: string;
  status: string;
  severity: string;
  message: string;
  detailsUrl: string;
}

export interface GlobalMessage {
  message: string;
  showToUsers: boolean;
  visibleUntil?: number | null;
}

export const appStore = $state({
  currentPage: 'home' as PageId,
  settingsOpen: false,
  // The settings rail lives in the Sidebar but the panel lives in Settings, so
  // the active section and its swap direction have to be shared state.
  settingsSection: 'general' as SettingsSectionId,
  settingsAnimDir: 1 as 1 | -1,
  appVersion: '',
  devModeEnabled: false,
  appearanceMode: 'system' as AppearanceMode,
  // Mirrors the `cleanup_enabled` setting. Shared here (rather than owned
  // privately by GeneralSection) so Style.svelte and the App Mappings
  // settings page can react live to the toggle without their own
  // fetch/listener plumbing — both are inert once cleanup is off, since
  // profile/tone/per-app cleanup-intensity overrides only ever feed the
  // cleanup LLM call that this setting skips entirely.
  cleanupEnabled: true,
  // Mirrors `legacy_features_enabled`. Gates App Mappings in Settings and the
  // Dictionary/Snippets pages in the sidebar nav — both superseded by Contexts,
  // kept reachable for anyone still relying on the old per-page workflow.
  legacyFeaturesEnabled: false,
  syncEnabled: false,
  pillState: 'idle' as PillState,
  setupComplete: null as boolean | null,
  snippets: [] as Snippet[],
  snippetsFetchStatus: 'idle' as FetchStatus,
  snippetsFetchError: '',
  dictionary: [] as DictionaryEntry[],
  dictionaryFetchStatus: 'idle' as FetchStatus,
  dictionaryFetchError: '',
  updateInfo: null as UpdateInfo | null,
  betaUpdatesEnabled: false,
  providerStatusAlerts: [] as ProviderStatusAlert[],
  providerStatusSimulation: false,
  globalMessage: null as GlobalMessage | null,
  globalMessageSimulation: false,
  apiHealthy: null as boolean | null,
  isOnline: true,
});

let snippetsFetchToken = 0;
let dictionaryFetchToken = 0;

export function cancelSnippetsFetch() {
  snippetsFetchToken++;
  if (appStore.snippetsFetchStatus === 'loading') appStore.snippetsFetchStatus = 'loaded';
}
export function cancelDictionaryFetch() {
  dictionaryFetchToken++;
  if (appStore.dictionaryFetchStatus === 'loading') appStore.dictionaryFetchStatus = 'loaded';
}

export function formatIpcError(err: unknown): string {
  return extractIpcErrorMessage(err);
}

export async function fetchSnippets(): Promise<void> {
  const token = ++snippetsFetchToken;
  appStore.snippetsFetchStatus = 'loading';
  appStore.snippetsFetchError = '';
  try {
    const data = await invoke<Snippet[]>('get_snippets');
    if (token !== snippetsFetchToken) return;
    appStore.snippets = data ?? [];
    appStore.snippetsFetchStatus = 'loaded';
  } catch (err) {
    if (token !== snippetsFetchToken) return;
    console.error('IPC fetchSnippets failed:', err);
    appStore.snippetsFetchStatus = 'error';
    appStore.snippetsFetchError = formatIpcError(err);
  }
}

/**
 * The one cleanup prompt, used by every model. It was once keyed per
 * provider/model, which quietly lost the edit the moment a fallback took over
 * — you tuned the prompt on your default and the fallback ran the stock one.
 */
export const cleanupPromptStore = $state<{ override: string }>({ override: '' });

export const cleanupPromptEditor = $state<{
  open: boolean;
  /** The model the editor tests the prompt against — not what it saves under. */
  provider: ProviderId | null;
  model: string | null;
  origin: { x: number; y: number } | null;
}>({
  open: false,
  provider: null,
  model: null,
  origin: null,
});

export function openCleanupPromptEditor(
  provider: ProviderId,
  model: string,
  triggerRect: DOMRect
) {
  cleanupPromptEditor.provider = provider;
  cleanupPromptEditor.model = model;
  cleanupPromptEditor.origin = {
    x: triggerRect.left + triggerRect.width / 2,
    y: triggerRect.top + triggerRect.height / 2,
  };
  cleanupPromptEditor.open = true;
}

export function closeCleanupPromptEditor() {
  cleanupPromptEditor.open = false;
}

export async function fetchDictionary(): Promise<void> {
  const token = ++dictionaryFetchToken;
  appStore.dictionaryFetchStatus = 'loading';
  appStore.dictionaryFetchError = '';
  try {
    const data = await invoke<DictionaryEntry[]>('get_dictionary');
    if (token !== dictionaryFetchToken) return;
    appStore.dictionary = data ?? [];
    appStore.dictionaryFetchStatus = 'loaded';
  } catch (err) {
    if (token !== dictionaryFetchToken) return;
    console.error('IPC fetchDictionary failed:', err);
    appStore.dictionaryFetchStatus = 'error';
    appStore.dictionaryFetchError = formatIpcError(err);
  }
}
