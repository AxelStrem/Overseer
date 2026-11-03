import { describe, it, expect, beforeEach, vi } from 'vitest'

// A box that narrows a tree and says `branch` brings everything under a match along with it, at
// every depth, while the match's ancestors stay in sight, dimmed. `narrow` puts a value in such a
// box from a press - a row picks itself - and a second press with the same value lets go of it.
// A box that picks from a field holding one value wants any of its picks, not all of them.

function setupDOM() {
  document.body.innerHTML = `
    <div id="app">
      <div id="toolbar"><button id="open-file-btn"></button><button id="new-file-btn"></button>
        </div>
      <button id="welcome-open-btn"></button><button id="welcome-new-btn"></button>
      <button id="error-back-btn"></button>
      <div id="tab-container"></div><div id="content-display"></div>
      <div id="status-bar"><span id="status-message"></span><span id="status-info"></span>
        <span id="file-path"></span></div>
      <div id="welcome-screen" class="screen"></div><div id="editor-screen" class="screen"></div>
      <div id="error-screen" class="screen"></div><div id="error-message"></div>
    </div>`
}

const invoke = vi.fn(async () => null)
vi.mock('@tauri-apps/api/core', () => ({ invoke: (...args) => invoke(...args) }))
import { OverseerApp } from '../src/main.js'

const base = (name, node_type, parameters = {}, children = []) => ({
  name, node_type, parameters, children, is_hierarchy_transparent: name === '',
  source_id: '', source_fingerprint: 1, param_order: [], authored_dash: false,
})
const valued = (name, type, value, more = {}) => base(name, type, { value: { String: value }, ...more })
const said = (params) => Object.fromEntries(Object.entries(params).map(([k, v]) => [k, { String: v }]))

const picksItself = base('click', 'on', {}, [
  base('', 'narrow', { path: { String: '/project/Narrow/branch' }, value: { Formula: './handle' } }),
])
const item = (handle, parent, title) => base(`[${handle}]`, 'div', {}, [
  picksItself,
  base('', 'div', {}, [
    valued('handle', 'string', handle),
    valued('parent', 'string', parent),
    valued('title', 'string', title),
  ]),
])

const docOf = () => ([base('project', 'tab', { mutable: { Boolean: true } }, [
  base('Narrow', 'div', { layout: { String: 'horizontal' } }, [
    valued('find', 'textbox', '', said({ placeholder: 'find', narrows: '/project/Items', by: 'title' })),
    valued('branch', 'textbox', '', {
      ...said({ vocabulary: '/project/Items', narrows: '/project/Items', by: 'handle' }),
      branch: { Boolean: true },
    }),
  ]),
  base('Items', 'list', { key: { String: 'handle' }, tree: { String: 'parent' } }, [
    item('plan', '', 'The plan'),
    item('parse', 'plan', 'Parse it'),
    item('lex', 'parse', 'Lex it'),
    item('draw', 'plan', 'Draw it'),
    item('docs', '', 'Write the docs'),
  ]),
])])

// By entry, not by element: the entry and the layout div inside it share one path.
const rows = () => {
  const seen = new Map()
  for (const element of document.querySelectorAll('[data-path]')) {
    let path
    try { path = JSON.parse(element.dataset.path) } catch { continue }
    if (path.length !== 3 || path[1] !== 'Items') continue
    const was = seen.get(element.dataset.path) || { out: false, dim: false }
    seen.set(element.dataset.path, {
      out: was.out || element.classList.contains('filtered-out'),
      dim: was.dim || element.classList.contains('tree-ancestor'),
    })
  }
  const handleOf = (key) => JSON.parse(key)[2].replace(/^\[|\]$/g, '')
  return {
    shown: [...seen].filter(([, s]) => !s.out && !s.dim).map(([key]) => handleOf(key)),
    dimmed: [...seen].filter(([, s]) => !s.out && s.dim).map(([key]) => handleOf(key)),
  }
}

const boxKey = JSON.stringify(['project', 'Narrow', 'branch'])
// Looked for by hand: the selector engine reads the brackets of a key inside a quoted value as its own.
const rowOf = (handle) => [...document.querySelectorAll('[data-path]')]
  .find((element) => element.dataset.path === JSON.stringify(['project', 'Items', `[${handle}]`]))
const fieldOf = (name) => document.querySelector(`[data-path='${JSON.stringify(['project', 'Narrow', name])}']`)
const type = (what) => {
  const input = fieldOf('find').querySelector('input')
  input.value = what
  input.dispatchEvent(new Event('input', { bubbles: true }))
}

let app
const open = (doc) => {
  setupDOM()
  app = new OverseerApp()
  app.currentDocument = doc
  app._currentText = 'TEXT'
  app.renderer._filters = new Map()
  app.renderer.renderDocument(app.currentDocument)
  app.renderer.applyFilters()
}

describe('a box that narrows a tree to a branch', () => {
  beforeEach(() => {
    invoke.mockClear()
    open(docOf())
  })

  it('shows the root and everything under it, its ancestors dimmed, and nothing else', () => {
    app.renderer.narrowTo(boxKey, 'parse')
    expect(rows()).toEqual({ shown: ['parse', 'lex'], dimmed: ['plan'] })
  })

  it('is picked by pressing a row, and let go by pressing it again, without asking the server', async () => {
    await app.renderer.emitEvent(app.currentDocument[0].children[1].children[3], rowOf('draw'), 'click')
    expect(rows()).toEqual({ shown: ['draw'], dimmed: ['plan'] })
    expect(fieldOf('branch').textContent).toContain('draw')
    await app.renderer.emitEvent(app.currentDocument[0].children[1].children[3], rowOf('draw'), 'click')
    expect(rows().shown).toEqual(['plan', 'parse', 'lex', 'draw', 'docs'])
    expect(invoke.mock.calls.filter(([name]) => String(name).startsWith('execute_overseer_event'))).toEqual([])
  })

  it('takes a picked row from what the server answers', () => {
    const at = [0, 0, 1]
    app.renderer.applyNarrowings([{ address: 'project/Narrow/branch', path: at, value: 'plan' }])
    expect(rows()).toEqual({ shown: ['plan', 'parse', 'lex', 'draw'], dimmed: [] })
  })

  it('shows every branch picked, since a handle is one value', () => {
    app.renderer.narrowTo(boxKey, 'lex, docs')
    expect(rows()).toEqual({ shown: ['lex', 'docs'], dimmed: ['plan', 'parse'] })
  })

  it('narrows within the branch by what the other boxes say', () => {
    app.renderer.narrowTo(boxKey, 'plan')
    type('lex')
    expect(rows()).toEqual({ shown: ['lex'], dimmed: ['plan', 'parse'] })
  })

  it('keeps what it holds through a repaint, and never sends it with a press', () => {
    app.renderer.narrowTo(boxKey, 'parse')
    app.renderer.renderDocument(app.currentDocument)
    app.renderer.applyFilters()
    expect(rows()).toEqual({ shown: ['parse', 'lex'], dimmed: ['plan'] })
    expect(app.renderer.typedText()).toEqual([])
  })
})
