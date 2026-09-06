import { afterEach, beforeEach, expect, it, vi } from 'vitest';

const ipc = vi.hoisted(() => ({ invoke: vi.fn(), listen: vi.fn() }));
vi.mock('./tauri', () => ipc);
vi.mock('./stores.svelte', () => ({ fetchSnippets: vi.fn(), fetchDictionary: vi.fn() }));
vi.mock('./contextsStore.svelte', () => ({ loadContexts: vi.fn() }));
import { startSyncListeners, syncStore } from './syncStore.svelte';

let handlers: Map<string, () => void>;
let doc: EventTarget & { hidden: boolean };
let stop: (() => void) | undefined;
beforeEach(() => {
  vi.useFakeTimers();
  doc = Object.assign(new EventTarget(), { hidden: false });
  vi.stubGlobal('document', doc);
  handlers = new Map();
  ipc.invoke.mockReset().mockResolvedValue({ pairing: null });
  ipc.listen.mockReset().mockImplementation(async (event, handler) => {
    handlers.set(event, handler);
    return () => handlers.delete(event);
  });
  syncStore.status = null;
  syncStore.loaded = false;
});
afterEach(() => {
  stop?.();
  stop = undefined;
  vi.useRealTimers();
  vi.unstubAllGlobals();
});

it('reconstructs initial state after listeners register and uses two idle polls per minute', async () => {
  stop = startSyncListeners();
  await vi.advanceTimersByTimeAsync(0);
  expect(handlers.size).toBe(5);
  expect(ipc.invoke).toHaveBeenCalledTimes(1);
  await vi.advanceTimersByTimeAsync(60_000);
  expect(ipc.invoke).toHaveBeenCalledTimes(3);
});

it('reacts to hidden incoming pairing events and retains the missed-event fallback', async () => {
  doc.hidden = true;
  stop = startSyncListeners();
  await vi.advanceTimersByTimeAsync(0);
  expect(ipc.invoke).toHaveBeenCalledTimes(1);
  ipc.invoke.mockResolvedValue({ pairing: { kind: 'incoming', phase: 'awaiting_code' } });
  handlers.get('verenu:sync-pair-request')!();
  await vi.advanceTimersByTimeAsync(0);
  expect(syncStore.status?.pairing?.kind).toBe('incoming');
  expect(ipc.invoke).toHaveBeenCalledTimes(2);
  await vi.advanceTimersByTimeAsync(30_000);
  expect(ipc.invoke).toHaveBeenCalledTimes(3);
  doc.hidden = false;
  doc.dispatchEvent(new Event('visibilitychange'));
  await vi.advanceTimersByTimeAsync(3000);
  expect(ipc.invoke).toHaveBeenCalledTimes(7);
});

it('cleans up late listener registrations without starting requests after disposal', async () => {
  let register!: (unlisten: () => void) => void;
  const unlisten = vi.fn();
  ipc.listen.mockImplementationOnce(() => new Promise(resolve => { register = resolve; }));
  stop = startSyncListeners();
  stop();
  register(unlisten);
  await vi.advanceTimersByTimeAsync(60_000);
  expect(unlisten).toHaveBeenCalledTimes(1);
  expect(ipc.invoke).not.toHaveBeenCalled();
  expect(handlers.size).toBe(0);
  expect(vi.getTimerCount()).toBe(0);
  stop = undefined;
});
