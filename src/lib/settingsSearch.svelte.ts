import { CATALOG, providerDisplayLabel, taskLabel } from './components/settings/models';
import type { SettingsSectionId } from './settingsSections';

export type SettingsSearchEntry = {
  id: string;
  section: SettingsSectionId;
  label: string;
  description: string;
  target: string;
  fallbackTarget?: string;
  keywords?: string[];
};

export type SettingsSearchRequest = Pick<
  SettingsSearchEntry,
  'section' | 'label' | 'target' | 'fallbackTarget'
> & { nonce: number };

export const settingsSearchNavigation = $state<{ request: SettingsSearchRequest | null }>({
  request: null,
});

let navigationNonce = 0;

export function requestSettingsSearchNavigation(entry: SettingsSearchEntry): void {
  settingsSearchNavigation.request = {
    section: entry.section,
    label: entry.label,
    target: entry.target,
    fallbackTarget: entry.fallbackTarget,
    nonce: ++navigationNonce,
  };
}

export function clearSettingsSearchNavigation(nonce: number): void {
  if (settingsSearchNavigation.request?.nonce === nonce) {
    settingsSearchNavigation.request = null;
  }
}

const BASE_ENTRIES: SettingsSearchEntry[] = [
  { id: 'general-hotkey', section: 'general', label: 'Hotkey', description: 'Hold to record and release to transcribe', target: 'general-hotkey', keywords: ['shortcut', 'keyboard', 'keybind', 'record'] },
  { id: 'general-copy-last', section: 'general', label: 'Copy last dictation', description: 'Copy the previous dictation to the clipboard', target: 'general-copy-last', keywords: ['clipboard', 'shortcut'] },
  { id: 'general-report', section: 'general', label: 'Report a dictation issue', description: 'Open the complaint box after a dictation', target: 'general-report', keywords: ['repair', 'feedback', 'shortcut'] },
  { id: 'general-language', section: 'general', label: 'Spoken language', description: 'Choose the language transcription should expect', target: 'general-language', keywords: ['locale', 'speech', 'transcription'] },
  { id: 'general-microphone', section: 'general', label: 'Microphone device', description: 'Choose the microphone used for dictation', target: 'general-microphone', keywords: ['mic', 'input', 'audio'] },
  { id: 'general-appearance', section: 'general', label: 'Appearance', description: 'Choose the light, dark, or system theme', target: 'general-appearance', keywords: ['theme', 'light mode', 'dark mode', 'system'] },
  { id: 'general-accent', section: 'general', label: 'Accent color', description: 'Change the color used for actions, highlights, and focus rings', target: 'general-accent', keywords: ['colour', 'orange', 'brand', 'palette', 'theme', 'custom color', 'hex'] },
  { id: 'general-startup', section: 'general', label: 'Start on boot', description: 'Launch Verenu when the computer starts', target: 'general-startup', keywords: ['autostart', 'startup', 'launch'] },
  { id: 'general-cleanup', section: 'general', label: 'Cleanup', description: 'Run an LLM cleanup pass after transcription', target: 'general-cleanup', keywords: ['formatting', 'rewrite', 'llm'] },
  { id: 'general-spacing', section: 'general', label: 'Smart spacing and capitalization', description: 'Adjust inserted text using the cursor context', target: 'general-spacing', keywords: ['caps', 'capitalization', 'punctuation'] },
  { id: 'general-caps-lock', section: 'general', label: 'Automatic caps lock detection', description: 'Output dictation in uppercase while Caps Lock is on', target: 'general-caps-lock', keywords: ['uppercase', 'capitalization'] },
  { id: 'general-legacy', section: 'general', label: 'Legacy pages', description: 'Restore App Mappings, Dictionary, and Snippets pages', target: 'general-legacy', keywords: ['dictionary', 'snippets', 'app mappings'] },

  { id: 'keys-groq', section: 'keys', label: 'Groq API key', description: 'Save or remove the key used for Groq models', target: 'api-key-groq', keywords: ['whisper', 'qwen'] },
  { id: 'keys-openai', section: 'keys', label: 'OpenAI API key', description: 'Save or remove the key used for OpenAI models', target: 'api-key-openai', keywords: ['gpt', 'whisper'] },
  { id: 'keys-google', section: 'keys', label: 'Gemini API key', description: 'Save or remove the key used for Gemini models', target: 'api-key-google', keywords: ['google'] },
  { id: 'keys-assemblyai', section: 'keys', label: 'AssemblyAI API key', description: 'Save or remove the key used for AssemblyAI models', target: 'api-key-assemblyai', keywords: ['universal'] },

  { id: 'models-presets', section: 'models', label: 'Model presets', description: 'Apply a recommended model configuration', target: 'model-presets', keywords: ['recommended', 'balanced', 'local', 'cloud'] },
  { id: 'models-advanced', section: 'models', label: 'Advanced models', description: 'Choose specific models, fallbacks, prompts, and downloads', target: 'advanced-models', keywords: ['custom', 'fallback', 'download'] },
  { id: 'models-transcription', section: 'models', label: 'Transcription models', description: 'Choose the model used to turn speech into text', target: 'models-transcription', fallbackTarget: 'advanced-models', keywords: ['speech to text', 'stt', 'voice'] },
  { id: 'models-cleanup', section: 'models', label: 'Cleanup models', description: 'Choose the model used to clean and format text', target: 'models-cleanup', fallbackTarget: 'advanced-models', keywords: ['llm', 'rewrite', 'prompt'] },
  { id: 'models-strategy', section: 'models', label: 'Transcription strategy', description: 'Use one model or compare two models', target: 'models-strategy', fallbackTarget: 'advanced-models', keywords: ['single model', 'dual model', 'fallback'] },
  { id: 'models-memory', section: 'models', label: 'Memory policy', description: 'Control when idle local models unload', target: 'models-memory', keywords: ['ram', 'unload', 'local'] },
  { id: 'models-folder', section: 'models', label: 'Models folder', description: 'Open the folder containing local models', target: 'models-folder', keywords: ['files', 'downloads', 'storage'] },

  { id: 'privacy-context', section: 'privacy', label: 'App context hint', description: 'Share the target app, website, and window title with cleanup', target: 'privacy-context', keywords: ['window', 'website', 'target app'] },
  { id: 'privacy-service-checks', section: 'privacy', label: 'Verenu service checks', description: 'Control background provider and service health requests', target: 'privacy-service-checks', keywords: ['network', 'status', 'api.verenu.com'] },
  { id: 'privacy-learning', section: 'privacy', label: 'On-device learning', description: 'Add confirmed corrections to the dictionary automatically', target: 'privacy-learning', keywords: ['dictionary', 'corrections', 'auto learn'] },
  { id: 'privacy-learning-activity', section: 'privacy', label: 'Auto-learn activity', description: 'Review automatic dictionary learning activity', target: 'privacy-learning-activity', keywords: ['dictionary', 'corrections'] },
  { id: 'privacy-history', section: 'privacy', label: 'Transcription history', description: 'Choose how long past dictations are kept', target: 'privacy-history', keywords: ['retention', 'delete', 'storage'] },
  { id: 'privacy-cache', section: 'privacy', label: 'Cleanup cache', description: 'Manage cached cleanup results', target: 'privacy-cache', keywords: ['storage', 'clear'] },
  { id: 'privacy-export', section: 'privacy', label: 'Export backup', description: 'Save a backup of Verenu data', target: 'privacy-export', keywords: ['backup', 'download', 'data'] },
  { id: 'privacy-import', section: 'privacy', label: 'Import backup', description: 'Restore Verenu data from a backup', target: 'privacy-import', keywords: ['backup', 'restore', 'data'] },

  { id: 'audio-gain', section: 'advanced', label: 'Microphone gain', description: 'Boost the microphone signal before transcription', target: 'audio-gain', keywords: ['mic', 'volume', 'input'] },
  { id: 'audio-calibration', section: 'advanced', label: 'Auto calibration', description: 'Automatically set microphone gain', target: 'audio-calibration', keywords: ['mic', 'gain', 'level'] },
  { id: 'audio-system-mute', section: 'advanced', label: 'Mute system audio', description: 'Mute computer audio while dictating', target: 'audio-system-mute', keywords: ['windows', 'macos', 'sound'] },
  { id: 'audio-exclusive', section: 'advanced', label: 'Exclusive microphone access', description: 'Reserve the microphone for Verenu while dictating', target: 'audio-exclusive', keywords: ['mic', 'input', 'other apps'] },
  { id: 'audio-pause-media', section: 'advanced', label: 'Pause media while dictating', description: 'Pause and resume active media around dictation', target: 'audio-pause-media', keywords: ['music', 'video', 'windows'] },
  { id: 'audio-noise', section: 'advanced', label: 'Noise reduction', description: 'Suppress background noise before transcription', target: 'audio-noise', keywords: ['rnnoise', 'background', 'mic'] },
  { id: 'audio-sounds', section: 'advanced', label: 'Sound effects volume', description: 'Set the volume of dictation chimes', target: 'audio-sounds', keywords: ['chime', 'sound', 'mute'] },

  { id: 'sync-this-device', section: 'sync', label: 'This device', description: 'Rename this device and manage its sync identity', target: 'sync-this-device', keywords: ['device name', 'lan'] },
  { id: 'sync-paired', section: 'sync', label: 'Paired devices', description: 'View, sync, or remove paired devices', target: 'sync-paired', keywords: ['lan', 'remove', 'sync now'] },
  { id: 'sync-nearby', section: 'sync', label: 'Nearby devices', description: 'Find and pair another Verenu device', target: 'sync-nearby', keywords: ['lan', 'pair', 'network'] },

  { id: 'about-version', section: 'about', label: 'Version', description: 'View the installed Verenu version', target: 'about-version', keywords: ['release', 'build'] },
  { id: 'about-license', section: 'about', label: 'License', description: 'View the Verenu software license', target: 'about-license', keywords: ['mit', 'open source'] },
  { id: 'about-website', section: 'about', label: 'Website', description: 'Open the Verenu website', target: 'about-website', keywords: ['link'] },
  { id: 'about-source', section: 'about', label: 'Source', description: 'Open the Verenu source code', target: 'about-source', keywords: ['github', 'repository'] },
  { id: 'about-setup', section: 'about', label: 'Setup', description: 'Run onboarding again', target: 'about-setup', keywords: ['onboarding', 'provider', 'defaults'] },
  { id: 'about-updates', section: 'about', label: 'Updates', description: 'Check for a newer Verenu release', target: 'about-updates', keywords: ['upgrade', 'release'] },
  { id: 'about-beta', section: 'about', label: 'Beta updates', description: 'Receive early prerelease builds from master', target: 'about-beta', keywords: ['preview', 'development'] },

  { id: 'permissions-macos', section: 'permissions', label: 'macOS permissions', description: 'Manage microphone, accessibility, and Keychain access', target: 'permissions', keywords: ['microphone', 'accessibility', 'keychain'] },
  { id: 'apps-mappings', section: 'apps', label: 'App mappings', description: 'Configure per-app cleanup and tone overrides', target: 'apps-mappings', keywords: ['applications', 'tone', 'legacy'] },

  { id: 'developer-sync', section: 'developer', label: 'LAN device sync', description: 'Enable experimental device-to-device sync', target: 'developer-sync', keywords: ['network', 'pairing'] },
  { id: 'developer-setup', section: 'developer', label: 'Force setup on launch', description: 'Show onboarding every time Verenu starts', target: 'developer-setup', keywords: ['onboarding', 'startup'] },
  { id: 'developer-logs', section: 'developer', label: 'Real-time logs', description: 'View the current diagnostic log stream', target: 'developer-logs', keywords: ['debug', 'logging'] },
  { id: 'developer-download-logs', section: 'developer', label: 'Download logs', description: 'Save session logs to the Downloads folder', target: 'developer-download-logs', keywords: ['debug', 'export'] },
  { id: 'developer-status', section: 'developer', label: 'Provider status check', description: 'Fetch and inspect the provider status response', target: 'developer-status', keywords: ['api', 'health'] },
  { id: 'developer-simulations', section: 'developer', label: 'UI simulations', description: 'Preview outage and offline notices', target: 'developer-simulations', keywords: ['test', 'notice'] },
  { id: 'developer-notifications', section: 'developer', label: 'System notification test', description: 'Send and test a native notification', target: 'developer-notifications', keywords: ['toast', 'test'] },
  { id: 'developer-installer', section: 'developer', label: 'Installer test', description: 'Test the installer update flow', target: 'developer-installer', keywords: ['update', 'release'] },
];

const MODEL_ENTRIES: SettingsSearchEntry[] = CATALOG.flatMap((model) =>
  model.tasks.map((task) => ({
    id: `model-${task}-${model.provider}-${model.id}`,
    section: 'models' as const,
    label: model.label,
    description: `${providerDisplayLabel(model.provider)} ${taskLabel(task).toLowerCase()} model`,
    target: `models-${task}`,
    fallbackTarget: 'advanced-models',
    keywords: [model.id, model.provider, providerDisplayLabel(model.provider), task, ...model.tags],
  })),
);

export const SETTINGS_SEARCH_ENTRIES: readonly SettingsSearchEntry[] = [
  ...BASE_ENTRIES,
  ...MODEL_ENTRIES,
];

function normalize(value: string): string {
  return value
    .toLowerCase()
    .replace(/&/g, ' and ')
    .replace(/[^a-z0-9]+/g, ' ')
    .trim()
    .replace(/\s+/g, ' ');
}

function scoreEntry(entry: SettingsSearchEntry, query: string, tokens: string[]): number {
  const label = normalize(entry.label);
  const description = normalize(entry.description);
  const keywords = normalize(entry.keywords?.join(' ') ?? '');
  const haystack = `${label} ${description} ${keywords}`;
  if (!tokens.every((token) => haystack.includes(token))) return -1;

  if (label === query) return 120;
  if (label.startsWith(query)) return 100;
  if (label.includes(query)) return 80;
  if (keywords.includes(query)) return 55;
  return 35;
}

export function searchSettings(
  rawQuery: string,
  visibleSections: readonly SettingsSectionId[],
  limit = 24,
): SettingsSearchEntry[] {
  const query = normalize(rawQuery);
  if (!query) return [];
  const tokens = query.split(' ');
  const visible = new Set(visibleSections);

  return SETTINGS_SEARCH_ENTRIES
    .map((entry) => ({ entry, score: visible.has(entry.section) ? scoreEntry(entry, query, tokens) : -1 }))
    .filter((result) => result.score >= 0)
    .sort((a, b) => b.score - a.score || a.entry.label.localeCompare(b.entry.label))
    .slice(0, limit)
    .map((result) => result.entry);
}
