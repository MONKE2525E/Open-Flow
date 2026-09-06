import type { ProviderId } from './settings';
import type { TranscriptionLanguageCode } from './transcriptionLanguages';

export type LanguageSupportScope = 'all' | TranscriptionLanguageCode[];

/**
 * Local model id -> exact subset of the 57 dropdown languages it supports.
 * Source of truth for the underlying data is src-tauri/src/local_stt/model.rs
 * (kept in sync by hand, same convention as the other frontend mirrors of
 * that manifest). Deliberately separate from the manifest's own
 * `supported_languages` display strings — that field uses human-readable
 * names and can include languages that aren't dropdown options at all (e.g.
 * Maltese, Cantonese); this map only needs the ones that ARE.
 */
const LOCAL_MODEL_LANGUAGES: Record<string, LanguageSupportScope> = {
  'parakeet-v3': [
    'bg', 'hr', 'cs', 'da', 'nl', 'en', 'et', 'fi', 'fr', 'de', 'el', 'hu', 'it', 'lv', 'lt',
    'pl', 'pt', 'ro', 'sk', 'sl', 'es', 'sv', 'ru', 'uk',
  ], // also supports Maltese, which isn't a dropdown option
  'parakeet-v2': ['en'],
  'moonshine-base': ['en'],
  'moonshine-tiny': ['en'],
  'moonshine-small': ['en'],
  'moonshine-medium': ['en'],
  'sense-voice': ['zh', 'en', 'ja', 'ko'], // also supports Cantonese, which isn't a dropdown option
  'gigaam-v3': ['ru'],
  'canary-180m-flash': ['en', 'de', 'es', 'fr'],
  'canary-1b-v2': [
    'bg', 'hr', 'cs', 'da', 'nl', 'en', 'et', 'fi', 'fr', 'de', 'el', 'hu', 'it', 'lv', 'lt',
    'pl', 'pt', 'ro', 'sk', 'sl', 'es', 'sv', 'ru', 'uk',
  ],
  cohere: ['en', 'fr', 'de', 'it', 'es', 'pt', 'el', 'nl', 'pl', 'zh', 'ja', 'ko', 'vi', 'ar'],
};

/**
 * Cloud providers: 'all' for all three today. Verenu's full 57-language
 * dropdown list is an exact match for OpenAI/Groq's published Whisper
 * language support, and Google publishes no narrower restriction for
 * Gemini's audio transcription — so there's nothing to filter down to for
 * any current cloud model. Modeled per-provider (not hardcoded globally to
 * 'all') so this stays correct if a future cloud model has narrower support.
 */
const CLOUD_PROVIDER_LANGUAGES: Record<'groq' | 'openai' | 'google', LanguageSupportScope> = {
  groq: 'all',
  openai: 'all',
  google: 'all',
};

/**
 * AssemblyAI, unlike the other cloud providers, has a real per-model split:
 * Universal 3.5 Pro natively covers only 6 languages, while Universal-2 covers
 * the same 99+ languages our full dropdown is drawn from.
 */
const ASSEMBLYAI_MODEL_LANGUAGES: Record<string, LanguageSupportScope> = {
  'universal-3-5-pro': ['en', 'es', 'de', 'fr', 'pt', 'it'],
  'universal-2': 'all',
};

export function getLanguageSupport(provider: ProviderId, modelId: string): LanguageSupportScope {
  if (provider === 'local') return LOCAL_MODEL_LANGUAGES[modelId] ?? 'all';
  if (provider === 'assemblyai') return ASSEMBLYAI_MODEL_LANGUAGES[modelId] ?? 'all';
  return CLOUD_PROVIDER_LANGUAGES[provider] ?? 'all';
}
