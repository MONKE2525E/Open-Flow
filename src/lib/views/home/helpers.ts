import type { UpdateInfo } from '../../stores';

export interface Entry {
  id: number;
  clean_text: string;
  words: number;
  created_at: string;
  app_name?: string | null;
  duration_ms?: number | null;
}
export interface Stats { total_words: number; avg_wpm: number; day_streak: number; }

export type RenderItem =
  | { type: 'header'; key: string; label: string }
  | { type: 'row'; key: string; entry: Entry };

export const HISTORY_PAGE_SIZE = 100;

export function formatAppLabel(value: string): string {
  return value
    .replace(/\.exe$/i, '')
    .replace(/[-_]+/g, ' ')
    .replace(/\b\w/g, (letter) => letter.toUpperCase());
}

export function getGreeting(): string {
  const now = new Date();
  const h = now.getHours();
  // Days since epoch — avoids the obvious 7-day weekday cycle
  const seed = Math.floor(now.getTime() / 86_400_000);

  const pick = (msgs: string[]) => msgs[seed % msgs.length];

  if (h >= 5 && h < 12) {
    return pick([
      'Good morning.',
      'Morning — ready to roll.',
      'Early start. Let\'s get into it.',
      'Morning. Coffee first, then dictation.',
      'Rise and grind.',
      'Big day ahead?',
      'Morning. Let\'s make it count.',
      'Up and at it.',
      'Another day, another wall of text.',
      'Morning. What\'s on the agenda?',
      'Fresh start. Let\'s go.',
      'Good morning. The day\'s yours.',
    ]);
  } else if (h >= 12 && h < 17) {
    return pick([
      'Good afternoon.',
      'Afternoon. Keep the momentum.',
      'Halfway through — still going.',
      'Afternoon grind. Let\'s go.',
      'Still going strong?',
      'Post-lunch slump? Push through.',
      'Afternoon. Knock out the list.',
      'How\'s the day treating you?',
      'Deep work hour. Let\'s make it count.',
      'Head down, get it done.',
      'Afternoon. The finish line\'s in sight.',
      'Good afternoon. Lot left to do?',
    ]);
  } else if (h >= 17 && h < 21) {
    return pick([
      'Good evening.',
      'Wrapping things up?',
      'Almost done for the day.',
      'Evening — one last push.',
      'How\'d the day go?',
      'Winding down? Get those last thoughts out.',
      'Evening mode.',
      'End of day. Finish strong.',
      'Evening. You made it.',
      'Tying up loose ends?',
      'Good evening. Almost there.',
      'Last stretch of the day.',
    ]);
  } else {
    return pick([
      'Working late?',
      'Burning the midnight oil.',
      'Still at it. Respect.',
      'Late night session.',
      'Night owl mode.',
      'The quiet hours hit different.',
      'Up late. You\'ve got this.',
      'Late night. Make it count.',
      'Can\'t sleep, or just in the zone?',
      'Night shift. Let\'s go.',
      'Everyone else is asleep. Your move.',
      'Late night grind. Respect.',
    ]);
  }
}

function parseTimestamp(value: string): Date {
  if (value.includes('T')) {
    return new Date(value.endsWith('Z') ? value : `${value}Z`);
  }
  return new Date(value.replace(' ', 'T') + 'Z');
}

export function localDayKey(iso: string): string {
  const d = parseTimestamp(iso);
  if (Number.isNaN(d.getTime())) return iso.slice(0, 10);
  const year = d.getFullYear();
  const month = String(d.getMonth() + 1).padStart(2, '0');
  const day = String(d.getDate()).padStart(2, '0');
  return `${year}-${month}-${day}`;
}

export function fmtTime(iso: string) {
  try {
    return parseTimestamp(iso).toLocaleTimeString([], { hour: 'numeric', minute: '2-digit' });
  } catch { return iso; }
}

export function fmtDate(iso: string) {
  try {
    const d = parseTimestamp(iso);
    const today = new Date();
    const yesterday = new Date(today); yesterday.setDate(today.getDate() - 1);
    if (d.toDateString() === today.toDateString()) return 'Today';
    if (d.toDateString() === yesterday.toDateString()) return 'Yesterday';
    return d.toLocaleDateString([], { weekday: 'short', month: 'short', day: 'numeric' });
  } catch { return iso.slice(0, 10); }
}

function downloadActionLabel(update: UpdateInfo): string {
  return update.assetName.toLowerCase().endsWith('.dmg')
    ? 'Download DMG'
    : 'Download Installer';
}

export function installActionLabel(update: UpdateInfo): string {
  return update.installMode === 'download' ? downloadActionLabel(update) : 'Install & Restart';
}
