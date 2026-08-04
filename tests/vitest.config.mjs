import { defineConfig } from 'vitest/config'

export default defineConfig({
  test: {
    environment: 'jsdom',
    include: ['tests/**/*.spec.js'],
    globals: true,
    setupFiles: ['tests/setup-tauri-internals.js']
  },
  // Do not load Vite app config when running tests
  resolve: {
    conditions: []
  }
})
