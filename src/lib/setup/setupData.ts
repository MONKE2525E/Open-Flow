import type { AppearanceMode, CleanupIntensity, ProviderId, ToneId } from '../settings';

export type SetupProvider = {
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
    desc: 'Gemini 3.5 Flash for both transcription and cleanup, on a generous free tier.',
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

export const providerGuides: Record<ProviderId, { url: string; steps: string[] }> = {
  local: {
    url: 'No API key needed',
    steps: [
      'Choose "On this device" for transcription',
      'Download Parakeet V3 from Settings → Models',
      'Set Cleanup to Off if you want the transcript to stay local too',
      'You can still add a cloud cleanup provider later',
    ],
  },
  // Captions are the carousel's step text and pair 1:1 with the screenshots in
  // src/assets/setup/<provider>-<n>-*.png — keep the two in step.
  groq: {
    url: 'console.groq.com/keys',
    steps: [
      'Sign in at console.groq.com — free, no card needed',
      'Open API Keys, then click "Create API Key"',
      'Give it any name and click Submit',
      'Copy the key now — Groq only shows it once',
    ],
  },
  openai: {
    url: 'platform.openai.com/api-keys',
    steps: [
      'Sign in or sign up at platform.openai.com',
      'On the home page, click "Create an API key"',
      'Give it any name and click Create secret key',
      'Copy the key now — OpenAI only shows it once',
    ],
  },
  google: {
    url: 'aistudio.google.com/app/apikey',
    steps: [
      'Sign in at aistudio.google.com and accept the terms',
      'Open API Keys, then click "Create API key"',
      'Give it any name and click Create key',
      'Open the key and click "Copy key"',
    ],
  },
  assemblyai: {
    url: 'app.assemblyai.com',
    steps: ['Go to app.assemblyai.com', 'Sign in or create a free account', 'Copy your API key from the dashboard', 'Paste it below'],
  },
};

export type CleanupCard = { id: CleanupIntensity; name: string; desc: string };

export const cleanupCards: CleanupCard[] = [
  { id: 'none', name: 'Off', desc: 'Raw transcript, no second AI call' },
  { id: 'light', name: 'Light', desc: 'Punctuation and obvious slips only' },
  { id: 'medium', name: 'Medium', desc: 'Fillers and repetition removed' },
  { id: 'high', name: 'Strong', desc: 'Rewritten shorter and tighter' },
];

export type ToneCard = { id: ToneId; name: string; desc: string };

export const toneCards: ToneCard[] = [
  { id: 'casual', name: 'Casual', desc: 'Reads like a quick message' },
  { id: 'formal', name: 'Formal', desc: 'Polished, professional prose' },
  { id: 'very_casual', name: 'Very Casual', desc: 'all lowercase, no punctuation' },
];

/** Illustrative-only before→after preview text. Not API-driven — a static approximation. */
const SAMPLE_RAW = 'um so like i think we should um ship it tomorrow maybe';

const cleanupPreview: Record<CleanupCard['id'], string> = {
  none: 'um so like i think we should um ship it tomorrow maybe',
  light: 'I think we should ship it tomorrow maybe',
  medium: 'I think we should ship it tomorrow',
  high: "Let's ship tomorrow",
};

const tonePreview: Record<ToneCard['id'], (base: string) => string> = {
  casual: (base) => base.charAt(0).toUpperCase() + base.slice(1),
  formal: (base) => `${base.charAt(0).toUpperCase()}${base.slice(1)}.`.replace(/\bmaybe\b/, 'pending final review'),
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
