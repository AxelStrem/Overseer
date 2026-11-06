import { describe, it, expect, beforeEach, vi } from 'vitest'

// The drawer is whoever is looking's drawer document, drawn in a pane; the page keeps only the
// shell around it - folding it away and back, Ctrl+\, lying over the page on a narrow screen, and
// saying an error while the status line is folded away. A document opened from it makes way for
// what was opened, and the window is named by the open document's tab.

function setupDOM() {
  document.body.className = ''
  document.body.innerHTML = `
    <div id="app" class="shell">
      <nav id="drawer" class="drawer">
        <button id="drawer-fold"></button>
        <div id="drawer-document" hidden></div>
        <div id="status-bar"><span id="status-message"></span><span id="status-info"></span></div>
      </nav>
      <div id="drawer-backdrop"></div>
      <main id="main">
        <div id="main-bar"><button id="drawer-open"></button><div id="tab-container"></div></div>
        <div id="content-area">
          <div id="welcome-screen" class="screen active">
            <button id="welcome-open-btn"></button><button id="welcome-new-btn"></button>
          </div>
          <div id="editor-screen" class="screen"><div id="content-display"></div></div>
          <div id="error-screen" class="screen"><p id="error-message"></p><button id="error-back-btn"></button></div>
        </div>
      </main>
    </div>`
}

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(async () => null) }))
import { invoke } from '@tauri-apps/api/core'
import { OverseerApp } from '../src/main.js'
import { Drawer } from '../src/drawer.js'
import { runCall } from '../src/calls.js'

const TASKS = 'E:\\docs\\tasks.os'
const PROJECT = 'E:\\docs\\projects\\overseer.os'

const withADrawer = async (open = TASKS) => {
  const app = new OverseerApp()
  app.currentFile = open
  app.currentDocument = [{ name: 'tasks', node_type: 'tab', parameters: { label: { String: 'Tasks' } }, children: [], is_hierarchy_transparent: false }]
  app.loadFile = vi.fn(async (path) => { app.currentFile = path })
  const drawer = new Drawer(app)
  app.drawer = drawer
  await drawer.start()
  return { app, drawer }
}

const narrow = (yes) => Object.defineProperty(window, 'innerWidth', { configurable: true, value: yes ? 400 : 1400 })

describe('the drawer', () => {
  beforeEach(() => {
    setupDOM()
    try { localStorage.clear() } catch (_) {}
    delete window.__OVERSEER_HOST__
    invoke.mockReset()
    invoke.mockImplementation(async () => null)
    narrow(false)
  })

  it('folds away and back, by its buttons and by Ctrl+\\, and remembers it', async () => {
    const { drawer } = await withADrawer()
    expect(drawer.isFolded()).toBe(false)
    document.getElementById('drawer-fold').click()
    expect(drawer.isFolded()).toBe(true)
    expect(localStorage.getItem('overseer.drawer.folded')).toBe('1')
    document.getElementById('drawer-open').click()
    expect(drawer.isFolded()).toBe(false)
    document.dispatchEvent(new KeyboardEvent('keydown', { key: '\\', ctrlKey: true }))
    expect(drawer.isFolded()).toBe(true)
  })

  it('starts folded on a narrow screen, where it would cover the page', async () => {
    narrow(true)
    const { drawer } = await withADrawer()
    expect(drawer.isFolded()).toBe(true)
  })

  it('says an error while the status line is folded away, and nothing while it is not', async () => {
    const { app, drawer } = await withADrawer()
    app.setStatus('Could not save', 'the disk is full', 'error')
    expect(document.getElementById('drawer-notice')).toBe(null)
    drawer.fold(true)
    app.setStatus('Could not save', 'the disk is full', 'error')
    expect(document.getElementById('drawer-notice').textContent).toBe('Could not save: the disk is full')
  })

  it('names the window by the open document\'s tab, an inherited one too, else by its file', async () => {
    const { app } = await withADrawer()
    app.showDocumentName()
    expect(document.title).toBe('Tasks - Overseer')
    app.currentDocument = [{
      name: 'project',
      node_type: 'project',
      parameters: { label: { String: 'Overseer' }, _original_type: { String: 'tab' } },
      children: [],
      is_hierarchy_transparent: false,
    }]
    app.showDocumentName()
    expect(document.title).toBe('Overseer - Overseer')
    app.currentDocument = [{ name: 'loose', node_type: 'div', parameters: { label: { String: 'Loose' } }, children: [] }]
    app.showDocumentName()
    expect(document.title).toBe('tasks.os - Overseer')
  })

  it('opens what a call names by loading its file in the desktop app, making way where it lies over the page', async () => {
    narrow(true)
    const { app, drawer } = await withADrawer()
    drawer.fold(false)
    expect((await runCall({ name: 'open', args: { path: PROJECT } }, app)).failed).toBe(false)
    expect(app.loadFile).toHaveBeenCalledWith(PROJECT)
    expect(drawer.isFolded()).toBe(true)
    // The one open already is only made way for.
    drawer.fold(false)
    await runCall({ name: 'open', args: { path: PROJECT } }, app)
    expect(app.loadFile).toHaveBeenCalledTimes(1)
    expect(drawer.isFolded()).toBe(true)
  })

  it('opens what a call names by going to its address in a browser, and every document where it names none', async () => {
    window.__OVERSEER_HOST__ = 'browser'
    const assign = vi.fn()
    const location = window.location
    delete window.location
    window.location = { ...location, assign }
    try {
      const { app } = await withADrawer('tasks.os')
      await runCall({ name: 'open', args: { path: 'projects/black spores.os' } }, app)
      expect(assign).toHaveBeenLastCalledWith('/doc/projects/black%20spores.os')
      await runCall({ name: 'open', args: {} }, app)
      expect(assign).toHaveBeenLastCalledWith('/')
    } finally {
      window.location = location
    }
  })

  it('opens whoever is looking\'s own drawer document in the pane', async () => {
    invoke.mockImplementation(async (cmd) => (cmd === 'drawer_location' ? 'drawer.os' : null))
    const { app } = await withADrawer()
    const opened = []
    app.openDrawerDocument = async (path) => { opened.push(path); return null }
    await app.openOwnDrawer()
    expect(opened).toEqual(['drawer.os'])
  })
})
