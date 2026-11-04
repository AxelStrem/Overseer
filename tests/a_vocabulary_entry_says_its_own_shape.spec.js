import { describe, it, expect, beforeEach, vi } from 'vitest'
import { readFileSync } from 'node:fs'

// A vocabulary entry may say its own `shape`, which wins over the field's: a person's chip, from
// `people.os`, is drawn the same in a column of owners, in a comment's mention and in the drawer.

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
    </div>
    <span id="account-who"></span>
    <input id="account-name" value="Alice">
    <span id="account-preview"></span>
    <div id="account-shape-choices"></div>`
}

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }))
import { OverseerApp } from '../src/main.js'
import { CHIP_SHAPES } from '../src/renderer.js'
import { Account } from '../src/account.js'

const base = (name, node_type) => ({
  name, node_type, parameters: {}, children: [], is_hierarchy_transparent: false,
  source_id: '', source_fingerprint: 1, param_order: [], authored_dash: false,
})

const valued = (name, type, variant, value) => Object.assign(base(name, type), {
  parameters: { value: { [variant]: value } },
})

const person = (tag, shape) => Object.assign(base(`Person__${tag}`, 'div'), {
  children: [
    valued('tag', 'string', 'String', tag),
    valued('name', 'string', 'String', tag),
    valued('shape', 'string', 'String', shape),
  ],
})

const docOf = () => ([Object.assign(base('project', 'tab'), {
  parameters: { mutable: { Boolean: true } },
  children: [
    Object.assign(base('People', 'list'), {
      parameters: { key: { String: 'tag' }, hidden: { Boolean: true } },
      children: [person('alice', 'tag'), person('bob', ''), person('carol', 'star')],
    }),
    Object.assign(valued('owners', 'tags', 'String', 'alice, bob, carol'), {
      parameters: {
        value: { String: 'alice, bob, carol' },
        vocabulary: { String: '/project/People' },
        shape: { String: 'box' },
      },
    }),
  ],
})])

describe('a vocabulary entry with a shape of its own', () => {
  let app
  beforeEach(() => {
    setupDOM()
    app = new OverseerApp()
    app.currentDocument = docOf()
    app._currentText = 'TEXT'
    app.renderer.renderDocument(app.currentDocument)
  })

  const chipOf = (tag) => document.querySelector(`.tags-field .tag-chip[data-tag='${tag}']`)

  it('is drawn in its own shape, in a field that asks for another', () => {
    expect(chipOf('alice').closest('.tag-chips').classList.contains('chip-box')).toBe(true)
    expect(chipOf('alice').classList.contains('chip-shape-tag')).toBe(true)
  })

  it('leaves an entry without one, or with a shape there is not, in the field\'s', () => {
    for (const tag of ['bob', 'carol']) {
      expect(Array.from(chipOf(tag).classList).filter((c) => c.startsWith('chip-shape-'))).toEqual([])
    }
  })

  it('keeps its shape when mentioned in text', () => {
    const field = app.currentDocument[0].children[1]
    expect(app.renderer.inlineChip('alice', field).classList.contains('chip-shape-tag')).toBe(true)
  })

  it('is drawn in the shape a person chose in the drawer, and each shape is offered', () => {
    const account = new Account(app)
    account.person = { username: 'alice', name: 'Alice', shape: 'notch' }
    account.paint()
    expect(document.querySelector('#account-who .tag-chip').classList.contains('chip-shape-notch')).toBe(true)
    const offered = Array.from(document.querySelectorAll('#account-shape-choices .account-shape-choice'))
    expect(offered.map((b) => b.dataset.shape)).toEqual(CHIP_SHAPES)
    expect(offered.filter((b) => b.getAttribute('aria-pressed') === 'true').map((b) => b.dataset.shape)).toEqual(['notch'])
    offered.find((b) => b.dataset.shape === 'wave').click()
    expect(account.shape).toBe('wave')
    expect(document.querySelector('#account-preview .tag-chip').classList.contains('chip-shape-wave')).toBe(true)
  })
})

describe('the shapes in the stylesheet', () => {
  const css = readFileSync('src/styles.css', 'utf8')

  it('give each shape to a field\'s row and to an entry\'s own chip alike', () => {
    for (const shape of CHIP_SHAPES.filter((s) => s !== 'pill')) {
      expect(css).toMatch(new RegExp(`\\.chip-${shape},\\s*\\.tag-chip\\.chip-shape-${shape} \\{`))
    }
  })

  it('put back the row\'s shape on a chip with one of its own before it takes that', () => {
    const reset = css.indexOf('.tag-chip[class*="chip-shape-"] {')
    expect(reset).toBeGreaterThan(-1)
    expect(reset).toBeLessThan(css.indexOf('.tag-chip.chip-shape-box {'))
  })
})
