/** Completion-based polling with visibility suspension and one trailing refresh.
 * Events during a request are reconciled once more so changes cannot be lost.
 */
export function startPolling(
  refresh: () => Promise<unknown>,
  intervalMs: number | (() => number),
  options: { hiddenIntervalMs?: number; immediate?: boolean } = {},
): { request: () => void; stop: () => void } {
  let stopped = false;
  let running = false;
  let pending = false;
  let timer: ReturnType<typeof setTimeout> | undefined;

  function schedule() {
    clearTimeout(timer);
    if (stopped) return;
    const delay = document.hidden
      ? options.hiddenIntervalMs
      : typeof intervalMs === 'function' ? intervalMs() : intervalMs;
    if (delay !== undefined) timer = setTimeout(request, delay);
  }

  function request() {
    if (stopped) return;
    clearTimeout(timer);
    if (running) {
      pending = true;
      return;
    }
    running = true;
    void Promise.resolve().then(() => stopped ? undefined : refresh()).catch((error) => {
      console.warn('Background refresh failed:', error);
    }).finally(() => {
      running = false;
      if (stopped) return;
      if (pending) {
        pending = false;
        request();
      } else {
        schedule();
      }
    });
  }

  function visibilityChanged() {
    if (!document.hidden) request();
    else schedule();
  }
  document.addEventListener('visibilitychange', visibilityChanged);
  if (options.immediate !== false && !document.hidden) request();
  else schedule();

  return {
    request,
    stop() {
      stopped = true;
      pending = false;
      clearTimeout(timer);
      document.removeEventListener('visibilitychange', visibilityChanged);
    },
  };
}
