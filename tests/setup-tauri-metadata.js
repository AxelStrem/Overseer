// Vitest global setup to stub Tauri window metadata to silence warnings during jsdom tests.
if (typeof window !== 'undefined') {
  if (!window.__TAURI_METADATA__) {
    window.__TAURI_METADATA__ = {
      __currentWindow: { label: 'main' },
      windows: [{ label: 'main' }],
      invoke: () => Promise.resolve(),
      transformCallback: (cb, _once) => cb,
    };
  }
}
