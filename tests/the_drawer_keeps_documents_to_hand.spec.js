import { describe, it, expect, beforeEach, vi } from 'vitest'

// The drawer: the documents kept to hand, and what there is to do with the open one.
//
// The list comes from the host - `menu_places` and friends - so the page never decides what is in
// it, only shows it, marks the one that is open, and asks for changes. Opening one is loading its
// file in the desktop app and going to its address in a browser. Folded, it leaves the document
// the screen, and says an error it would otherwise have shown in its status line.

function setupDOM() {
  document.body.className = ''
  document.body.innerHTML = `
    <div id="app" class="shell">
      <nav id="drawer" class="drawer">
        <button id="drawer-fold"></button>
        <ul id="drawer-places"></ul>
        <button id="drawer-keep" hidden></button>
        <button id="drawer-edit-list"></button>
        <div id="file-path"></div>
        <button id="undo-file-btn" disabled></button>
        <button id="open-file-btn"></button>
        <button id="new-file-btn"></button>
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

let places
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }))
import { invoke } from '@tauri-apps/api/core'
import { OverseerApp } from '../src/main.js'
import { Drawer } from '../src/drawer.js'

const TASKS = 'E:\\docs\\tasks.os'
const PROJECT = 'E:\\docs\\projects\\overseer.os'

const answering = () => invoke.mockImplementation(async (cmd, args) => {
  if (cmd === 'menu_places') return places
  if (cmd === 'menu_keep') {
    if (!places.some((p) => p.path === args.place)) places = [...places, { path: args.place, name: args.name || '' }]
    return places
  }
  if (cmd === 'menu_drop') { places = places.filter((p) => p.path !== args.place); return places }
  if (cmd === 'menu_location') return 'C:\\settings\\menu.os'
  return null
})

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

const shown = () => Array.from(document.querySelectorAll('.drawer-place')).map((li) => ({
  name: li.querySelector('.drawer-place-open').textContent,
  current: li.classList.contains('current'),
}))
const calls = (name) => invoke.mock.calls.filter(([cmd]) => cmd === name)

describe('the drawer', () => {
  beforeEach(() => {
    setupDOM()
    try { localStorage.clear() } catch (_) {}
    delete window.__OVERSEER_HOST__
    invoke.mockReset()
    places = [{ path: TASKS, name: 'Chores' }, { path: PROJECT, name: '' }]
    answering()
  })

  it('lists what is kept, by name or by file, and marks the open one', async () => {
    await withADrawer()
    expect(shown()).toEqual([{ name: 'Chores', current: true }, { name: 'overseer', current: false }])
  })

  it('offers to keep the open document only when it is not kept already', async () => {
    await withADrawer()
    expect(document.getElementById('drawer-keep').hidden).toBe(true)
    places = [{ path: PROJECT, name: '' }]
    const { drawer } = await withADrawer()
    expect(document.getElementById('drawer-keep').hidden).toBe(false)
    await drawer.keep()
    expect(calls('menu_keep').slice(-1)[0][1]).toEqual({ place: TASKS, name: 'Tasks' })
    expect(document.getElementById('drawer-keep').hidden).toBe(true)
    expect(shown().map((s) => s.name)).toEqual(['overseer', 'Tasks'])
  })

  it('takes one off the list', async () => {
    await withADrawer()
    document.querySelectorAll('.drawer-place-drop')[1].click()
    await new Promise((r) => setTimeout(r, 0))
    expect(calls('menu_drop')[0][1]).toEqual({ place: PROJECT })
    expect(shown().map((s) => s.name)).toEqual(['Chores'])
  })

  it('opens one by loading its file in the desktop app', async () => {
    const { app } = await withADrawer()
    document.querySelectorAll('.drawer-place-open')[1].click()
    await new Promise((r) => setTimeout(r, 0))
    expect(app.loadFile).toHaveBeenCalledWith(PROJECT)
  })

  it('opens one by going to its address in a browser', async () => {
    window.__OVERSEER_HOST__ = 'browser'
    places = [{ path: 'tasks.os', name: '' }, { path: 'projects/black spores.os', name: '' }]
    const { app, drawer } = await withADrawer('tasks.os')
    drawer.navigate = vi.fn()
    document.querySelectorAll('.drawer-place-open')[1].click()
    await new Promise((r) => setTimeout(r, 0))
    expect(drawer.navigate).toHaveBeenCalledWith('/doc/projects/black%20spores.os')
    expect(app.loadFile).not.toHaveBeenCalled()
    expect(document.body.classList.contains('in-browser')).toBe(true)
  })

  it('opens its own list for renaming and reordering', async () => {
    const { app } = await withADrawer()
    document.getElementById('drawer-edit-list').click()
    await new Promise((r) => setTimeout(r, 0))
    expect(app.loadFile).toHaveBeenCalledWith('C:\\settings\\menu.os')
  })

  it('folds away and comes back, from its buttons and from Ctrl+\\, and remembers', async () => {
    await withADrawer()
    document.getElementById('drawer-fold').click()
    expect(document.body.classList.contains('drawer-folded')).toBe(true)
    expect(localStorage.getItem('overseer.drawer.folded')).toBe('1')
    document.getElementById('drawer-open').click()
    expect(document.body.classList.contains('drawer-folded')).toBe(false)
    document.dispatchEvent(new window.KeyboardEvent('keydown', { key: '\\', ctrlKey: true }))
    expect(document.body.classList.contains('drawer-folded')).toBe(true)
  })

  it('says an error it cannot show while folded', async () => {
    const { app, drawer } = await withADrawer()
    drawer.fold(true)
    app.setStatus('Failed to load file', 'no such file', 'error')
    const notice = document.getElementById('drawer-notice')
    expect(notice && notice.textContent).toBe('Failed to load file: no such file')
    expect(notice.classList.contains('shown')).toBe(true)
  })

  it('marks another document when that one is opened', async () => {
    const { app } = await withADrawer()
    app.currentFile = PROJECT
    app.updateTitle()
    expect(shown()).toEqual([{ name: 'Chores', current: false }, { name: 'overseer', current: true }])
  })
})
