import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'

// A chip whose entry has a title says it on hover, in place of what its column says; and a
// field's hover text, which can differ from row to row, draws a `<#key>` as its field's chip.

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

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }))
import { OverseerApp } from '../src/main.js'

const base = (name, node_type) => ({
  name, node_type, parameters: {}, children: [], is_hierarchy_transparent: false,
  source_id: '', source_fingerprint: 1, param_order: [], authored_dash: false,
})

const valued = (name, type, variant, value) => Object.assign(base(name, type), {
  parameters: { value: { [variant]: value } },
})

const entry = (tag, name, colour, title) => Object.assign(base(`Label__${tag}`, 'div'), {
  children: [
    valued('tag', 'string', 'String', tag),
    valued('name', 'string', 'String', name),
    valued('colour', 'string', 'String', colour),
    ...(title ? [valued('title', 'string', 'String', title)] : []),
  ],
})

const field = (name, type, held, extra = {}) => Object.assign(base(name, type), {
  parameters: { value: { String: held }, vocabulary: { String: '/project/Labels' }, ...extra },
})

const docWith = (fields) => ([
  Object.assign(base('project', 'tab'), {
    parameters: { mutable: { Boolean: true } },
    children: [
      Object.assign(base('Labels', 'list'), {
        parameters: { key: { String: 'tag' }, hidden: { Boolean: true } },
        children: [entry('ready', 'ready', '#1d4ed8', 'Nothing left to decide'),
                   entry('ui', 'ui', '#0369a1', '')],
      }),
      ...fields,
    ],
  }),
])

const said = (text) => ({ 'hover-text': { String: text } })
const cell = (name) => document.querySelector(`[data-path='${JSON.stringify(['project', name])}']`)
const chipsOf = (name) => Array.from(cell(name).querySelectorAll('.tag-chip:not(.tag-option)'))
const popup = () => document.querySelector('.overseer-hover-text')
const hover = (el) => el.dispatchEvent(new MouseEvent('mouseover', { bubbles: true }))
const leave = (el) => el.dispatchEvent(new MouseEvent('mouseout', { bubbles: true }))

describe('hover text over a chip', () => {
  let app

  const render = (doc) => {
    app = new OverseerApp()
    app.currentDocument = doc
    app._currentText = 'TEXT'
    app.renderer.renderDocument(app.currentDocument)
  }

  beforeEach(() => {
    setupDOM()
    document.querySelectorAll('.overseer-hover-text').forEach((n) => n.remove())
    vi.useFakeTimers()
  })
  afterEach(() => vi.useRealTimers())

  it("says the chip's own title, not its column's", () => {
    render(docWith([field('labels', 'tags', 'ready, ui', said('Project tags'))]))
    hover(chipsOf('labels')[0])
    vi.advanceTimersByTime(400)
    expect(popup().style.display).toBe('block')
    expect(popup().textContent.startsWith('Nothing left to decide')).toBe(true)
  })

  it("says the column's text over a chip with no title of its own", () => {
    render(docWith([field('labels', 'tags', 'ready, ui', said('Project tags'))]))
    hover(chipsOf('labels')[1])
    vi.advanceTimersByTime(400)
    expect(popup().textContent).toBe('Project tags')
  })

  it("puts the browser's own tooltip away while the popup says it, and back after", () => {
    render(docWith([field('labels', 'tags', 'ready', said('Project tags'))]))
    const [chip] = chipsOf('labels')
    expect(chip.title.startsWith('Nothing left to decide')).toBe(true)
    hover(chip)
    vi.advanceTimersByTime(400)
    expect(chip.hasAttribute('title')).toBe(false)
    leave(chip)
    expect(chip.title.startsWith('Nothing left to decide')).toBe(true)
  })

  it("draws a key in the field's text as the field's chip, its own title under it", () => {
    render(docWith([field('stage', 'enum', 'ready', said('This item is at the <#ready> stage'))]))
    hover(chipsOf('stage')[0])
    vi.advanceTimersByTime(400)
    const lines = Array.from(popup().children)
    expect(lines).toHaveLength(2)
    const drawn = lines[0].querySelector('.tag-chip')
    expect(drawn.querySelector('.chip-name').textContent).toBe('ready')
    expect(drawn.style.backgroundColor).toBe('rgb(29, 78, 216)')
    expect(lines[0].textContent).toBe('This item is at the ready stage')
    expect(lines[1].textContent).toContain('Nothing left to decide')
  })

  it("says only the column's text when the field is hovered beside its chips", () => {
    render(docWith([field('labels', 'tags', 'ready', said('Project tags'))]))
    hover(cell('labels'))
    vi.advanceTimersByTime(400)
    expect(popup().textContent).toBe('Project tags')
  })
})
