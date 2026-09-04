// Official-compatible Tauri IPC mock for Playwright addInitScript.
// Based on @tauri-apps/api/mocks — mirrors the exact __TAURI_INTERNALS__ interface
// that Tauri injects into WebView2. Without this, invoke() throws and the Setup
// overlay shows (setupComplete defaults to false), blocking all UI tests.
//
// Usage: await page.addInitScript(tauriMock, { appVersion: APP_VERSION });

const APP_VERSION = require('../../package.json').version;

function tauriMock({ appVersion } = {}) {
  const APP_VERSION = appVersion || '0.0.0';
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
      openai: ['gpt-4o-mini-transcribe', 'gpt-4o-transcribe', 'whisper-1'],
      google: ['gemini-2.5-flash', 'gemini-3.5-transcribe', 'gemini-2.5-flash-lite', 'gemini-2.5-pro'],
      local: ['parakeet-v3'],
    },
    transcription_fallback_models: [],
    transcription_language:  'en',
    cleanup_provider:        'groq',
    cleanup_model:           'groq/qwen/qwen3.6-27b',
    cleanup_default_model:   'groq/qwen/qwen3.6-27b',
    cleanup_models_by_provider: {
      groq: ['qwen/qwen3.6-27b', 'openai/gpt-oss-20b', 'openai/gpt-oss-120b'],
      openai: ['gpt-4o-mini', 'gpt-4o'],
      google: ['gemini-2.5-flash', 'gemini-2.5-flash-lite', 'gemini-2.5-pro'],
      local: [],
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
    accent_color:            null,
    clipboard_phrase:        null,
    clipboard_phrase_enabled: false,
    legacy_features_enabled: false,
    advanced_model_ui:       true,
    local_model_memory_policy: 'unload_after_5m',
    update_dismissed_version: null,
    update_notified_version: null,
    ...storedMem,
  };
  const recentEntries = Array.from({ length: 105 }, (_, index) => ({
    id: 105 - index,
    clean_text: `Mock history entry ${105 - index}`,
    words: 3,
    created_at: `2026-05-${String(31 - Math.floor(index / 8)).padStart(2, '0')} ${String(8 + (index % 10)).padStart(2, '0')}:00:00`,
  }));

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
    if (cmd === 'plugin:app|version')       return APP_VERSION;
    if (cmd === 'plugin:app|name')          return 'Verenu';
    if (cmd === 'plugin:app|tauri_version') return '2.0.0';

    // Autostart plugin
    if (cmd === 'plugin:autostart|enable'  ||
        cmd === 'plugin:autostart|disable' ||
        cmd === 'plugin:autostart|is_enabled') return false;

    switch (cmd) {
      case 'get_setting':        return mem[args?.key] ?? null;
      case 'save_setting':       mem[args?.key] = args?.value; persistMem(); return null;
      case 'get_all_settings':   return {
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
        auto_learn_enabled:            mem.auto_learn_enabled ?? null,
        contextual_caps_enabled:       mem.contextual_caps_enabled ?? null,
        auto_spacing_enabled:          mem.auto_spacing_enabled ?? null,
        history_retention:             mem.history_retention ?? null,
        mic_gain:                      mem.mic_gain ?? null,
        microphone_device:             mem.microphone_device ?? null,
        hotkey:                        mem.hotkey ?? null,
        appearance_mode:               mem.appearance_mode ?? null,
        accent_color:                  mem.accent_color ?? null,
        clipboard_phrase:              mem.clipboard_phrase ?? null,
        clipboard_phrase_enabled:      mem.clipboard_phrase_enabled ?? null,
        legacy_features_enabled:       mem.legacy_features_enabled ?? null,
        advanced_model_ui:             mem.advanced_model_ui ?? null,
        local_model_memory_policy:     mem.local_model_memory_policy ?? null,
        update_dismissed_version:      mem.update_dismissed_version ?? null,
        update_notified_version:       mem.update_notified_version ?? null,
      };
      case 'get_api_key_status': return { groq: false, openai: false, google: false, local: false };
      case 'list_local_stt_models':
        return [
          {
            id: 'parakeet-v3',
            name: 'Parakeet V3',
            description: 'Fast and accurate. Supports 25 European languages.',
            filename: 'parakeet-v3-int8.tar.gz',
            url: 'https://blob.handy.computer/parakeet-v3-int8.tar.gz',
            sha256: '43d37191602727524a7d8c6da0eef11c4ba24320f5b4730f1a2497befc2efa77',
            size_mb: 456,
            is_directory: true,
            is_downloaded: true,
            is_downloading: false,
            partial_size: 0,
            engine_type: 'parakeet',
            speed_score: 4.25,
            accuracy_score: 4.0,
            privacy_label: 'Runs on this device',
            supported_languages: [
              'Bulgarian', 'Croatian', 'Czech', 'Danish', 'Dutch', 'English', 'Estonian', 'Finnish',
              'French', 'German', 'Greek', 'Hungarian', 'Italian', 'Latvian', 'Lithuanian', 'Maltese',
              'Polish', 'Portuguese', 'Romanian', 'Slovak', 'Slovenian', 'Spanish', 'Swedish',
              'Russian', 'Ukrainian',
            ],
            supports_language_selection: false,
            supports_translation: false,
            is_recommended: true,
          },
          {
            id: 'parakeet-v2',
            name: 'Parakeet V2',
            description: 'English-only alternative to Parakeet V3 with slightly higher English accuracy.',
            filename: 'parakeet-v2-int8.tar.gz',
            url: 'https://blob.handy.computer/parakeet-v2-int8.tar.gz',
            sha256: 'ac9b9429984dd565b25097337a887bb7f0f8ac393573661c651f0e7d31563991',
            size_mb: 451,
            is_directory: true,
            is_downloaded: false,
            is_downloading: false,
            partial_size: 0,
            engine_type: 'parakeet',
            speed_score: 4.25,
            accuracy_score: 4.25,
            privacy_label: 'Runs on this device',
            supported_languages: ['English'],
            supports_language_selection: false,
            supports_translation: false,
            is_recommended: false,
          },
          {
            id: 'moonshine-base',
            name: 'Moonshine Base',
            description: 'Smaller English model for weaker machines and faster local tests.',
            filename: 'moonshine-base.tar.gz',
            url: 'https://blob.handy.computer/moonshine-base.tar.gz',
            sha256: '04bf6ab012cfceebd4ac7cf88c1b31d027bbdd3cd704649b692e2e935236b7e8',
            size_mb: 187,
            is_directory: true,
            is_downloaded: false,
            is_downloading: false,
            partial_size: 0,
            engine_type: 'moonshine',
            speed_score: 4.8,
            accuracy_score: 3.3,
            privacy_label: 'Runs on this device',
            supported_languages: ['English'],
            supports_language_selection: false,
            supports_translation: false,
            is_recommended: false,
          },
          {
            id: 'moonshine-tiny',
            name: 'Moonshine Tiny',
            description: 'Smallest and fastest English model. Best for low-power machines.',
            filename: 'moonshine-tiny-streaming-en.tar.gz',
            url: 'https://blob.handy.computer/moonshine-tiny-streaming-en.tar.gz',
            sha256: '465addcfca9e86117415677dfdc98b21edc53537210333a3ecdb58509a80abaf',
            size_mb: 31,
            is_directory: true,
            is_downloaded: false,
            is_downloading: false,
            partial_size: 0,
            engine_type: 'moonshine_streaming',
            speed_score: 4.75,
            accuracy_score: 2.75,
            privacy_label: 'Runs on this device',
            supported_languages: ['English'],
            supports_language_selection: false,
            supports_translation: false,
            is_recommended: false,
          },
          {
            id: 'moonshine-small',
            name: 'Moonshine Small',
            description: 'Fast English model with a good balance of speed and accuracy.',
            filename: 'moonshine-small-streaming-en.tar.gz',
            url: 'https://blob.handy.computer/moonshine-small-streaming-en.tar.gz',
            sha256: 'dbb3e1c1832bd88a4ac712f7449a136cc2c9a18c5fe33a12ed1b7cb1cfe9cdd5',
            size_mb: 99,
            is_directory: true,
            is_downloaded: false,
            is_downloading: false,
            partial_size: 0,
            engine_type: 'moonshine_streaming',
            speed_score: 4.5,
            accuracy_score: 3.25,
            privacy_label: 'Runs on this device',
            supported_languages: ['English'],
            supports_language_selection: false,
            supports_translation: false,
            is_recommended: false,
          },
          {
            id: 'moonshine-medium',
            name: 'Moonshine Medium',
            description: 'Higher quality English transcription, still fast.',
            filename: 'moonshine-medium-streaming-en.tar.gz',
            url: 'https://blob.handy.computer/moonshine-medium-streaming-en.tar.gz',
            sha256: '07a66f3bff1c77e75a2f637e5a263928a08baae3c29c4c053fc968a9a9373d13',
            size_mb: 192,
            is_directory: true,
            is_downloaded: false,
            is_downloading: false,
            partial_size: 0,
            engine_type: 'moonshine_streaming',
            speed_score: 4.0,
            accuracy_score: 3.75,
            privacy_label: 'Runs on this device',
            supported_languages: ['English'],
            supports_language_selection: false,
            supports_translation: false,
            is_recommended: false,
          },
          {
            id: 'sense-voice',
            name: 'SenseVoice',
            description: 'Very fast multilingual model: Chinese, English, Japanese, Korean, Cantonese.',
            filename: 'sense-voice-int8.tar.gz',
            url: 'https://blob.handy.computer/sense-voice-int8.tar.gz',
            sha256: '171d611fe5d353a50bbb741b6f3ef42559b1565685684e9aa888ef563ba3e8a4',
            size_mb: 152,
            is_directory: true,
            is_downloaded: false,
            is_downloading: false,
            partial_size: 0,
            engine_type: 'sense_voice',
            speed_score: 4.75,
            accuracy_score: 3.25,
            privacy_label: 'Runs on this device',
            supported_languages: ['Chinese', 'English', 'Japanese', 'Korean', 'Cantonese'],
            supports_language_selection: true,
            supports_translation: false,
            is_recommended: false,
          },
          {
            id: 'gigaam-v3',
            name: 'GigaAM v3',
            description: 'Dedicated Russian speech recognition. Fast and accurate.',
            filename: 'giga-am-v3-int8.tar.gz',
            url: 'https://blob.handy.computer/giga-am-v3-int8.tar.gz',
            sha256: 'd872462268430db140b69b72e0fc4b787b194c1dbe51b58de39444d55b6da45b',
            size_mb: 151,
            is_directory: true,
            is_downloaded: false,
            is_downloading: false,
            partial_size: 0,
            engine_type: 'giga_am',
            speed_score: 3.75,
            accuracy_score: 4.25,
            privacy_label: 'Runs on this device',
            supported_languages: ['Russian'],
            supports_language_selection: false,
            supports_translation: false,
            is_recommended: false,
          },
          {
            id: 'canary-180m-flash',
            name: 'Canary 180M Flash',
            description: 'Small, fast multilingual model: English, German, Spanish, French. Supports translation.',
            filename: 'canary-180m-flash.tar.gz',
            url: 'https://blob.handy.computer/canary-180m-flash.tar.gz',
            sha256: '6d9cfca6118b296e196eaedc1c8fa9788305a7b0f1feafdb6dc91932ab6e53f7',
            size_mb: 146,
            is_directory: true,
            is_downloaded: false,
            is_downloading: false,
            partial_size: 0,
            engine_type: 'canary',
            speed_score: 4.25,
            accuracy_score: 3.75,
            privacy_label: 'Runs on this device',
            supported_languages: ['English', 'German', 'Spanish', 'French'],
            supports_language_selection: true,
            supports_translation: true,
            is_recommended: false,
          },
          {
            id: 'canary-1b-v2',
            name: 'Canary 1B v2',
            description: 'Larger, more accurate multilingual model. 25 European languages. Supports translation.',
            filename: 'canary-1b-v2.tar.gz',
            url: 'https://blob.handy.computer/canary-1b-v2.tar.gz',
            sha256: '02305b2a25f9cf3e7deaffa7f94df00efa44f442cd55c101c2cb9c000f904666',
            size_mb: 691,
            is_directory: true,
            is_downloaded: false,
            is_downloading: false,
            partial_size: 0,
            engine_type: 'canary',
            speed_score: 3.5,
            accuracy_score: 4.25,
            privacy_label: 'Runs on this device',
            supported_languages: [
              'Bulgarian', 'Croatian', 'Czech', 'Danish', 'Dutch', 'English', 'Estonian', 'Finnish',
              'French', 'German', 'Greek', 'Hungarian', 'Italian', 'Latvian', 'Lithuanian', 'Maltese',
              'Polish', 'Portuguese', 'Romanian', 'Slovak', 'Slovenian', 'Spanish', 'Swedish',
              'Russian', 'Ukrainian',
            ],
            supports_language_selection: true,
            supports_translation: true,
            is_recommended: false,
          },
          {
            id: 'cohere',
            name: 'Cohere',
            description: 'Largest and most accurate multilingual model. Covers European and East Asian languages, but slower.',
            filename: 'cohere-int8.tar.gz',
            url: 'https://blob.handy.computer/cohere-int8.tar.gz',
            sha256: 'ea2257d52434f3644574f187dcdcf666e302cd11b92866116ab8e14cd9c887f0',
            size_mb: 1708,
            is_directory: true,
            is_downloaded: false,
            is_downloading: false,
            partial_size: 0,
            engine_type: 'cohere',
            speed_score: 3.0,
            accuracy_score: 4.5,
            privacy_label: 'Runs on this device',
            supported_languages: [
              'English', 'French', 'German', 'Italian', 'Spanish', 'Portuguese', 'Greek', 'Dutch',
              'Polish', 'Chinese', 'Japanese', 'Korean', 'Vietnamese', 'Arabic',
            ],
            supports_language_selection: true,
            supports_translation: false,
            is_recommended: false,
          },
        ];
      case 'list_local_llm_models':
        return [
          {
            id: 'gemma-4-e2b',
            name: 'Gemma 4 E2B',
            description: 'Best small default for local cleanup. Strong punctuation and instruction following.',
            repo_id: 'google/gemma-4-E2B-it-qat-q4_0-gguf',
            size_mb: 1640,
            quantization: 'Q4_0',
            privacy_label: 'Runs on this device',
            is_downloaded: false,
            is_downloading: false,
            partial_size: 0,
            is_recommended: true,
            prompt_family: 'gemma4',
          },
          {
            id: 'qwen2.5-3b-instruct',
            name: 'Qwen 2.5 3B Instruct',
            description: 'Balanced local cleanup model with strong formatting control and good latency.',
            repo_id: 'Qwen/Qwen2.5-3B-Instruct-GGUF',
            size_mb: 1960,
            quantization: 'Q4_K_M',
            privacy_label: 'Runs on this device',
            is_downloaded: false,
            is_downloading: false,
            partial_size: 0,
            is_recommended: true,
            prompt_family: 'qwen25',
          },
          {
            id: 'phi-3-mini-4k-instruct',
            name: 'Phi-3 Mini 4K Instruct',
            description: 'Compact Microsoft model with good cleanup reliability and a short context window.',
            repo_id: 'microsoft/Phi-3-mini-4k-instruct-gguf',
            size_mb: 2280,
            quantization: 'Q4',
            privacy_label: 'Runs on this device',
            is_downloaded: false,
            is_downloading: false,
            partial_size: 0,
            is_recommended: true,
            prompt_family: 'phi3',
          },
        ];
      case 'get_local_transcription_state':
        return {
          current_model_id: 'parakeet-v3',
          is_loaded: true,
          is_loading: false,
          is_downloading: false,
          downloading_model_id: null,
        };
      case 'get_local_llm_state':
        return {
          current_model_id: null,
          is_loaded: false,
          is_loading: false,
          is_downloading: false,
          downloading_model_id: null,
          endpoint: null,
        };
      case 'download_local_stt_model':
      case 'download_local_llm_model':
      case 'cancel_local_stt_model_download':
      case 'cancel_local_llm_model_download':
      case 'delete_local_stt_model':
      case 'delete_local_llm_model':
      case 'open_local_stt_models_folder':
      case 'open_local_models_folder':
        return null;
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
      case 'get_recent': {
        const limit = Number(args?.limit ?? 100);
        const offset = Number(args?.offset ?? 0);
        return recentEntries.slice(offset, offset + limit);
      }
      case 'get_stats':          return { total_words: 315, avg_wpm: 152, day_streak: 6 };
      case 'count_old_transcriptions':
        return args?.retention === 'Forever' ? 0 : 3;
      // Contexts live in the sidebar rail now, so the mock has to supply a
      // list for those rows to exist at all.
      case 'get_contexts':       return [
        { id: 1, name: 'Everywhere', is_everywhere: true,  icon: null,   tone: null, cleanup_intensity: null, color: null, custom_instructions: null, pinned_at: null,                  created_at: '2026-05-01 10:00:00', updated_at: '2026-05-01 10:00:00' },
        { id: 2, name: 'Work',       is_everywhere: false, icon: 'chart', tone: null, cleanup_intensity: null, color: null, custom_instructions: null, pinned_at: '2026-05-16 10:00:00', created_at: '2026-05-02 10:00:00', updated_at: '2026-05-02 10:00:00' },
        { id: 3, name: 'Writing',    is_everywhere: false, icon: 'pencil', tone: null, cleanup_intensity: null, color: null, custom_instructions: null, pinned_at: null,                 created_at: '2026-05-03 10:00:00', updated_at: '2026-05-03 10:00:00' },
      ];
      case 'get_context_targets': return [
        { id: 1, context_id: 2, executable: 'code.exe', created_at: '2026-05-02 10:00:00' },
      ];
      case 'get_context_websites': return [
        { id: 1, context_id: 2, domain: 'github.com', created_at: '2026-05-02 10:00:00' },
      ];
      case 'get_context_dictionary':
      case 'get_context_snippets': return [];
      case 'get_context_stats':  return { dictations: 0, words: 0, last_used_at: null };
      case 'get_app_icon':
      case 'get_site_icon':      return null;
      case 'get_dictionary':     return [];
      case 'get_snippets':       return [];
      case 'get_memory_mb':      return 75;   // number required — tweened(0) crashes on null
      case 'check_for_update':   return null;
      case 'get_recent_logs':    return ['[2026-05-17 10:00:00.000] INFO  smoke logger'];
      case 'download_logs':      return 'C:\\Users\\test\\Downloads\\verenu-logs-20260517-100000.txt';
      case 'get_dev_logging_enabled': return false;
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

module.exports = { tauriMock, APP_VERSION };
