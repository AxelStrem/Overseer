import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'

// A node that says `on dblclick` runs it on a double click - a double tap on a phone - wherever
// nothing inside it keeps the double tap for itself: a field that can be edited there is edited
// instead. A project tracker's row says it, to put a link to the row on the clipboard through
// `call (name="copy")` with `link`, which the page makes from where it was opened and the
// document's full name.

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
import { runCall } from '../src/calls.js'

const base = (name, node_type, parameters = {}, children = []) => ({
  name, node_type, parameters, children, is_hierarchy_transparent: false,
  source_id: '', source_fingerprint: 1, param_order: [], authored_dash: false,
})

const on = (event) => base(event, 'on', {}, [base('toggle', 'toggle', { path: { String: './x' } })])

const theDocument = () => [base('s', 'tab', { mutable: { Boolean: true } }, [
  base('row', 'div', {}, [
    base('title', 'string', { value: { String: 'Write the parser' } }),
    base('handle', 'string', { value: { String: 'parser' }, mutable: { Boolean: false } }),
    on('dblclick'),
  ]),
  base('both', 'div', {}, [base('x', 'bool', { value: { Boolean: false } }), on('click'), on('dblclick')]),
  base('outer', 'div', {}, [
    base('inner', 'div', {}, [base('y', 'string', { value: { String: 'y' }, mutable: { Boolean: false } }), on('dblclick')]),
    on('dblclick'),
  ]),
])]

let app
let ran

const render = () => {
  app = new OverseerApp()
  app.currentDocument = theDocument()
  app._currentText = 'TEXT'
  ran = []
  vi.spyOn(app.renderer, 'emitEvent').mockImplementation(async (node, _el, eventName) => {
    if (app.renderer.declaresEvent(node, eventName)) ran.push(`${node.name} ${eventName}`)
  })
  app.renderer.renderDocument(app.currentDocument)
}

const at = (...names) => document.querySelector(`[data-path='${JSON.stringify(['s', ...names])}']`)
const valueOf = (...names) => at(...names).querySelector('.field-value')

const click = (element) => element.dispatchEvent(new window.MouseEvent('click', { bubbles: true, cancelable: true }))
const doubleClick = (element) => {
  click(element)
  click(element)
  element.dispatchEvent(new window.MouseEvent('dblclick', { bubbles: true, cancelable: true }))
}

describe('a double click is an event of its own', () => {
  beforeEach(() => {
    setupDOM()
    render()
  })

  it('runs on dblclick on the node', () => {
    doubleClick(at('row'))
    expect(ran).toEqual(['row dblclick'])
  })

  it('runs it where a field inside cannot be edited', () => {
    doubleClick(valueOf('row', 'handle'))
    expect(ran).toEqual(['row dblclick'])
  })

  it('leaves a field that can be edited to its own double tap, which edits it', () => {
    const editing = vi.spyOn(app.renderer, 'makeFieldEditable').mockImplementation(() => {})
    doubleClick(valueOf('row', 'title'))
    expect(ran).toEqual([])
    expect(editing).toHaveBeenCalledTimes(1)
  })

  it('does not draw a node that only answers a double click as pressable', () => {
    expect(at('row').classList.contains('overseer-pressable')).toBe(false)
    expect(at('row').getAttribute('role')).toBe(null)
    click(at('row'))
    expect(ran).toEqual([])
  })

  it('runs a click on each click of a double click, then the double click', () => {
    doubleClick(at('both'))
    expect(ran).toEqual(['both click', 'both click', 'both dblclick'])
  })

  it('is taken by the innermost node that answers it', () => {
    doubleClick(valueOf('outer', 'inner', 'y'))
    expect(ran).toEqual(['inner dblclick'])
  })
})

describe('copy with a link', () => {
  let copied
  beforeEach(() => {
    copied = []
    Object.defineProperty(navigator, 'clipboard', {
      value: { writeText: async (line) => { copied.push(line) } },
      configurable: true,
    })
  })
  afterEach(() => { delete window.__OVERSEER_HOST__ })

  const viewing = (document) => ({ account: { document, documentName: () => 'projects/overseer.os' } })

  it('puts the address of a node of the document, by its full name, on the clipboard', async () => {
    window.__OVERSEER_HOST__ = 'browser'
    const done = await runCall({ name: 'copy', args: { link: 'project/Items/[item-link]' } }, viewing('ap31/projects/overseer.os'))
    expect(done).toEqual({ answer: 'Copied.', failed: false })
    expect(copied).toEqual([window.location.origin + '/doc/ap31/projects/overseer.os#project/Items/[item-link]'])
  })

  it('links the document itself when the address is empty', async () => {
    window.__OVERSEER_HOST__ = 'browser'
    await runCall({ name: 'copy', args: { link: '' } }, viewing('ap31/projects/overseer.os'))
    expect(copied).toEqual([window.location.origin + '/doc/ap31/projects/overseer.os'])
  })

  it('names the document as its address does where the server gave no full name', async () => {
    window.__OVERSEER_HOST__ = 'browser'
    await runCall({ name: 'copy', args: { link: 'a b/[c#1]' } }, viewing(null))
    expect(copied).toEqual([window.location.origin + '/doc/projects/overseer.os#a%20b/[c%231]'])
  })

  it('answers that a link is not available in the desktop app', async () => {
    const done = await runCall({ name: 'copy', args: { link: 'project/Items/[x]' } }, viewing('ap31/p.os'))
    expect(done.failed).toBe(true)
    expect(done.answer).toMatch(/not available here/)
    expect(copied).toEqual([])
  })

  it('says on the status line that it copied', async () => {
    setupDOM()
    render()
    const said = vi.spyOn(app, 'setStatus')
    await app.renderer.runCalls([{ name: 'copy', args: { text: 'hello' }, into: null }])
    expect(said).toHaveBeenCalledWith('Copied.', '', 'info')
  })

  it('still copies text as it did', async () => {
    const done = await runCall({ name: 'copy', args: { text: 'hello' } }, {})
    expect(done).toEqual({ answer: 'Copied.', failed: false })
    expect(copied).toEqual(['hello'])
  })
})
