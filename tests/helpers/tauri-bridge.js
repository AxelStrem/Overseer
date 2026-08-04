// The Tauri v2 IPC bridge that `@tauri-apps/api` reads off `window`.
//
// Specs that assert on backend calls install their own
// `vi.mock('@tauri-apps/api/core', ...)`. This bridge is the fallback for specs
// that only exercise UI behaviour, so an unmocked `invoke` resolves quietly
// instead of throwing on an undefined bridge.
//
// It has to be installed on every window the specs run against: the global
// setup applies it to the vitest jsdom window, and `bootstrapMinimalDom`
// re-applies it because it swaps in a fresh JSDOM instance.
export function installTauriBridge(target) {
  if (!target || target.__TAURI_INTERNALS__) return target;
  target.__TAURI_INTERNALS__ = {
    invoke: () => Promise.resolve(),
    transformCallback: (cb, _once) => cb,
    convertFileSrc: (path) => path,
    metadata: {
      currentWindow: { label: 'main' },
      currentWebview: { windowLabel: 'main', label: 'main' },
    },
  };
  return target;
}
