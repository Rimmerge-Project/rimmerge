import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { onScopeDispose } from "vue";

/**
 * Subscribes to a Tauri event (`name`) for the lifetime of the calling
 * component/composable's reactive scope, calling `handler` with each
 * event's payload. Registration is fire-and-forget — `listen` itself is
 * async, and a lost registration only costs the first event a late
 * subscriber would've missed anyway (a cosmetic gap, not a correctness
 * one), so callers don't need an `async setup()` just to await it.
 *
 * That asynchrony has a sharp edge this guards against: if the scope
 * disposes (component unmounts) *before* `listen` resolves,
 * `onScopeDispose` runs while there's no `UnlistenFn` yet to call —
 * naively stashing whatever `listen` later resolves to would leave that
 * listener registered forever, calling `handler` for a scope that's
 * already gone. `disposed` remembers that the teardown already happened
 * so the resolved `UnlistenFn` gets invoked immediately instead of
 * stored.
 */
export function useTauriEvent<T>(name: string, handler: (payload: T) => void): void {
  let unlisten: UnlistenFn | undefined;
  let disposed = false;

  void listen<T>(name, (event) => handler(event.payload)).then((stop) => {
    if (disposed) {
      stop();
      return;
    }
    unlisten = stop;
  });

  onScopeDispose(() => {
    disposed = true;
    unlisten?.();
  });
}
