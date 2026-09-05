export type SortKey = 'newest' | 'oldest' | 'alpha' | 'most_used';
export type CreatedRecordMeta = { id: number; created_at: string };

export const TRIGGER_LIMIT = 300;
// Cap auto-grow so a long paste can't blow out the modal layout; the textarea
// scrolls internally past this height.
export const FIELD_GROW_MAX = 220;

export const sortLabels: { key: SortKey; label: string }[] = [
  { key: 'newest', label: 'Newest' },
  { key: 'oldest', label: 'Oldest' },
  { key: 'alpha', label: 'A → Z' },
  { key: 'most_used', label: 'Most used' },
];

export function fmtDate(iso: string): string {
  try {
    const MS_PER_DAY = 86_400_000;
    const d = new Date(/[Z+]/.test(iso) ? iso : iso + 'Z');
    const diffDays = Math.floor((Date.now() - d.getTime()) / MS_PER_DAY);
    if (diffDays === 0) return 'Today';
    if (diffDays === 1) return 'Yesterday';
    if (diffDays < 7) return `${diffDays}d ago`;
    return d.toLocaleDateString([], { month: 'short', day: 'numeric' });
  } catch {
    return iso.slice(0, 10);
  }
}

export const countCodePoints = (value: string): number => [...value].length;

export function normalizeText(value: string): string {
  return value.replace(/\r\n/g, '\n').replace(/\r/g, '\n').trim();
}

export function requireCreatedRecordMeta(value: unknown): CreatedRecordMeta {
  if (typeof value !== 'object' || value === null) {
    throw new Error('Snippet save returned no record metadata. Relaunch the Tauri app and try again.');
  }
  const meta = value as Partial<CreatedRecordMeta>;
  if (typeof meta.id !== 'number' || !Number.isFinite(meta.id) || typeof meta.created_at !== 'string' || !meta.created_at.trim()) {
    throw new Error('Snippet save returned invalid record metadata. Check the app logs before retrying.');
  }
  return { id: meta.id, created_at: meta.created_at };
}

export function autoGrow(el: HTMLTextAreaElement | null) {
  if (!el) return;
  el.style.height = 'auto';
  const borderDiff = el.offsetHeight - el.clientHeight;
  el.style.height = Math.min(el.scrollHeight + borderDiff, FIELD_GROW_MAX) + 'px';
}
