import { describe, it, expect, vi, beforeEach } from 'vitest'

// An image pasted or dropped into a markdown text's editor is kept beside the document with
// `save_overseer_image`, and the editor gets `![](the path it was kept at)` where the caret is.

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

const saved = []
let refuse = false
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(async (cmd, args) => {
    if (cmd !== 'save_overseer_image') return null
    saved.push(args)
    if (refuse) throw new Error('too large')
    return `images/tracker/20261014-153012${saved.length > 1 ? `-${saved.length}` : ''}.png`
  }),
}))
import { OverseerApp } from '../src/main.js'

const base = (name, node_type, parameters = {}, children = []) => ({
  name, node_type, parameters, children, is_hierarchy_transparent: false,
  source_id: '', source_fingerprint: 1, param_order: [], authored_dash: false,
})

const openEditor = (parameters = {}) => {
  setupDOM()
  const app = new OverseerApp()
  window.app = app
  app.currentFile = 'projects/tracker.os'
  app.currentDocument = [base('t', 'tab', {}, [
    base('body', 'text', { value: { String: 'See: here' }, mutable: { Boolean: true }, ...parameters }),
  ])]
  app.renderer._filters = new Map()
  app.renderer.renderDocument(app.currentDocument)
  const shown = document.querySelector('.text-content')
  shown.dispatchEvent(new window.MouseEvent('dblclick', { bubbles: true }))
  return document.querySelector('textarea')
}

const PNG_BYTES = [0x89, 0x50, 0x4e, 0x47]
const aPng = () => new File([new Uint8Array(PNG_BYTES)], 'shot.png', { type: 'image/png' })

const withFiles = (type, files, key) => {
  const event = new Event(type, { bubbles: true, cancelable: true })
  Object.defineProperty(event, key, { value: { files, items: files.map(() => ({ kind: 'file' })) } })
  return event
}
const paste = (textarea, files) => textarea.dispatchEvent(withFiles('paste', files, 'clipboardData'))
const drop = (textarea, files) => textarea.dispatchEvent(withFiles('drop', files, 'dataTransfer'))
const settle = () => new Promise((r) => setTimeout(r, 20))

beforeEach(() => { saved.length = 0; refuse = false })

describe('a pasted image lands in the text as a reference', () => {
  it('is kept beside the open document and referred to at the caret', async () => {
    const textarea = openEditor()
    expect(textarea.classList.contains('markdown-editor')).toBe(true)
    textarea.setSelectionRange(5, 5)
    expect(paste(textarea, [aPng()])).toBe(false)
    await settle()
    expect(saved).toEqual([{ path: 'projects/tracker.os', mime: 'image/png', data: btoa(String.fromCharCode(...PNG_BYTES)) }])
    expect(textarea.value).toBe('See: ![](images/tracker/20261014-153012.png)here')
    expect(textarea.selectionStart).toBe('See: ![](images/tracker/20261014-153012.png)'.length)
  })

  it('takes the place of what is selected', async () => {
    const textarea = openEditor()
    textarea.setSelectionRange(5, 9)
    paste(textarea, [aPng()])
    await settle()
    expect(textarea.value).toBe('See: ![](images/tracker/20261014-153012.png)')
  })

  it('leaves a paste of anything but an image to the browser', async () => {
    const textarea = openEditor()
    expect(paste(textarea, [new File(['words'], 'a.txt', { type: 'text/plain' })])).toBe(true)
    expect(paste(textarea, [])).toBe(true)
    await settle()
    expect(saved).toEqual([])
    expect(textarea.value).toBe('See: here')
  })

  it('inserts nothing when the image is refused, and says so', async () => {
    refuse = true
    const textarea = openEditor()
    paste(textarea, [aPng()])
    await settle()
    expect(textarea.value).toBe('See: here')
    expect(document.querySelector('.overseer-refusal').textContent).toMatch(/not kept/)
  })

  it('a dropped image is kept the same way', async () => {
    const textarea = openEditor()
    textarea.setSelectionRange(0, 0)
    expect(drop(textarea, [aPng(), aPng()])).toBe(false)
    await settle()
    expect(saved).toHaveLength(2)
    expect(textarea.value).toBe('![](images/tracker/20261014-153012.png)![](images/tracker/20261014-153012-2.png)See: here')
  })

  it('a text that draws no markdown takes no images', async () => {
    const textarea = openEditor({ markdown: { Boolean: false } })
    expect(textarea.classList.contains('markdown-editor')).toBe(false)
    expect(paste(textarea, [aPng()])).toBe(true)
    await settle()
    expect(saved).toEqual([])
  })
})
