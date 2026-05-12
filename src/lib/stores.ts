import { writable } from 'svelte/store';

export const currentPage = writable<'home' | 'dictionary' | 'snippets' | 'style'>('home');
export const settingsOpen = writable(false);
export const accentColor = writable<'terracotta' | 'moss' | 'slate' | 'ink'>('terracotta');
export const pillState = writable<'idle' | 'recording' | 'processing' | 'handsfree'>('idle');

// Home page data
export const recentDictations = writable([
  {
    time: '11:21 PM',
    text: "Lay the groundwork for the open-source dictation client; sketched the sidebar and pill states.",
  },
  {
    time: '11:08 PM',
    text: "First milestone: under 100 MB resident memory at idle. Half a gig is wild for a tray utility.",
  },
  {
    time: '10:45 PM',
    text: "API keys instead of subscription. Plug in your own Groq, OpenAI, or Gemini key — we never see audio.",
  },
  {
    time: '9:30 PM',
    text: "Bind hold-to-dictate to right option. Hands-free toggle should live on the pill.",
  },
]);

export const stats = writable({
  totalWords: 15800,
  wpm: 107,
  dayStreak: 7,
});

export const providers = writable([
  {
    name: 'Groq',
    model: 'whisper-large-v3',
    status: '240 ms avg',
    color: 'rgb(74, 67, 58)',
    active: true,
  },
  {
    name: 'OpenAI',
    model: 'whisper-1',
    status: 'fallback · ready',
    color: 'rgb(216, 211, 207)',
    active: false,
  },
]);

// Dictionary
export const dictionaryTerms = writable([
  { term: 'Anthropic', pron: 'an-THROP-ik', kind: 'Proper noun' },
  { term: 'Tauri', pron: 'TOW-ree', kind: 'Proper noun' },
  { term: 'OAuth', pron: 'OH-auth', kind: 'Acronym' },
  { term: 'Kubernetes', pron: 'koo-ber-NET-eez', kind: 'Proper noun' },
  { term: 'Noah Bergeron', pron: '—', kind: 'Name' },
  { term: 'OpenRouter', pron: 'OH-pen-rowt-er', kind: 'Proper noun' },
]);

// Snippets
export const snippets = writable([
  { trigger: '/sig', text: "— Noah Bergeron, Maintainer, Open Flow", uses: 142, when: '2h ago' },
  { trigger: '/addr', text: "1428 Pearl St, Boulder CO 80302", uses: 27, when: 'Yesterday' },
  { trigger: '/wfh', text: "Working from home today — reply may be slower.", uses: 9, when: '3d ago' },
  { trigger: '/standup', text: "Yesterday I — Today I'm — Blockers: none.", uses: 64, when: '1d ago' },
  { trigger: '/repo', text: "github.com/noah/open-flow", uses: 51, when: '4h ago' },
  { trigger: '/ty', text: "Thanks — appreciate it. I'll follow up later.", uses: 88, when: '11h ago' },
]);

// Style
export const currentIntensity = writable('medium');
export const currentTone = writable('casual');
export const styleTab = writable('cleanup');
