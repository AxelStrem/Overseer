import { describe, it, expect, beforeEach, vi } from 'vitest'

// An instance or a list whose template could not be had says why where it is drawn.
//
// The resolver leaves the reason in `_inherit_error` and the node as it was written; the page
// draws the reason at the top of a container, under its fold header, and just before a field.

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

const why = (text) => ({ _inherit_error: { String: text } })

const theDocument = () => [base('t', 'tab', {}, [
  base('Broken', 'div', why("'base.os' not found"), [
    base('note', 'string', { value: { String: 'as written' } }),
  ]),
  base('Folded', 'div', { ...why("'Thing' is not in 'base.os'"), foldable: { Boolean: true }, label: { String: 'Folded' } }, [
    base('note', 'string', { value: { String: 'under the header' } }),
  ]),
  base('Items', 'list', why("'base.os' could not be read"), [
    base('Item__1', 'div', {}, [base('name', 'string', { value: { String: 'one' } })]),
  ]),
  base('Named', 'Thing', why("'base.os' not found"), [
    base('note', 'string', { value: { String: 'kind is the template name' } }),
  ]),
  base('Lone', 'string', { ...why("'base.os' not found"), value: { String: 'a field' } }),
  base('Fine', 'div', {}, [base('note', 'string', { value: { String: 'fine' } })]),
])]

const drawn = (...path) => document.querySelector(`[data-path='${JSON.stringify(['t', ...path])}']`)

describe('a template that cannot be had', () => {
  beforeEach(() => {
    setupDOM()
    const app = new OverseerApp()
    app.currentDocument = theDocument()
    app.currentFile = 't.os'
    app._currentText = 'TEXT'
    app.renderer._filters = new Map()
    app.renderer.renderDocument(app.currentDocument)
  })

  it('says why at the top of the instance, which is drawn as written', () => {
    const div = drawn('Broken')
    expect(div.firstElementChild.className).toBe('inherit-error')
    expect(div.firstElementChild.textContent).toBe("'base.os' not found")
    expect(div.textContent).toContain('as written')
  })

  it('says why inside an instance whose kind is the missing template', () => {
    expect(drawn('Named').firstElementChild.className).toBe('inherit-error')
  })

  it('says why under a fold header, which stays first', () => {
    const div = drawn('Folded')
    expect(div.firstElementChild.classList.contains('overseer-fold-header')).toBe(true)
    expect(div.children[1].className).toBe('inherit-error')
  })

  it('says why at the top of a list, above its entries', () => {
    const list = drawn('Items')
    const line = list.querySelector('.inherit-error')
    expect(line?.textContent).toBe("'base.os' could not be read")
    const entry = drawn('Items', 'Item__1')
    expect(line.compareDocumentPosition(entry) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy()
  })

  it('says why just before a field', () => {
    const field = drawn('Lone')
    expect(field.previousElementSibling?.className).toBe('inherit-error')
  })

  it('says nothing where the template was had', () => {
    expect(drawn('Fine').querySelector('.inherit-error')).toBeNull()
    expect(document.querySelectorAll('.inherit-error').length).toBe(5)
  })
})
