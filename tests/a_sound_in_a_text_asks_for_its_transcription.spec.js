import { describe, it, expect, vi, beforeAll, beforeEach } from 'vitest'

// Where the theme's `audio/request` is `transcription`, a sound in a markdown text carries a box
// asking for it to be transcribed, kept in its alt text: `![transcribe](...)` asks, and
// `![transcribed](...)` says it was done.

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

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(async (cmd) => {
    if (cmd === 'load_overseer_audio') return { mime: 'audio/ogg', data: 'T2dnUw==' }
    if (cmd === 'save_overseer_audio') return 'sounds/t/20261124-091500.webm'
    return null
  }),
}))
import { OverseerApp } from '../src/main.js'
import { withSoundAlt } from '../src/renderer.js'

beforeAll(() => {
  let n = 0
  URL.createObjectURL = () => `blob:sound-${++n}`
})

const base = (name, node_type, parameters = {}, children = []) => ({
  name, node_type, parameters, children, is_hierarchy_transparent: false,
  source_id: '', source_fingerprint: 1, param_order: [], authored_dash: false,
})

let written
const draw = (body, { themed = true, mutable = true } = {}) => {
  setupDOM()
  if (themed) document.getElementById('content-display').style.setProperty('--ov-audio-request', 'transcription')
  const app = new OverseerApp()
  window.app = app
  written = []
  app.markDocumentModified = () => {}
  app.reevaluateDocumentSelective = (paths, changes) => { written.push(...changes) }
  const params = { value: { String: body } }
  if (mutable) params.mutable = { Boolean: true }
  app.currentDocument = [base('t', 'tab', {}, [base('body', 'text', params)])]
  app.currentFile = 'notes/t.os'
  app.renderer.renderDocument(app.currentDocument)
  return app
}
const settle = () => new Promise((r) => setTimeout(r, 20))
const boxes = () => Array.from(document.querySelectorAll('.text-content .ov-audio-request input'))
const tick = (input) => input.dispatchEvent(new window.MouseEvent('click', { bubbles: true, cancelable: true }))

describe('a sound in a text asks for its transcription', () => {
  it('rewrites the alt text of one sound of a text and nothing else', () => {
    const text = 'A ![](sounds/a.webm) and `![](sounds/x.webm)` ![old](sounds/b.webm) ![pic](images/p.png)'
    expect(withSoundAlt(text, 1, 'transcribe')).toBe(
      'A ![](sounds/a.webm) and `![](sounds/x.webm)` ![transcribe](sounds/b.webm) ![pic](images/p.png)')
    expect(withSoundAlt(text, 0, 'transcribe', 'sounds/a.webm')).toBe(
      'A ![transcribe](sounds/a.webm) and `![](sounds/x.webm)` ![old](sounds/b.webm) ![pic](images/p.png)')
    expect(withSoundAlt('![transcribe](sounds/a.webm)', 0, '')).toBe('![](sounds/a.webm)')
    expect(withSoundAlt('![a [b] c](sounds/a.webm)', 0, 'transcribe')).toBe('![transcribe](sounds/a.webm)')
    expect(withSoundAlt(text, 2, 'transcribe')).toBeNull()
    expect(withSoundAlt(text, 0, 'transcribe', 'sounds/b.webm')).toBeNull()
  })

  it('draws the box of each sound in a text as its alt text says', () => {
    draw('![](sounds/a.webm) ![transcribe](sounds/b.webm) ![transcribed](sounds/c.webm)')
    const [a, b, c] = boxes()
    expect([a.checked, b.checked, c.checked]).toEqual([false, true, true])
    expect([a.disabled, b.disabled, c.disabled]).toEqual([false, false, true])
    expect(Array.from(document.querySelectorAll('.ov-audio')).map((p) => p.dataset.audioIndex)).toEqual(['0', '1', '2'])
  })

  it('an audio field has no box', () => {
    setupDOM()
    const app = new OverseerApp()
    window.app = app
    app.currentDocument = [base('t', 'tab', {}, [base('memo', 'audio', { value: { String: 'sounds/memo.ogg' } })])]
    app.currentFile = 'notes/t.os'
    app.renderer.renderDocument(app.currentDocument)
    expect(document.querySelector('.ov-audio')).not.toBeNull()
    expect(document.querySelector('.ov-audio-request')).toBeNull()
  })

  it('ticking the box of the second sound writes that one reference as asked for', async () => {
    const app = draw('One ![](sounds/a.webm)\n\nTwo ![](sounds/b.webm)')
    tick(boxes()[1])
    await settle()
    expect(written).toEqual([{ path: 't/body', oldValue: 'One ![](sounds/a.webm)\n\nTwo ![](sounds/b.webm)',
      newValue: 'One ![](sounds/a.webm)\n\nTwo ![transcribe](sounds/b.webm)' }])
    expect(app.currentDocument[0].children[0].parameters.value).toEqual({ String: 'One ![](sounds/a.webm)\n\nTwo ![transcribe](sounds/b.webm)' })
    expect(boxes().map((b) => b.checked)).toEqual([false, true])
    expect(document.querySelector('textarea')).toBeNull()

    tick(boxes()[1])
    await settle()
    expect(written[1].newValue).toBe('One ![](sounds/a.webm)\n\nTwo ![](sounds/b.webm)')
  })

  it('writes nothing where the theme does not ask, in a text that cannot be changed, or once transcribed', async () => {
    draw('![](sounds/a.webm)', { themed: false })
    tick(boxes()[0])
    draw('![](sounds/a.webm)', { mutable: false })
    tick(boxes()[0])
    draw('![transcribed](sounds/a.webm)')
    tick(boxes()[0])
    await settle()
    expect(written).toEqual([])
  })
})

describe('a sound kept in the markdown editor', () => {
  const openEditor = (themed) => {
    const app = draw('See: here', { themed })
    document.querySelector('.text-content').dispatchEvent(new window.MouseEvent('dblclick', { bubbles: true }))
    return { app, textarea: document.querySelector('textarea') }
  }
  const sound = () => new Blob([new Uint8Array([1, 2, 3])], { type: 'audio/webm' })

  it('a recording is asked to be transcribed where the theme asks, and nowhere else', async () => {
    let { app, textarea } = openEditor(true)
    textarea.setSelectionRange(0, 0)
    await app.renderer.keepFileInto(textarea, sound(), { recorded: true })
    expect(textarea.value).toBe('![transcribe](sounds/t/20261124-091500.webm)See: here')

    ;({ app, textarea } = openEditor(false))
    textarea.setSelectionRange(0, 0)
    await app.renderer.keepFileInto(textarea, sound(), { recorded: true })
    expect(textarea.value).toBe('![](sounds/t/20261124-091500.webm)See: here')
  })

  it('a sound dropped in is not asked to be transcribed', async () => {
    const { app, textarea } = openEditor(true)
    textarea.setSelectionRange(0, 0)
    await app.renderer.keepFileInto(textarea, sound())
    expect(textarea.value).toBe('![](sounds/t/20261124-091500.webm)See: here')
  })
})
