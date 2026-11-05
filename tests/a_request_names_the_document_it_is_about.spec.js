import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'
import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'

// In a browser every command goes to the server through the bridge, which says which document it
// is about: the tab's own, unless the invoke's options name another - the drawer's document, with
// the one open beside it, which the server tells it of as `host/open`.

const bridge = readFileSync(resolve(__dirname, '../src-tauri/src/bridge.js'), 'utf8')

let internals
let fetched

const sent = () => JSON.parse(fetched.mock.calls[fetched.mock.calls.length - 1][1].body)

describe('a request through the bridge', () => {
  beforeEach(() => {
    internals = window.__TAURI_INTERNALS__
    window.history.replaceState(null, '', '/?doc=tasks.os')
    fetched = vi.fn(async () => ({ ok: true, status: 200, json: async () => ({ result: 'answered' }) }))
    vi.stubGlobal('fetch', fetched)
    new Function(bridge)()
  })

  afterEach(() => {
    window.__TAURI_INTERNALS__ = internals
    delete window.__OVERSEER_HOST__
    delete window.__overseerWatch
    vi.unstubAllGlobals()
    window.history.replaceState(null, '', '/')
  })

  it('is about the tab\'s document when it names none', async () => {
    expect(await window.__TAURI_INTERNALS__.invoke('load_overseer_file', { path: 'tasks.os' })).toBe('answered')
    expect(sent().document).toBe('tasks.os')
    expect(sent().args).toEqual({ path: 'tasks.os' })
    expect('open' in sent()).toBe(false)
  })

  it('is about the document its options name, with the one open beside it', async () => {
    await window.__TAURI_INTERNALS__.invoke('load_overseer_file', { path: 'drawer.os' },
      { headers: { 'Overseer-Document': 'drawer.os', 'Overseer-Open': 'tasks.os' } })
    expect(sent().document).toBe('drawer.os')
    expect(sent().open).toBe('tasks.os')
    expect(sent().args).toEqual({ path: 'drawer.os' })
  })

  it('reads names the pane encoded as they were', async () => {
    await window.__TAURI_INTERNALS__.invoke('load_overseer_file', {},
      { headers: { 'Overseer-Document': encodeURIComponent('Заметки/ящик.os'), 'Overseer-Open': encodeURIComponent('Заметки/дом.os') } })
    expect(sent().document).toBe('Заметки/ящик.os')
    expect(sent().open).toBe('Заметки/дом.os')
  })

  it('reads the names from headers given as Headers too', async () => {
    await window.__TAURI_INTERNALS__.invoke('catch_up_overseer_document', {},
      { headers: new Headers({ 'Overseer-Document': 'drawer.os' }) })
    expect(sent().document).toBe('drawer.os')
    expect('open' in sent()).toBe(false)
  })

  it('watches a pane\'s document, and stops when told', () => {
    const opened = []
    class FakeSource {
      constructor(url) { this.url = url; this.closed = false; opened.push(this) }
      addEventListener() {}
      close() { this.closed = true }
    }
    vi.stubGlobal('EventSource', FakeSource)
    const stop = window.__overseerWatch('drawer.os', {})
    expect(opened.map((s) => s.url)).toEqual(['/v1/watch?document=drawer.os'])
    stop()
    expect(opened[0].closed).toBe(true)
  })
})
