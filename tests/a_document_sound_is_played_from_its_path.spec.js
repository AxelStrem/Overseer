import { describe, it, expect, vi, beforeAll, beforeEach } from 'vitest'

// `audio memo = "sounds/memo.ogg"` and `![memo](sounds/memo.ogg)` draw a player for a sound the
// document keeps beside it; a sound dropped into a markdown editor is kept with
// `save_overseer_audio` and referred to at the caret, and the editor can record one.

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
const saved = []
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(async (cmd, args) => {
    if (cmd === 'load_overseer_audio') {
      asked.push(args)
      if (args.audio === 'sounds/missing.ogg') throw new Error('no sound')
      return { mime: 'audio/ogg', data: 'T2dnUw==' }
    }
    if (cmd === 'load_overseer_image') return { mime: 'image/png', data: 'iVBORw0KGgo=' }
    if (cmd === 'save_overseer_audio' || cmd === 'save_overseer_image') {
      saved.push({ cmd, ...args })
      return cmd === 'save_overseer_audio' ? 'sounds/t/20261123-091500.webm' : 'images/t/20261123-091500.png'
    }
    return null
  }),
}))
import { OverseerApp } from '../src/main.js'
import { audioBeside, audioTime } from '../src/renderer.js'

beforeAll(() => {
  let n = 0
  URL.createObjectURL = () => `blob:sound-${++n}`
})
beforeEach(() => { asked.length = 0; saved.length = 0 })

const base = (name, node_type, parameters = {}, children = []) => ({
  name, node_type, parameters, children, is_hierarchy_transparent: false,
  source_id: '', source_fingerprint: 1, param_order: [], authored_dash: false,
})

const draw = (children, file = 'notes/t.os') => {
  setupDOM()
  const app = new OverseerApp()
  window.app = app
  app.currentDocument = [base('t', 'tab', {}, children)]
  app.currentFile = file
  app.renderer.renderDocument(app.currentDocument)
  return app
}
const settle = () => new Promise((r) => setTimeout(r, 20))

describe('a document sound is played from its path', () => {
  it('an audio node is drawn as a player of the sound beside the document', async () => {
    draw([base('memo', 'audio', { value: { String: 'sounds/memo.ogg' }, label: { String: 'Voice memo' } })])
    const field = document.querySelector('.audio-field')
    expect(field.querySelector('label').textContent).toBe('Voice memo')
    const player = field.querySelector('.ov-audio')
    expect(player.dataset.audioFile).toBe('sounds/memo.ogg')
    expect(player.getAttribute('aria-label')).toBe('Voice memo')
    expect(player.querySelector('.ov-audio-play')).not.toBeNull()
    await settle()
    expect(asked).toEqual([{ path: 'notes/t.os', audio: 'sounds/memo.ogg' }])
    expect(player.querySelector('audio').getAttribute('src')).toMatch(/^blob:sound-/)
    expect(player.classList.contains('audio-missing')).toBe(false)
  })

  it('a sound that is missing, or a path that leaves the folder, draws a player that cannot play', async () => {
    draw([
      base('gone', 'audio', { value: { String: 'sounds/missing.ogg' } }),
      base('out', 'audio', { value: { String: '../memo.ogg' } }),
    ])
    await settle()
    const [gone, out] = document.querySelectorAll('.ov-audio')
    expect(gone.classList.contains('audio-missing')).toBe(true)
    expect(out.classList.contains('audio-missing')).toBe(true)
    expect(asked.map((a) => a.audio)).toEqual(['sounds/missing.ogg'])
  })

  it('a markdown reference to a sound is a player, and one to a picture is still a picture', async () => {
    draw([base('body', 'text', { value: { String: 'Listen: ![memo](sounds/memo.ogg) ![plate](images/plate.png)' } })])
    await settle()
    const text = document.querySelector('.text-content')
    const player = text.querySelector('.ov-audio')
    expect(player.dataset.audioFile).toBe('sounds/memo.ogg')
    expect(player.querySelector('audio').getAttribute('src')).toMatch(/^blob:sound-/)
    expect(text.querySelector('img[data-image-file="images/plate.png"]')).not.toBeNull()
  })

  it('a sound from elsewhere is a link to it, not a player', () => {
    draw([base('body', 'text', { value: { String: '![song](https://example.org/song.mp3)' } })])
    const text = document.querySelector('.text-content')
    expect(text.querySelector('.ov-audio')).toBeNull()
    const link = text.querySelector('a')
    expect(link.getAttribute('href')).toBe('https://example.org/song.mp3')
    expect(link.getAttribute('rel')).toBe('noreferrer')
  })

  it('a press on a player in a text does not open the text for editing', async () => {
    draw([base('body', 'text', { value: { String: '![memo](sounds/memo.ogg)' }, mutable: { Boolean: true } })])
    await settle()
    const audio = document.querySelector('.ov-audio audio')
    audio.play = vi.fn(() => Promise.resolve())
    const button = document.querySelector('.ov-audio-play')
    button.dispatchEvent(new window.MouseEvent('dblclick', { bubbles: true }))
    button.dispatchEvent(new window.MouseEvent('click', { bubbles: true }))
    expect(document.querySelector('textarea')).toBeNull()
    expect(audio.play).toHaveBeenCalled()
  })

  it('only a path of a sound beside the document is asked for, and times read as a clock', () => {
    expect(audioBeside('sounds/memo.webm')).toBe(true)
    expect(audioBeside('sounds/Song.MP3')).toBe(true)
    for (const p of ['../memo.ogg', '/memo.ogg', 'C:/memo.ogg', 'sounds/memo.png', 'sounds//memo.ogg', '']) {
      expect(audioBeside(p)).toBe(false)
    }
    expect(audioTime(0)).toBe('0:00')
    expect(audioTime(65.4)).toBe('1:05')
    expect(audioTime(3725)).toBe('1:02:05')
    expect(audioTime(NaN)).toBe('0:00')
  })
})

describe('a sound in the markdown editor', () => {
  const openEditor = () => {
    draw([base('body', 'text', { value: { String: 'See: here' }, mutable: { Boolean: true } })])
    document.querySelector('.text-content').dispatchEvent(new window.MouseEvent('dblclick', { bubbles: true }))
    return document.querySelector('textarea')
  }
  const drop = (textarea, files) => {
    const event = new Event('drop', { bubbles: true, cancelable: true })
    Object.defineProperty(event, 'dataTransfer', { value: { files, items: files.map(() => ({ kind: 'file' })) } })
    return textarea.dispatchEvent(event)
  }

  it('a dropped sound is kept beside the document and referred to at the caret', async () => {
    const textarea = openEditor()
    textarea.setSelectionRange(5, 5)
    const sound = new File([new Uint8Array([0x4f, 0x67, 0x67, 0x53])], 'memo.webm', { type: 'audio/webm' })
    const picture = new File([new Uint8Array([0x89, 0x50])], 'shot.png', { type: 'image/png' })
    expect(drop(textarea, [sound, picture])).toBe(false)
    await settle()
    expect(saved.map((s) => [s.cmd, s.mime])).toEqual([['save_overseer_audio', 'audio/webm'], ['save_overseer_image', 'image/png']])
    expect(saved[0].data).toBe('T2dnUw==')
    expect(textarea.value).toBe('See: ![](sounds/t/20261123-091500.webm)![](images/t/20261123-091500.png)here')
  })

  it('has no record button where the page cannot record', () => {
    const had = window.MediaRecorder
    delete window.MediaRecorder
    openEditor()
    expect(document.querySelector('.markdown-toolbar .record-btn')).toBeNull()
    if (had) window.MediaRecorder = had
  })

  it('records from the microphone, lets it go, and keeps the recording', async () => {
    const stopped = []
    const track = { stop: vi.fn(() => stopped.push('track')) }
    const stream = { getTracks: () => [track] }
    Object.defineProperty(window.navigator, 'mediaDevices', {
      configurable: true, value: { getUserMedia: vi.fn(async () => stream) },
    })
    class FakeRecorder {
      static isTypeSupported(t) { return t === 'audio/ogg;codecs=opus' }
      constructor(s, options) { this.mimeType = options.mimeType; this.state = 'inactive'; this.listeners = {} }
      addEventListener(type, fn) { this.listeners[type] = fn }
      start() { this.state = 'recording' }
      stop() {
        this.state = 'inactive'
        this.listeners.dataavailable({ data: new Blob([new Uint8Array([1, 2, 3])], { type: this.mimeType }) })
        this.listeners.stop()
      }
    }
    window.MediaRecorder = FakeRecorder
    try {
      const textarea = openEditor()
      textarea.setSelectionRange(0, 0)
      const record = document.querySelector('.markdown-toolbar .record-btn')
      expect(record.textContent).toBe('Record')
      record.click()
      await settle()
      expect(record.classList.contains('recording')).toBe(true)
      expect(record.textContent).toMatch(/^Stop \d:\d\d$/)
      record.click()
      await settle()
      expect(stopped).toEqual(['track'])
      expect(record.textContent).toBe('Record')
      expect(saved).toEqual([{ cmd: 'save_overseer_audio', path: 'notes/t.os', mime: 'audio/ogg;codecs=opus', data: 'AQID' }])
      expect(textarea.value).toBe('![](sounds/t/20261123-091500.webm)See: here')

      // Closed while it records, the editor drops the recording and still lets the microphone go.
      record.click()
      await settle()
      document.querySelector('.markdown-toolbar .cancel-btn').click()
      await settle()
      expect(stopped).toEqual(['track', 'track'])
      expect(saved).toHaveLength(1)
    } finally {
      delete window.MediaRecorder
      delete window.navigator.mediaDevices
    }
  })
})
