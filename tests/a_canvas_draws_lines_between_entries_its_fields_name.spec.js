import { describe, it, expect, beforeEach, vi } from 'vitest'

// A canvas list that says `lines="needs, with"` joins each card to the cards its entry names, by
// key, in those fields. The field as the template writes it says how: `line-color`, `line-style`
// and `line-arrow=true` for a relation one way. A mutual relation is drawn once for a pair, and
// nothing is drawn to a key that names no card, to a card in the strip, or to one filtered out.

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

const base = (name, node_type, parameters = {}, children = []) => ({
  name, node_type, parameters, children, is_hierarchy_transparent: false,
  source_id: '', source_fingerprint: 1, param_order: [], authored_dash: false,
})

const fields = (handle, x, y, needs = '', with_ = '') => [
  base('handle', 'string', { value: { String: handle } }),
  base('needs', 'tags', { value: { String: needs }, label: { String: 'needs' }, 'line-arrow': { Boolean: true } }),
  base('with', 'tags', {
    value: { String: with_ }, label: { String: 'goes with' },
    'line-color': { String: '#16a34a' }, 'line-style': { String: 'dashed' },
  }),
  base('x', 'float', { value: { Float: x }, hidden: { Boolean: true } }),
  base('y', 'float', { value: { Float: y }, hidden: { Boolean: true } }),
]

const theDocument = (entries) => [base('t', 'tab', { mutable: { Boolean: true } }, [
  base('Feature', 'div', { hidden: { Boolean: true } }, fields('', 0, 0)),
  base('Features', 'list', {
    entry: { Template: '<Feature>' }, key: { String: 'handle' },
    view: { String: 'canvas' }, x: { String: 'x' }, y: { String: 'y' }, lines: { String: 'needs, with' },
  }, entries.map((e) => base(e[0], 'div', {}, fields(...e)))),
])]

let app

const render = (entries) => {
  app = new OverseerApp()
  app.currentDocument = theDocument(entries)
  app.currentFile = 'd.os'
  app._currentText = 'TEXT'
  app.renderer.renderDocument(app.currentDocument)
}

const list = () => document.querySelector(`.overseer-list[data-path='${JSON.stringify(['t', 'Features'])}']`)
const lines = () => Array.from(list().querySelectorAll('.canvas-area > svg.canvas-lines > line'))
const between = (a, b) => lines().filter((l) => [l.dataset.from, l.dataset.to].sort().join() === [a, b].sort().join())

describe('a canvas draws lines between the entries its fields name', () => {
  beforeEach(() => setupDOM())

  it('joins a card to the one it needs, with a head at the one needed', () => {
    render([['A', 10, 0, 'B'], ['B', 400, 0]])
    const [line] = between('A', 'B')
    expect(lines()).toHaveLength(1)
    expect(line.dataset.from).toBe('A')
    expect(line.getAttribute('marker-end')).toMatch(/^url\(#/)
    expect(line.getAttribute('marker-start')).toBeNull()
    expect(line.querySelector('title').textContent).toBe('A needs B')
  })

  it('meets each card at its edge rather than its centre', () => {
    render([['A', 10, 0, 'B'], ['B', 400, 0]])
    const [line] = lines()
    // Cards of 220 by 80 when the page has no sizes: A's right edge, B's left one.
    expect(line.getAttribute('x1')).toBe('230')
    expect(line.getAttribute('x2')).toBe('400')
    expect(line.getAttribute('y1')).toBe('40')
  })

  it('draws a mutual pair once, whichever side names it', () => {
    render([['A', 10, 0, '', 'B'], ['B', 400, 0, '', 'A']])
    expect(between('A', 'B')).toHaveLength(1)
    expect(between('A', 'B')[0].getAttribute('marker-end')).toBeNull()
  })

  it('gives a one-way relation named from both sides a head at each end', () => {
    render([['A', 10, 0, 'B'], ['B', 400, 0, 'A']])
    const [line] = between('A', 'B')
    expect(lines()).toHaveLength(1)
    expect(line.getAttribute('marker-start')).toBe(line.getAttribute('marker-end'))
  })

  it('draws two relations between the same cards side by side', () => {
    render([['A', 10, 0, 'B', 'B'], ['B', 400, 0]])
    const ys = between('A', 'B').map((l) => l.getAttribute('y1')).sort()
    expect(ys).toHaveLength(2)
    expect(ys[0]).not.toBe(ys[1])
  })

  it('takes how a line looks from the field it comes from', () => {
    render([['A', 10, 0, '', 'B'], ['B', 400, 0]])
    const [line] = lines()
    expect(line.style.stroke).toMatch(/16a34a|rgb\(22, 163, 74\)/)
    expect(line.getAttribute('stroke-dasharray')).toBe('8 5')
  })

  it('draws nothing to a key that names no card, or to a card with no place yet', () => {
    render([['A', 10, 0, 'GONE, C'], ['B', 400, 0], ['C', 0, 0]])
    // C still stands at the template's place, in the strip.
    expect(lines()).toHaveLength(0)
  })

  it('leaves out a card that is filtered out', () => {
    render([['A', 10, 0, 'B, C'], ['B', 400, 0], ['C', 0, 300]])
    expect(lines()).toHaveLength(2)
    const c = document.querySelector(`[data-path='${JSON.stringify(['t', 'Features', 'C'])}']`)
    c.classList.add('filtered-out')
    app.renderer.drawCanvasLines(list(), app.renderer.canvasNodeOf(list()))
    expect(lines().map((l) => l.dataset.to)).toEqual(['B'])
  })

  it('draws no lines on a canvas that names none', () => {
    render([['A', 10, 0, 'B'], ['B', 400, 0]])
    delete list().dataset.canvasLines
    app.renderer.drawCanvasLines(list(), app.renderer.canvasNodeOf(list()))
    expect(list().querySelector('svg.canvas-lines')).toBeNull()
  })
})
