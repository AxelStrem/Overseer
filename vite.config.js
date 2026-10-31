import { defineConfig } from 'vite'
import fs from 'node:fs'
import path from 'node:path'
import { iconIndex, resolveIcon, writeIconLibrary } from './src/icons/library.mjs'

// The icon library goes out beside the bundle as one file per icon, so the page fetches only
// the icons it draws. The dev server answers the same addresses straight from the sources.
function iconLibrary() {
  let outDir = 'dist'
  return {
    name: 'overseer-icons',
    configResolved(config) {
      outDir = path.resolve(config.root, config.build.outDir)
    },
    configureServer(server) {
      server.middlewares.use('/icons/', (req, res, next) => {
        if (/^[/]index[.]json(?:[?#].*)?$/.test(req.url || '')) {
          res.setHeader('Content-Type', 'application/json')
          return res.end(iconIndex())
        }
        const match = /^[/]([^/?#]+)[.]svg(?:[?#].*)?$/.exec(req.url || '')
        const file = match && resolveIcon(match[1])
        if (!file) return next()
        res.setHeader('Content-Type', 'image/svg+xml')
        res.end(fs.readFileSync(file))
      })
    },
    closeBundle() {
      writeIconLibrary(outDir)
    },
  }
}

export default defineConfig({
  // Vite options tailored for Tauri development
  plugins: [iconLibrary()],
  clearScreen: false,
  // Tauri expects a fixed port, fail if that port is not available
  server: {
    host: '127.0.0.1',
    port: 1420,
    strictPort: true,
  },
  // To make use of `TAURI_DEBUG` and other env variables
  // https://tauri.studio/v1/api/config#buildconfig.beforedevcommand
  envPrefix: ['VITE_', 'TAURI_'],
  build: {
    // Tauri supports es2021
    target: process.env.TAURI_PLATFORM == 'windows' ? 'chrome105' : 'safari13',
    // Don't minify for debug builds
    minify: !process.env.TAURI_DEBUG ? 'esbuild' : false,
    // Produce sourcemaps for debug builds
    sourcemap: !!process.env.TAURI_DEBUG,
  },
})
