import { invoke } from './tauri';
import { classifyIpcError } from './errors';
import type { Context, ContextTarget, ContextWebsiteTarget } from './stores';

export const EVERYWHERE_ID = 1;

/**
 * Contexts moved into the app sidebar, so the list, its app/site targets and
 * the current selection are now shared between the rail and the Contexts
 * management page instead of being owned by the page. Both read this store;
 * the page still owns the create/edit modal itself, which the rail reaches
 * through `modalRequest`.
 */
export const contextsStore = $state({
  contexts: [] as Context[],
  targets: [] as ContextTarget[],
  websites: [] as ContextWebsiteTarget[],
  selectedId: EVERYWHERE_ID,
  /**
   * +1 when the newly selected context sits below the previous one in the
   * rail, -1 when above. The management view swaps along the same axis the
   * sidebar list runs on, so the content appears to come from the row you
   * clicked rather than from nowhere.
   */
  selectDir: 1 as 1 | -1,
  loaded: false,
  error: '',
  /**
   * Set by the sidebar to ask the Contexts page to open its existing
   * create/edit modal. The page clears it once consumed — keeping the modal
   * where it already lives avoids duplicating the whole context form.
   */
  modalRequest: null as null | { mode: 'create' } | { mode: 'edit'; id: number },
});

/**
 * Pins newest-first, then everything else in creation order (which is the order
 * the backend already returns). Everywhere is an ordinary row here — it sorts
 * first among the unpinned only because it is the oldest.
 */
export function orderedContexts(list: Context[]): Context[] {
  const pinned = list
    .filter((context) => context.pinned_at)
    .sort((a, b) => (b.pinned_at ?? '').localeCompare(a.pinned_at ?? '') || b.id - a.id);
  const rest = list.filter((context) => !context.pinned_at);
  return [...pinned, ...rest];
}

/**
 * Selects a context and records which way the selection travelled. Direction
 * has to be resolved here rather than in an effect on the page: the transition
 * reads it while the swap is being created, which is before any effect runs.
 */
export function selectContext(id: number): void {
  if (id === contextsStore.selectedId) return;
  const order = orderedContexts(contextsStore.contexts);
  const from = order.findIndex((context) => context.id === contextsStore.selectedId);
  const to = order.findIndex((context) => context.id === id);
  contextsStore.selectDir = from === -1 || to === -1 || to >= from ? 1 : -1;
  contextsStore.selectedId = id;
}

export function isPinned(context: Context): boolean {
  return Boolean(context.pinned_at);
}

export function selectedContext(): Context | null {
  return (
    contextsStore.contexts.find((context) => context.id === contextsStore.selectedId) ??
    contextsStore.contexts[0] ??
    null
  );
}

let inFlight: Promise<void> | null = null;

/** Loads (or reloads) contexts and their targets. Concurrent calls share one round-trip. */
export function loadContexts(force = false): Promise<void> {
  if (inFlight) return inFlight;
  if (contextsStore.loaded && !force) return Promise.resolve();
  inFlight = (async () => {
    try {
      const [contexts, targets, websites] = await Promise.all([
        invoke<Context[]>('get_contexts'),
        invoke<ContextTarget[]>('get_context_targets', { contextId: null }),
        invoke<ContextWebsiteTarget[]>('get_context_websites', { contextId: null }),
      ]);
      contextsStore.contexts = contexts ?? [];
      contextsStore.targets = targets ?? [];
      contextsStore.websites = websites ?? [];
      contextsStore.error = '';
      contextsStore.loaded = true;
      if (!contextsStore.contexts.some((context) => context.id === contextsStore.selectedId)) {
        contextsStore.selectedId =
          contextsStore.contexts.find((context) => context.is_everywhere)?.id ??
          contextsStore.contexts[0]?.id ??
          EVERYWHERE_ID;
      }
    } catch (error) {
      contextsStore.error = classifyIpcError(error).message;
    } finally {
      inFlight = null;
    }
  })();
  return inFlight;
}

/**
 * Compact creation age for the sidebar rows: `8s`, `3m`, `5h`, `6d`, `3w`,
 * `5mo`, `1y`. SQLite hands back `YYYY-MM-DD HH:MM:SS` in UTC with no zone
 * marker, so normalize before parsing.
 */
export function compactAge(iso: string): string {
  // Only the time portion can carry a zone marker (`Z`, `+hh:mm`, `-hh:mm`);
  // testing the whole string would misread the `-` in the date part.
  const parsed = Date.parse(/(?:[Z+]|-\d{2}:?\d{2})$/.test(iso.slice(10)) ? iso : `${iso.replace(' ', 'T')}Z`);
  if (Number.isNaN(parsed)) return '';
  const seconds = Math.max(0, Math.floor((Date.now() - parsed) / 1000));
  if (seconds < 60) return `${seconds}s`;
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes}m`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours}h`;
  const days = Math.floor(hours / 24);
  if (days < 7) return `${days}d`;
  const weeks = Math.floor(days / 7);
  if (days < 30) return `${weeks}w`;
  const months = Math.floor(days / 30);
  if (months < 12) return `${months}mo`;
  return `${Math.floor(days / 365)}y`;
}
