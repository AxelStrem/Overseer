import { defineConfig } from 'vitest/config'

export default defineConfig({
  test: {
    environment: 'jsdom',
    // Each file still isolated, which the specs' per-file `vi.mock`s need, but in a worker thread
    // rather than a forked process, which starts faster.
    pool: 'threads',
    include: ['tests/**/*.spec.js'],
    globals: true,
    setupFiles: ['tests/setup-tauri-internals.js']
  },
  // Do not load Vite app config when running tests
  resolve: {
    conditions: []
  }
})
