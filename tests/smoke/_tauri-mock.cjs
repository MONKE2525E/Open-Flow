// Official-compatible Tauri IPC mock for Playwright addInitScript.
// Based on @tauri-apps/api/mocks — mirrors the exact __TAURI_INTERNALS__ interface
// that Tauri injects into WebView2. Without this, invoke() throws and the Setup
// overlay shows (setupComplete defaults to false), blocking all UI tests.
//
// Usage: await page.addInitScript(tauriMock);

function tauriMock() {
  // ── Bootstrap Tauri globals (mirrors mockInternals() in @tauri-apps/api/mocks) ──
  window.__TAURI_INTERNALS__ = window.__TAURI_INTERNALS__ ?? {};
  window.__TAURI_EVENT_PLUGIN_INTERNALS__ = window.__TAURI_EVENT_PLUGIN_INTERNALS__ ?? {};

  // Required by getCurrentWindow() in @tauri-apps/api/window
  window.__TAURI_INTERNALS__.metadata = {
    currentWindow:  { label: 'main' },
    currentWebview: { windowLabel: 'main', label: 'main' },
  };

  // ── In-memory store (save_setting writes here, get_setting reads here) ─────
  let storedMem = {};
  try {
    storedMem = JSON.parse(window.localStorage.getItem('__open_flow_tauri_mock_settings') || '{}');
  } catch {}

  const mem = {
    setup_complete:          true,
    force_setup_on_launch:   false,
    transcription_provider:  'groq',
    transcription_model:     'groq/whisper-large-v3-turbo',
    transcription_default_model: 'groq/whisper-large-v3-turbo',
    transcription_models_by_provider: {
      groq: ['whisper-large-v3-turbo', 'whisper-large-v3'],
      openai: ['gpt-4o-mini-transcribe', 'gpt-4o-transcribe'],
      google: ['gemini-2.5-flash', 'gemini-3.5-flash'],
    },
    transcription_fallback_models: [],
    transcription_language:  'en',
    cleanup_provider:        'groq',
    cleanup_model:           'groq/llama-3.3-70b-versatile',
    cleanup_default_model:   'groq/llama-3.3-70b-versatile',
    cleanup_models_by_provider: {
      groq: ['llama-3.3-70b-versatile', 'llama-3.1-8b-instant'],
      openai: ['gpt-4o-mini', 'gpt-4o'],
      google: ['gemini-2.5-flash', 'gemini-3.5-flash'],
    },
    cleanup_fallback_models: [],
    cleanup_enabled:         true,
    noise_reduction:         true,
    mute_audio:              false,
    autostart_enabled:       false,
    hotkey:                  ['ControlLeft', 'MetaLeft'],
    app_context_hint:        false,
    api_fallback_enabled:    false,
    auto_learn_enabled:      false,
    contextual_caps_enabled: true,
    auto_spacing_enabled:    true,
    default_tone:            'casual',
    cleanup_intensity:       'medium',
    history_retention:       '30 days',
    mic_gain:                3.5,
    microphone_device:       '',
    appearance_mode:         'system',
    advanced_model_ui:       true,
    update_dismissed_version: null,
    ...storedMem,
  };

  function persistMem() {
    try {
      window.localStorage.setItem('__open_flow_tauri_mock_settings', JSON.stringify(mem));
    } catch {}
  }

  // ── Callback registry (mirrors official mock — uses crypto so ID is a Number) ─
  const callbacks = new Map();
  const listeners = new Map();

  function registerCallback(callback, once = false) {
    const id = window.crypto.getRandomValues(new Uint32Array(1))[0];
    callbacks.set(id, (data) => {
      if (once) callbacks.delete(id);
      return callback && callback(data);
    });
    return id;
  }

  function unregisterCallback(id) { callbacks.delete(id); }
  function runCallback(id, data)  { callbacks.get(id)?.(data); }

  // ── Event plugin (plugin:event|*) ─────────────────────────────────────────
  function handleListen(args) {
    if (!listeners.has(args.event)) listeners.set(args.event, []);
    listeners.get(args.event).push(args.handler);
    return args.handler;
  }
  function handleUnlisten(args) {
    const evs = listeners.get(args.event);
    if (evs) {
      const i = evs.indexOf(args.eventId);
      if (i !== -1) evs.splice(i, 1);
    }
  }

  // ── Core invoke handler ────────────────────────────────────────────────────
  async function invoke(cmd, args) {
    // Event plugin (required for listen/unlisten in @tauri-apps/api/event)
    if (cmd === 'plugin:event|listen')   return handleListen(args);
    if (cmd === 'plugin:event|unlisten') { handleUnlisten(args); return null; }
    if (cmd === 'plugin:event|emit')     return null;

    // App plugin (getVersion / getName called by Settings and Home)
    if (cmd === 'plugin:app|version')       return '0.5.1';
    if (cmd === 'plugin:app|name')          return 'Open Flow';
    if (cmd === 'plugin:app|tauri_version') return '2.0.0';

    // Autostart plugin
    if (cmd === 'plugin:autostart|enable'  ||
        cmd === 'plugin:autostart|disable' ||
        cmd === 'plugin:autostart|is_enabled') return false;

    switch (cmd) {
      case 'get_setting':        return mem[args?.key] ?? null;
      case 'save_setting':       mem[args?.key] = args?.value; persistMem(); return null;
      case 'get_all_settings':   return {
        setup_complete:                mem.setup_complete ?? null,
        force_setup_on_launch:         mem.force_setup_on_launch ?? null,
        transcription_provider:        mem.transcription_provider ?? null,
        cleanup_provider:              mem.cleanup_provider ?? null,
        transcription_model:           mem.transcription_model ?? null,
        cleanup_model:                 mem.cleanup_model ?? null,
        transcription_default_model:   mem.transcription_default_model ?? null,
        cleanup_default_model:         mem.cleanup_default_model ?? null,
        transcription_models_by_provider: mem.transcription_models_by_provider ?? null,
        cleanup_models_by_provider:    mem.cleanup_models_by_provider ?? null,
        transcription_fallback_models: mem.transcription_fallback_models ?? null,
        cleanup_fallback_models:       mem.cleanup_fallback_models ?? null,
        transcription_language:        mem.transcription_language ?? null,
        cleanup_enabled:               mem.cleanup_enabled ?? null,
        noise_reduction:               mem.noise_reduction ?? null,
        mute_audio:                    mem.mute_audio ?? null,
        autostart_enabled:             mem.autostart_enabled ?? null,
        app_context_hint:              mem.app_context_hint ?? null,
        api_fallback_enabled:          mem.api_fallback_enabled ?? null,
        auto_learn_enabled:            mem.auto_learn_enabled ?? null,
        contextual_caps_enabled:       mem.contextual_caps_enabled ?? null,
        auto_spacing_enabled:          mem.auto_spacing_enabled ?? null,
        default_tone:                  mem.default_tone ?? null,
        cleanup_intensity:             mem.cleanup_intensity ?? null,
        history_retention:             mem.history_retention ?? null,
        mic_gain:                      mem.mic_gain ?? null,
        microphone_device:             mem.microphone_device ?? null,
        hotkey:                        mem.hotkey ?? null,
        appearance_mode:               mem.appearance_mode ?? null,
        advanced_model_ui:             mem.advanced_model_ui ?? null,
        update_dismissed_version:      mem.update_dismissed_version ?? null,
      };
      case 'get_api_key_status': return { groq: false, openai: false, google: false };
      case 'check_api_key_set':  return false;
      case 'get_microphones':    return [];
      case 'get_installed_apps': return [
        { name: 'Google Chrome',       exe: 'chrome.exe'  },
        { name: 'Slack',               exe: 'slack.exe'   },
        { name: 'Visual Studio Code',  exe: 'code.exe'    },
        { name: 'Notion',              exe: 'notion.exe'  },
      ];
      case 'get_app_mappings':   return mem._app_mappings ?? [];
      case 'save_app_mappings':  mem._app_mappings = args?.mappings ?? []; persistMem(); return null;
      case 'get_recent':         return [];
      case 'get_stats':          return { total_words: 0, avg_wpm: 0, day_streak: 0 };
      case 'get_dictionary':     return [];
      case 'get_snippets':       return [];
      case 'get_memory_mb':      return 75;   // number required — tweened(0) crashes on null
      case 'check_for_update':   return null;
      case 'get_recent_logs':    return ['[2026-05-17 10:00:00.000] INFO  smoke logger'];
      case 'download_logs':      return 'C:\\Users\\test\\Downloads\\open-flow-logs-20260517-100000.txt';
      case 'set_dev_logging_enabled': return null;
      default:                   return null;
    }
  }

  // ── Wire up __TAURI_INTERNALS__ (matches official mock shape) ─────────────
  window.__TAURI_INTERNALS__.invoke             = invoke;
  window.__TAURI_INTERNALS__.transformCallback  = registerCallback;
  window.__TAURI_INTERNALS__.unregisterCallback = unregisterCallback;
  window.__TAURI_INTERNALS__.runCallback        = runCallback;
  window.__TAURI_INTERNALS__.callbacks          = callbacks;

  window.__TAURI_EVENT_PLUGIN_INTERNALS__.unregisterListener = (_event, id) => {
    unregisterCallback(id);
  };
}

module.exports = { tauriMock };
