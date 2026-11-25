import { describe, it, expect, vi, beforeAll, beforeEach, afterEach } from 'vitest'

// A recording made with the markdown editor's Record button exists nowhere else: whatever becomes
// of the save or of the editor, it reaches the text, or the page says why not and keeps it to be
// sent again or downloaded.

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

const saves = []
let refuse = null
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(async (cmd, args) => {
    if (cmd === 'save_overseer_audio') {
      saves.push(args.args || args)
      if (refuse) throw new Error(refuse)
      return `sounds/t/take-${saves.length}.webm`
    }
    return null
  }),
}))
import { OverseerApp } from '../src/main.js'

let recorders = []
class FakeRecorder extends EventTarget {
  constructor(stream, options) {
    super()
    this.options = options
    this.mimeType = 'audio/webm;codecs=opus'
    this.state = 'inactive'
    recorders.push(this)
  }
  static isTypeSupported() { return true }
  start(slice) { this.slice = slice; this.state = 'recording' }
  emit(data) {
    const event = new Event('dataavailable')
    event.data = data
    this.dispatchEvent(event)
  }
  stop() {
    if (this.state === 'inactive') return
    this.state = 'inactive'
    this.emit(new Blob([new Uint8Array([1, 2, 3])], { type: this.mimeType }))
    this.dispatchEvent(new Event('stop'))
  }
}

beforeAll(() => {
  let n = 0
  URL.createObjectURL = () => `blob:recording-${++n}`
  URL.revokeObjectURL = () => {}
})

beforeEach(() => {
  saves.length = 0
  refuse = null
  recorders = []
  window.MediaRecorder = FakeRecorder
  globalThis.MediaRecorder = FakeRecorder
  Object.defineProperty(navigator, 'mediaDevices', {
    configurable: true,
    value: { getUserMedia: async () => ({ getTracks: () => [{ stop() {} }] }) },
  })
})

afterEach(() => {
  delete window.MediaRecorder
  delete globalThis.MediaRecorder
})

const base = (name, node_type, parameters = {}, children = []) => ({
  name, node_type, parameters, children, is_hierarchy_transparent: false,
  source_id: '', source_fingerprint: 1, param_order: [], authored_dash: false,
})

let written
const openEditor = () => {
  setupDOM()
  const app = new OverseerApp()
  window.app = app
  written = []
  app.markDocumentModified = () => {}
  app.reevaluateDocumentSelective = (paths, changes) => { written.push(...changes) }
  app.currentDocument = [base('t', 'tab', {}, [base('body', 'text', { value: { String: 'See: here' }, mutable: { Boolean: true } })])]
  app.currentFile = 'notes/t.os'
  app.renderer.renderDocument(app.currentDocument)
  document.querySelector('.text-content').dispatchEvent(new window.MouseEvent('dblclick', { bubbles: true }))
  const textarea = document.querySelector('textarea')
  textarea.setSelectionRange(0, 0)
  return { app, textarea }
}
const settle = (ms = 20) => new Promise((r) => setTimeout(r, ms))
const press = (el) => el.dispatchEvent(new window.MouseEvent('click', { bubbles: true, cancelable: true }))
const record = async () => {
  press(document.querySelector('.record-btn'))
  await settle()
  return recorders[recorders.length - 1]
}
const note = () => document.querySelector('.overseer-recording-note')
const control = (text) => Array.from(note().querySelectorAll('button, a')).find((c) => c.textContent === text)

describe('a recording from the markdown editor', () => {
  it('is recorded at a voice rate, in slices', async () => {
    openEditor()
    const recorder = await record()
    expect(recorder.options.audioBitsPerSecond).toBe(32000)
    expect(recorder.slice).toBe(1000)
    recorder.stop()
    await settle()
  })

  it('refused, says why and how large, and can be sent again or downloaded', async () => {
    const { textarea } = openEditor()
    refuse = 'it is too large for the server to take'
    press(document.querySelector('.record-btn'))
    await settle()
    press(document.querySelector('.record-btn'))
    await settle()
    expect(note().textContent).toContain('The sound was not kept (0.0 MB): it is too large for the server to take')
    expect(control('Download').getAttribute('href')).toMatch(/^blob:recording-/)
    expect(control('Download').getAttribute('download')).toMatch(/\.webm$/)
    expect(textarea.value).toBe('See: here')

    refuse = null
    press(control('Try again'))
    await settle()
    expect(saves).toHaveLength(2)
    expect(saves[1].data).toBe(saves[0].data)
    expect(saves[1].path).toBe('notes/t.os')
    expect(textarea.value).toBe('![](sounds/t/take-2.webm)See: here')
    expect(note()).toBeNull()
  })

  it('is kept when Save is pressed while recording, and the text saved with it', async () => {
    openEditor()
    await record()
    press(document.querySelector('.markdown-editor-container .save-btn') ||
      Array.from(document.querySelectorAll('.markdown-editor-container button')).find((b) => b.textContent === 'Save'))
    await settle()
    expect(saves).toHaveLength(1)
    expect(written.map((c) => c.newValue)).toEqual(['![](sounds/t/take-1.webm)See: here'])
    expect(document.querySelector('textarea')).toBeNull()
  })

  it('is dropped with a word when the edit is cancelled', async () => {
    openEditor()
    await record()
    press(Array.from(document.querySelectorAll('.markdown-editor-container button')).find((b) => b.textContent === 'Cancel'))
    await settle()
    expect(saves).toHaveLength(0)
    expect(document.getElementById('status-message').textContent).toBe('The recording was dropped with the edit')
  })

  it('stops by itself as it nears the most a sound may weigh, and is kept', async () => {
    const { textarea } = openEditor()
    const recorder = await record()
    recorder.emit(new Blob([new Uint8Array(16 * 1024 * 1024 - 512 * 1024)], { type: recorder.mimeType }))
    await settle()
    expect(recorder.state).toBe('inactive')
    expect(saves).toHaveLength(1)
    expect(textarea.value).toBe('![](sounds/t/take-1.webm)See: here')
    expect(document.querySelector('.overseer-refusal').textContent).toBe('The recording reached the longest a sound may be, and was stopped and kept')
  })

  it('taken off the page by a drawing, is stopped and kept in the editor opened again on its field', async () => {
    const { app, textarea } = openEditor()
    await record()
    const field = textarea.dataset.draftOf
    expect(field).toBeTruthy()
    textarea.closest('.markdown-editor-container').remove()
    const reopened = document.createElement('textarea')
    reopened.dataset.draftOf = field
    reopened.value = 'See: here'
    reopened.setSelectionRange(0, 0)
    document.getElementById('content-display').appendChild(reopened)
    app.renderer._draftEditor = reopened
    await settle(300)
    expect(saves).toHaveLength(1)
    expect(reopened.value).toBe('![](sounds/t/take-1.webm)See: here')
  })

  it('kept with no editor left for it, shows its reference to be put in by hand', async () => {
    const { textarea } = openEditor()
    await record()
    textarea.closest('.markdown-editor-container').remove()
    await settle(300)
    expect(saves).toHaveLength(1)
    expect(note().textContent).toContain('put ![](sounds/t/take-1.webm) into the text where it belongs')
    press(control('Dismiss'))
    expect(note()).toBeNull()
  })
})
