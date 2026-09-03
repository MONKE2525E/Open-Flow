import type { AppearanceMode, CleanupIntensity, ProviderId, ToneId } from '../settings';

type SetupProvider = {
  id: ProviderId;
  name: string;
  badge: string;
  desc: string;
};

// Recommended default first. Local sits last because it is the only option that
// can't dictate until a separate model download finishes.
export const providers: SetupProvider[] = [
  {
    id: 'groq',
    name: 'Groq',
    badge: 'Recommended',
    desc: 'A free key with generous limits, running on LPUs — the fastest option here.',
  },
  {
    id: 'google',
    name: 'Google Gemini',
    badge: '',
    desc: 'Gemini Flash-Lite for fast, low-cost transcription cleanup on a generous free tier.',
  },
  {
    id: 'openai',
    name: 'OpenAI',
    badge: '',
    desc: 'gpt-4o-transcribe and gpt-4o-mini. The most polished cleanup, but it bills per use.',
  },
  {
    id: 'local',
    name: 'On this device',
    badge: 'Beta',
    desc: 'Parakeet V3 runs on your machine and nothing leaves it. Needs a model download first.',
  },
];

export type ProviderGuideStep = { caption: string; alt: string };
export type ProviderGuide = { url: string; steps: ProviderGuideStep[] };

export const providerGuides: Record<ProviderId, ProviderGuide> = {
  local: {
    url: 'No API key needed',
    steps: [
      { caption: 'Choose "On this device" for transcription', alt: 'On this device provider option' },
      { caption: 'Download Parakeet V3 from Settings → Models', alt: 'Local transcription model download' },
      { caption: 'Set Cleanup to Off to keep the transcript local too', alt: 'Cleanup Off setting' },
      { caption: 'You can add a cloud cleanup provider later', alt: 'Cloud cleanup provider setting' },
    ],
  },
  // Captions are the carousel's step text and pair 1:1 with the screenshots in
  // src/assets/setup/<provider>-<n>-*.png — keep the two in step.
  groq: {
    url: 'console.groq.com/keys',
    steps: [
      { caption: 'Sign in to Groq', alt: 'Groq sign-in page with the sign-in action highlighted' },
      { caption: 'Open API Keys and choose "Create API Key"', alt: 'Groq API Keys page with the Create API Key button highlighted' },
      { caption: 'Name the key, then submit', alt: 'Groq key creation dialog with the name field and Submit button highlighted' },
      { caption: 'Copy the key now; Groq shows it once', alt: 'Groq key confirmation dialog with the Copy button highlighted' },
    ],
  },
  openai: {
    url: 'platform.openai.com/api-keys',
    steps: [
      { caption: 'Sign in to the OpenAI platform', alt: 'OpenAI platform sign-in page with the sign-in form highlighted' },
      { caption: 'Choose "Create an API key"', alt: 'OpenAI platform home page with Create an API key highlighted' },
      { caption: 'Name it, then create the secret key', alt: 'OpenAI key creation dialog with the name field and Create secret key button highlighted' },
      { caption: 'Copy the key now; it is shown once', alt: 'OpenAI key confirmation dialog with the Copy button highlighted' },
    ],
  },
  google: {
    url: 'aistudio.google.com/app/apikey',
    steps: [
      { caption: 'Accept the terms and continue', alt: 'Google AI Studio terms page with Continue highlighted' },
      { caption: 'Open API Keys and choose "Create API key"', alt: 'Google AI Studio API Keys page with Create API key highlighted' },
      { caption: 'Name it, then create the key', alt: 'Google AI Studio key creation dialog with the name field and Create key highlighted' },
      { caption: 'Open the key and choose "Copy key"', alt: 'Google AI Studio key details with Copy key highlighted and sensitive fields redacted' },
    ],
  },
  assemblyai: {
    url: 'app.assemblyai.com',
    steps: [
      { caption: 'Go to app.assemblyai.com', alt: 'AssemblyAI home page' },
      { caption: 'Sign in or create a free account', alt: 'AssemblyAI sign-in page' },
      { caption: 'Copy your API key from the dashboard', alt: 'AssemblyAI dashboard API key area' },
      { caption: 'Paste the key into Verenu', alt: 'Verenu API key field' },
    ],
  },
};

type CleanupCard = { id: CleanupIntensity; name: string; desc: string };

export const cleanupCards: CleanupCard[] = [
  { id: 'none', name: 'Off', desc: 'Keep the raw transcript. A second call is used only to reconcile two transcripts.' },
  { id: 'light', name: 'Light', desc: 'Remove speech artifacts and fix basics. Keep wording, order, and structure.' },
  { id: 'medium', name: 'Medium', desc: 'Improve flow and remove redundancy while preserving every distinct detail.' },
  { id: 'high', name: 'Strong', desc: 'Rewrite concisely while preserving facts, constraints, qualifiers, and emphasis.' },
];

type ToneCard = { id: ToneId; name: string; desc: string };

export const toneCards: ToneCard[] = [
  { id: 'casual', name: 'Casual', desc: 'Contractions, normal casing, and the speaker’s casual voice' },
  { id: 'formal', name: 'Formal', desc: 'Professional wording without added greetings or politeness' },
  { id: 'very_casual', name: 'Very Casual', desc: 'Mostly lowercase, minimal readable punctuation, and preserved emphasis' },
];

/** Illustrative-only before→after preview text. Not API-driven — a static approximation. */
const SAMPLE_RAW = 'um so like i think we should um ship it tomorrow maybe';

const cleanupPreview: Record<CleanupCard['id'], string> = {
  none: 'um so like i think we should um ship it tomorrow maybe',
  light: 'I think we should ship it tomorrow maybe',
  medium: 'I think we should ship it tomorrow, maybe.',
  high: 'We should ship it tomorrow.',
};

const tonePreview: Record<ToneCard['id'], (base: string) => string> = {
  casual: (base) => base.charAt(0).toUpperCase() + base.slice(1),
  formal: (base) => {
    const sentence = `${base.charAt(0).toUpperCase()}${base.slice(1)}`;
    return `${sentence.replace(/[.!?]+$/, '')}.`;
  },
  very_casual: (base) => base.toLowerCase().replace(/[.!?]+$/, ''),
};

export function writingStylePreview(intensity: CleanupCard['id'], tone: ToneCard['id']): { before: string; after: string } {
  return { before: SAMPLE_RAW, after: tonePreview[tone](cleanupPreview[intensity]) };
}

/**
 * The wizard no longer asks about appearance — it is not a first-run decision,
 * and Settings → General has a live 3-way picker. Setup writes 'system'.
 */
export const SETUP_APPEARANCE_MODE: AppearanceMode = 'system';
