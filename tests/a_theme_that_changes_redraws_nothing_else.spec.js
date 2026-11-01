import { describe, it, expect, beforeEach, vi } from 'vitest'

// A theme key with a formula - a backdrop that follows a count, as the Game of Life's glow follows
// its population - changes with what it reads. The theme is written again where it is said, and
// nothing else on the page is drawn again: drawing the whole document for it, on every tick of the
// game, started every motion on the page over. A clock's `tick=` is read as it beats, and draws
// nothing either.

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

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(async () => null) }))
import { OverseerApp } from '../src/main.js'

const value = (v) => (typeof v === 'number' ? { Integer: v } : { String: v })

const key = (name, v) => ({
  name, node_type: '-', children: [], is_hierarchy_transparent: false, parameters: { value: value(v) },
})

const block = (name, children) => ({
  name, node_type: '-', children, is_hierarchy_transparent: false, parameters: {},
})

const theme = (keys) => ({
  name: 'theme', node_type: 'context', is_hierarchy_transparent: false, children: keys,
  parameters: { _context_copy: { Boolean: true } },
})

const field = (name, v) => ({
  name, node_type: 'string', children: [], is_hierarchy_transparent: false, parameters: { value: value(v) },
})

const div = (name, children, parameters = {}) => ({
  name, node_type: 'div', parameters, is_hierarchy_transparent: !name, children,
})

const elementAt = (path) => document.querySelector(`[data-path='${JSON.stringify(path)}']`)

const glow = (center) => block('backdrop', [block('glow', [
  key('kind', 'gradient'), key('from', 'red'), key('to', 'blue'), key('motion', 'turn'), key('center', center),
])])

function opened(doc) {
  const app = new OverseerApp()
  app.currentDocument = doc
  app.renderer.renderDocument(app.currentDocument)
  const full = vi.spyOn(app.renderer, 'renderDocument')
  return { app, full }
}

describe('a theme that changes', () => {
  beforeEach(() => {
    setupDOM()
    document.documentElement.removeAttribute('style')
    document.body.querySelectorAll('.ov-backdrop-motion').forEach((el) => el.remove())
  })

  it('at the top of the document is written again, and nothing else is drawn', () => {
    const { app, full } = opened([
      theme([key('accent', 'green'), glow('20%')]),
      div('life', [field('title', 'Game of Life'), field('count', '1')]),
    ])
    const title = elementAt(['life', 'title'])
    const box = document.querySelector('.ov-backdrop-motion > [data-layer="glow"]')
    expect(box).not.toBeNull()

    app.showLooked({ changes: [
      { kind: 'parameters', path: [0, 0], parameters: { value: value('red') } },
      { kind: 'parameters', path: [0, 1, 0, 4], parameters: { value: value('60%') } },
    ] })

    expect(full).not.toHaveBeenCalled()
    expect(elementAt(['life', 'title'])).toBe(title)
    expect(document.documentElement.style.getPropertyValue('--ov-accent')).toBe('red')
    expect(document.documentElement.style.getPropertyValue('--ov-backdrop')).toContain('red 10%, blue 110%')
    // The turning box is the one that was turning, so it goes on from where it was.
    expect(document.querySelector('.ov-backdrop-motion > [data-layer="glow"]')).toBe(box)
  })

  it('on a container is written again on that container alone, unnamed as it may be', () => {
    const { app, full } = opened([
      div('game', [
        field('status', 'Tap to dig'),
        div(undefined, [theme([key('accent', 'green')]), field('cell', 'x')]),
      ]),
    ])
    const status = elementAt(['game', 'status'])
    const cell = elementAt(['game', 'cell'])
    const holder = cell.parentElement.closest('.overseer-div')

    app.showLooked({ changes: [
      { kind: 'parameters', path: [0, 1, 0, 0], parameters: { value: value('orange') } },
    ] })

    expect(full).not.toHaveBeenCalled()
    expect(elementAt(['game', 'status'])).toBe(status)
    expect(elementAt(['game', 'cell'])).toBe(cell)
    expect(holder.style.getPropertyValue('--ov-accent')).toBe('orange')
  })

  it('in its own motion is drawn again, since motions are read as things are drawn', () => {
    const { app, full } = opened([
      theme([key('motion', 'full')]),
      div('life', [field('title', 'Game of Life')]),
    ])
    app.showLooked({ changes: [{ kind: 'parameters', path: [0, 0], parameters: { value: value('none') } }] })
    expect(full).toHaveBeenCalled()
  })
})

describe('a clock that starts or stops', () => {
  beforeEach(setupDOM)

  it('draws nothing again', () => {
    const { app, full } = opened([
      div('life', [field('title', 'Game of Life')], { tick: { Integer: 0 } }),
    ])
    const title = elementAt(['life', 'title'])
    app.showLooked({ changes: [
      { kind: 'parameters', path: [0], parameters: { tick: { String: '200ms' } } },
    ] })
    expect(full).not.toHaveBeenCalled()
    expect(elementAt(['life', 'title'])).toBe(title)
    expect(app.currentDocument[0].parameters.tick).toEqual({ String: '200ms' })
  })
})
