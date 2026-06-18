import type { AppearanceMode, CleanupIntensity, ProviderId, ToneId } from '../settings';

export type SetupProvider = {
  id: ProviderId;
  name: string;
  tagline: string;
  badge: string;
  desc: string;
};

export const providers: SetupProvider[] = [
  {
    id: 'groq',
    name: 'Groq',
    tagline: 'Free tier · Fastest',
    badge: 'Recommended',
    desc: 'Free API with very generous limits. LPU inference — the fastest option.',
  },
  {
    id: 'openai',
    name: 'OpenAI',
    tagline: 'GPT-4o · High quality',
    badge: '',
    desc: 'Uses gpt-4o-transcribe and gpt-4o-mini. Best cleanup quality.',
  },
  {
    id: 'google',
    name: 'Google Gemini',
    tagline: 'Gemini · Fast & free tier',
    badge: '',
    desc: 'Uses Gemini 3.5 Flash for both transcription and cleanup. Generous free tier.',
  },
];

export const providerGuides: Record<ProviderId, { url: string; steps: string[] }> = {
  groq: {
    url: 'console.groq.com/keys',
    steps: ['Go to console.groq.com/keys', 'Sign in or create a free account', 'Click "Create API Key"', 'Copy and paste it below'],
  },
  openai: {
    url: 'platform.openai.com/api-keys',
    steps: ['Go to platform.openai.com/api-keys', 'Sign in to your OpenAI account', 'Click "Create new secret key"', 'Copy and paste it below'],
  },
  google: {
    url: 'aistudio.google.com/app/apikey',
    steps: ['Go to aistudio.google.com', 'Sign in with your Google account', 'Click "Get API key" → "Create API key"', 'Copy and paste it below'],
  },
};

export type CleanupCard = { id: CleanupIntensity; name: string; desc: string };

export const cleanupCards: CleanupCard[] = [
  { id: 'none', name: 'Verbatim', desc: 'No cleanup at all' },
  { id: 'light', name: 'Light', desc: 'Light touch-ups only' },
  { id: 'medium', name: 'Medium', desc: 'Fillers and repetition removed' },
  { id: 'high', name: 'Direct', desc: 'Short and punchy' },
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

export type AppearanceCard = { id: AppearanceMode; name: string; desc: string };

export const appearanceModes: AppearanceCard[] = [
  { id: 'system', name: 'System', desc: 'Match your system theme automatically.' },
  { id: 'dark', name: 'Dark', desc: 'Lower glare for night work and dark desktops.' },
  { id: 'light', name: 'Light', desc: 'Brighter surfaces with higher daylight contrast.' },
];
