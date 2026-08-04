// Vitest global setup: install the Tauri v2 IPC bridge on the jsdom window so
// `@tauri-apps/api` has something to call. See helpers/tauri-bridge.js.
import { installTauriBridge } from './helpers/tauri-bridge.js';

if (typeof window !== 'undefined') {
  installTauriBridge(window);
}
