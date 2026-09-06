import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { startPolling } from './polling';

describe('background polling', () => {
  let doc: EventTarget & { hidden: boolean };
  beforeEach(() => {
    vi.useFakeTimers();
    doc = Object.assign(new EventTarget(), { hidden: false });
    vi.stubGlobal('document', doc);
  });
  afterEach(() => { vi.useRealTimers(); vi.unstubAllGlobals(); });
  const flush = () => vi.advanceTimersByTimeAsync(0);
  function hide(hidden: boolean) {
    doc.hidden = hidden;
    doc.dispatchEvent(new Event('visibilitychange'));
  }

  it('suspends hidden diagnostics and immediately reconciles on return', async () => {
    const refresh = vi.fn(async () => {});
    const poll = startPolling(refresh, 5000);
    await flush();
    await vi.advanceTimersByTimeAsync(10_000);
    expect(refresh).toHaveBeenCalledTimes(3);
    hide(true);
    await vi.advanceTimersByTimeAsync(60_000);
    expect(refresh).toHaveBeenCalledTimes(3);
    hide(false);
    await flush();
    expect(refresh).toHaveBeenCalledTimes(4);
    poll.stop();
    expect(vi.getTimerCount()).toBe(0);
  });

  it('keeps a slow hidden fallback and accelerates active pairing', async () => {
    let pairing = false;
    const refresh = vi.fn(async () => {});
    const poll = startPolling(refresh, () => pairing ? 1000 : 30_000,
      { hiddenIntervalMs: 30_000 });
    await flush();
    await vi.advanceTimersByTimeAsync(60_000);
    expect(refresh).toHaveBeenCalledTimes(3);
    pairing = true;
    poll.request();
    await flush();
    await vi.advanceTimersByTimeAsync(3000);
    expect(refresh).toHaveBeenCalledTimes(7);
    hide(true);
    await vi.advanceTimersByTimeAsync(30_000);
    expect(refresh).toHaveBeenCalledTimes(8);
    poll.stop();
  });

  it('coalesces events during slow requests without losing the last change', async () => {
    let resolve!: () => void;
    const refresh = vi.fn(() => new Promise<void>((done) => { resolve = done; }));
    const poll = startPolling(refresh, 1000);
    await flush();
    poll.request(); poll.request(); poll.request();
    await vi.advanceTimersByTimeAsync(10_000);
    expect(refresh).toHaveBeenCalledTimes(1);
    resolve();
    await flush();
    expect(refresh).toHaveBeenCalledTimes(2);
    poll.stop();
    resolve();
    await flush();
    hide(false);
    poll.request();
    await vi.advanceTimersByTimeAsync(10_000);
    expect(refresh).toHaveBeenCalledTimes(2);
    expect(vi.getTimerCount()).toBe(0);
  });

  it('recovers after rejection and does no hidden startup work', async () => {
    doc.hidden = true;
    const refresh = vi.fn().mockRejectedValueOnce(new Error('offline')).mockResolvedValue(undefined);
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
    const poll = startPolling(refresh, 1000);
    await vi.advanceTimersByTimeAsync(5000);
    expect(refresh).not.toHaveBeenCalled();
    hide(false);
    await flush();
    await vi.advanceTimersByTimeAsync(1000);
    expect(refresh).toHaveBeenCalledTimes(2);
    poll.stop();
    warn.mockRestore();
  });
});
