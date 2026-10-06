import { describe, it, expect, beforeEach, vi } from 'vitest'

// A string saying `kind=color` holds a colour, written `#rrggbb` - every tag list keeps one per
// entry. It is shown as a colour beside its code, and picked with the browser's own picker rather
// than typed. What is picked is written exactly as a typed code would be; a textbox saying the
// same is a colour input, holding what is picked in the page like anything typed.

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
import { invoke } from '@tauri-apps/api/core'
import { OverseerApp } from '../src/main.js'

const base = (name, node_type, parameters = {}, children = []) => ({
  name, node_type, parameters, children, is_hierarchy_transparent: false,
  source_id: '', source_fingerprint: 1, param_order: [], authored_dash: false,
})

const colour = { kind: { String: 'color' } }

const theDocument = ({ mutable = true } = {}) => [base('p', 'tab', { mutable: { Boolean: mutable } }, [
  base('Label', 'div', {}, [
    base('colour', 'string', { ...colour, value: { String: '#D73A4A' } }),
    base('odd', 'string', { ...colour, value: { String: 'red-ish' } }),
    base('plain', 'string', { value: { String: '#d73a4a' } }),
  ]),
  base('NewTag', 'div', {}, [
    base('colour', 'textbox', { ...colour, value: { String: '#6b7280' } }),
  ]),
])]

let app

const render = (options) => {
  app = new OverseerApp()
  app.currentDocument = theDocument(options)
  app.currentFile = 'p.os'
  app._currentText = 'TEXT'
  app.renderer._filters = new Map()
  app.reevaluateDocumentSelective = vi.fn(async () => ({ success: true }))
  app.renderer.renderDocument(app.currentDocument)
}

const field = (...names) => document.querySelector(`[data-path='${JSON.stringify(['p', ...names])}']`)
const swatch = (...names) => field(...names).querySelector('.colour-swatch')
const picker = (...names) => field(...names).querySelector('input.colour-picker')
const valueOf = (...names) => {
  let node = app.currentDocument[0]
  for (const name of names) node = node.children.find(c => c.name === name)
  return node.parameters.value
}

const pick = async (input, code) => {
  input.value = code
  input.dispatchEvent(new window.Event('change', { bubbles: true }))
  await new Promise(r => setTimeout(r, 0))
}

describe('a colour string', () => {
  beforeEach(() => {
    setupDOM()
    invoke.mockReset()
    invoke.mockImplementation(async () => null)
  })

  it('is shown as the colour it holds, beside its code', () => {
    render()
    expect(swatch('Label', 'colour').style.backgroundColor).not.toBe('')
    expect(swatch('Label', 'colour').classList.contains('colour-unset')).toBe(false)
    expect(picker('Label', 'colour').value).toBe('#d73a4a')
    expect(field('Label', 'colour').querySelector('.field-value').textContent).toBe('#D73A4A')
  })

  it('is written when a colour is picked, as a typed code would be', async () => {
    render()
    await pick(picker('Label', 'colour'), '#1d4ed8')
    expect(valueOf('Label', 'colour')).toEqual({ String: '#1d4ed8' })
    expect(app.reevaluateDocumentSelective).toHaveBeenCalledWith(
      ['p/Label/colour'], [expect.objectContaining({ path: 'p/Label/colour', newValue: '#1d4ed8' })])
    expect(field('Label', 'colour').querySelector('.field-value').textContent).toBe('#1d4ed8')
    expect(field('Label', 'colour').querySelector('input.field-editor'), 'the editor stayed open').toBeNull()
  })

  it('reaches the backend, though its code starts as a markdown heading would', () => {
    // An edit kept to the page as a heading was never written: the save after it sent text
    // that had not seen it, so every colour picked was gone on the next load.
    render()
    expect(app.canHandleAsDOMOnlyUpdate([{ path: 'p/Label/colour', newValue: '#1d4ed8' }])).toBe(false)
  })

  it('cannot be picked where nothing may change', async () => {
    render({ mutable: false })
    await pick(picker('Label', 'colour'), '#1d4ed8')
    expect(valueOf('Label', 'colour')).toEqual({ String: '#D73A4A' })
    expect(app.reevaluateDocumentSelective).not.toHaveBeenCalled()
  })

  it('draws an empty square for a value that is not a colour', () => {
    render()
    expect(swatch('Label', 'odd').classList.contains('colour-unset')).toBe(true)
    expect(swatch('Label', 'odd').style.backgroundColor).toBe('')
    expect(picker('Label', 'odd').value).toBe('#000000')
  })

  it('is only a colour when it says so', () => {
    render()
    expect(swatch('Label', 'plain')).toBeNull()
  })
})

describe('a colour textbox', () => {
  beforeEach(() => {
    setupDOM()
    invoke.mockReset()
    invoke.mockImplementation(async () => null)
  })

  const box = () => field('NewTag', 'colour').querySelector('input.textbox-input')

  it('is a colour input, holding what is picked in the page', () => {
    render()
    expect(box().type).toBe('color')
    expect(box().value).toBe('#6b7280')
    box().value = '#15803d'
    box().dispatchEvent(new window.Event('input', { bubbles: true }))
    expect(app.renderer.typedText()).toEqual([
      { node_path: ['p', 'NewTag', 'colour'], value: { String: '#15803d' } },
    ])
    expect(invoke.mock.calls, 'picking reached the backend').toEqual([])
  })

  it('cannot be picked where nothing may change', () => {
    render({ mutable: false })
    expect(box().disabled).toBe(true)
  })
})
