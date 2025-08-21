import { defineConfig } from 'vitest/config'

export default defineConfig({
  test: {
    environment: 'jsdom',
    include: ['tests/**/*.spec.js'],
    globals: true,
  },
  // Do not load Vite app config when running tests
  resolve: {
    conditions: []
  }
})
