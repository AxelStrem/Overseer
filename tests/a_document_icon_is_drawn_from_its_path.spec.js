import { describe, it, expect, vi, beforeAll } from 'vitest'

// `<icon=icons/carrot.svg>` draws an SVG the document keeps beside it, asked for by the open
// document and its path, in its own colours; `color=` draws any icon in one colour instead.

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

const asked = []
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(async (cmd, args) => {
    if (cmd !== 'load_overseer_icon') return null
    asked.push(args)
    if (args.icon === 'icons/missing.svg') throw new Error('no icon')
    return '<svg xmlns="http://www.w3.org/2000/svg"/>'
  }),
}))
import { OverseerApp } from '../src/main.js'

beforeAll(() => {
  let n = 0
  URL.createObjectURL = () => `blob:icon-${++n}`
})

const base = (name, node_type, parameters = {}, children = []) => ({
  name, node_type, parameters, children, is_hierarchy_transparent: false,
  source_id: '', source_fingerprint: 1, param_order: [], authored_dash: false,
})

const draw = (children, file = 'kitchen/t.os') => {
  setupDOM()
  const app = new OverseerApp()
  window.app = app
  app.currentDocument = [base('t', 'tab', {}, children)]
  app.currentFile = file
  app.renderer._filters = new Map()
  app.renderer.renderDocument(app.currentDocument)
  return app
}

const at = (name) => document.querySelector(`[data-path='${JSON.stringify(['t', name])}']`)
const settle = () => new Promise((r) => setTimeout(r, 0))
const label = (text) => base('f', 'string', { label: { String: text } })

describe('a document icon is drawn from its path', () => {
  it('asks the open document for it, and draws it in its own colours', async () => {
    draw([label('<icon=icons/Carrot.svg> Carrot')])
    const slot = at('f').querySelector('label span.icon')
    expect(slot.dataset.iconFile).toBe('icons/Carrot.svg')
    expect(slot.hasAttribute('data-own-colours')).toBe(true)
    await settle()
    expect(asked).toContainEqual({ path: 'kitchen/t.os', icon: 'icons/Carrot.svg' })
    expect(slot.dataset.icon).toBe('icons/Carrot.svg')
    expect(slot.style.getPropertyValue('--icon')).toMatch(/^url\("blob:icon-\d+"\)$/)
  })

  it('is asked for once, and drawn at once the next time', async () => {
    draw([label('<icon=icons/once.svg>'), base('g', 'string', { label: { String: '<icon=icons/once.svg>' } })])
    await settle()
    const before = asked.filter((a) => a.icon === 'icons/once.svg').length
    draw([label('<icon=icons/once.svg> again')])
    const slot = at('f').querySelector('label span.icon')
    expect(slot.dataset.icon).toBe('icons/once.svg')
    expect(asked.filter((a) => a.icon === 'icons/once.svg').length).toBe(before)
    expect(before).toBe(1)
  })

  it('with a colour, is a mask in that colour', async () => {
    draw([label('<icon=icons/jar.svg color=#888> Jar')])
    const slot = at('f').querySelector('label span.icon')
    expect(slot.hasAttribute('data-own-colours')).toBe(false)
    expect(slot.style.getPropertyValue('--icon-color')).toBe('#888')
  })

  it('a library icon takes a colour too', () => {
    draw([label('<icon=clock color=red> Due')])
    const slot = at('f').querySelector('label span.icon')
    expect(slot.dataset.icon).toBe('clock')
    expect(slot.style.getPropertyValue('--icon-color')).toBe('red')
  })

  it('a path that leaves the document folder, or is not an svg, is never asked for', async () => {
    const tries = ['../up.svg', '/abs.svg', 'C:/x.svg', 'icons/./x.svg', 'icons/x.png', 'a\\b.svg']
    draw(tries.map((t, i) => base(`f${i}`, 'string', { label: { String: `<icon=${t}>` } })))
    await settle()
    for (let i = 0; i < tries.length; i++) {
      const slot = at(`f${i}`).querySelector('label span.icon')
      expect(slot.dataset.icon).toBeUndefined()
      expect(slot.dataset.iconFile).toBeUndefined()
    }
    for (const t of tries) expect(asked.some((a) => a.icon === t)).toBe(false)
  })

  it('one that is not there leaves an empty slot', async () => {
    draw([label('<icon=icons/missing.svg>')])
    await settle()
    const slot = at('f').querySelector('label span.icon')
    expect(slot.dataset.icon).toBeUndefined()
  })

  it('in markdown, keeps its path and colour through the parser', async () => {
    draw([base('m', 'text', { markdown: { Boolean: true }, value: { String: '**see** <icon=my_icons/a*b.svg color=rgb(1, 2, 3)> _here_' } })])
    await settle()
    const slot = at('m').querySelector('span.icon')
    expect(slot.dataset.iconFile).toBe('my_icons/a*b.svg')
    expect(slot.style.getPropertyValue('--icon-color')).toBe('rgb(1, 2, 3)')
    expect(slot.dataset.icon).toBe('my_icons/a*b.svg')
    expect(at('m').querySelector('strong').textContent).toBe('see')
    expect(at('m').querySelector('em').textContent).toBe('here')
  })
})
