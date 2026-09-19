import { describe, it, expect, beforeEach, vi, afterEach } from 'vitest'

// A change reaches the disk without anybody pressing Save.
//
// This is the half that was missing when the feature was first built: the writing was put into
// the server's command dispatcher, and the desktop app never goes through it. In the app,
// pressing a button changed nothing on disk, reloading threw the change away, the asterisk never
// cleared, and Undo answered "Command undo_overseer_file not found" - which is to say it behaved
// exactly as it had before.
//
// So the saving lives in the page now, which is one implementation for both hosts. What these
// check is the page's side of it: that a change asks to be written, that it waits rather than
// writing per keystroke, that a refusal is survivable, and that the asterisk goes when the write
// lands. What they cannot check is whether a backend answers - that is
// `both_hosts_answer_what_the_page_asks`, which reads both command lists and compares them.

function setupDOM() {
  document.body.innerHTML = `
    <div id="app">
      <div id="toolbar"><button id="open-file-btn"></button><button id="new-file-btn"></button>
        <button id="undo-file-btn"></button></div>
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
import { invoke } from '@tauri-apps/api/core'
import { OverseerApp } from '../src/main.js'

/** An app with a document open, and nothing real behind it. */
const anApp = () => {
  const app = new OverseerApp()
  app.currentFile = '/documents/day.os'
  app._currentText = 'tab day (label="Day", mutable=true) {\n    int n = 1\n}\n'
  app._originalText = app._currentText
  app.currentDocument = []
  return app
}

const wrote = () => invoke.mock.calls.filter(([cmd]) => cmd.startsWith('save_overseer_file'))

describe('a change writes itself', () => {
  beforeEach(() => {
    setupDOM()
    invoke.mockReset()
    invoke.mockResolvedValue(null)
    vi.useFakeTimers()
  })
  afterEach(() => { vi.useRealTimers() })

  it('asks to be written when the document changes', async () => {
    const app = anApp()
    app.markDocumentModified()
    expect(wrote(), 'it wrote before waiting at all').toHaveLength(0)
    await vi.runAllTimersAsync()
    expect(wrote().length, 'the change never reached a save').toBeGreaterThan(0)
  })

  it('waits, so typing a word is one write and not eight', async () => {
    // Every keystroke in a field marks the document changed. A write each would be absurd, and
    // on a large document would be slower than the typing.
    const app = anApp()
    for (let i = 0; i < 8; i += 1) {
      app.markDocumentModified()
      await vi.advanceTimersByTimeAsync(50)
    }
    await vi.runAllTimersAsync()
    expect(wrote()).toHaveLength(1)
  })

  it('writes a press at once instead of waiting out the typing debounce', async () => {
    // A press is a single act, not a stream, and it should be its own step to take back.
    const app = anApp()
    app.markDocumentModified(true)
    await vi.advanceTimersByTimeAsync(10)
    expect(wrote(), 'the press waited as though it were typing').toHaveLength(1)
  })

  it('keeps a press and whatever is typed after it as separate steps', async () => {
    // What made undo look as though it took back two changes at once: the press sat in the
    // same 400 ms window as the next keystroke, so one write held both and one step undid both.
    const app = anApp()
    app.markDocumentModified(true)
    await vi.advanceTimersByTimeAsync(10)
    app.markDocumentModified()
    await vi.runAllTimersAsync()
    expect(wrote(), 'the press and the typing after it became one step').toHaveLength(2)
  })

  it('does not write a document that has no file behind it', async () => {
    const app = anApp()
    app.currentFile = null
    app.markDocumentModified()
    await vi.runAllTimersAsync()
    expect(wrote()).toHaveLength(0)
  })

  it('survives a refusal rather than leaving the page broken', async () => {
    // The backend refuses a save built on a document somebody else has written since. The page
    // has to carry on - the next change asks again.
    const app = anApp()
    invoke.mockRejectedValue(new Error('day.os has changed since this page loaded it'))
    app.markDocumentModified()
    await expect(vi.runAllTimersAsync()).resolves.not.toThrow()
  })

  it('stops saying there is something unsaved once there is not', async () => {
    const app = anApp()
    app.markDocumentModified()
    expect(app.isDocumentModified).toBe(true)
    await vi.runAllTimersAsync()
    expect(app.isDocumentModified, 'the asterisk would still be showing').toBe(false)
  })
})

describe('taking the last change back', () => {
  beforeEach(() => {
    setupDOM()
    invoke.mockReset()
    invoke.mockResolvedValue(null)
  })

  it('asks the backend rather than keeping a stack of its own', async () => {
    // What is taken back is the last write to the document, whoever made it - a press here or
    // the bot recording a meal. A stack held in the page could not know about the second.
    const app = anApp()
    invoke.mockImplementation((cmd) =>
      cmd === 'undo_overseer_file' ? Promise.resolve({ steps_left: 2 }) : Promise.resolve(null))
    await app.undoLastChange()
    expect(invoke.mock.calls.map(([cmd]) => cmd)).toContain('undo_overseer_file')
  })

  it('says how much further back it can go', async () => {
    const app = anApp()
    invoke.mockImplementation((cmd) =>
      cmd === 'undo_overseer_file' ? Promise.resolve({ steps_left: 2 }) : Promise.resolve(null))
    await app.undoLastChange()
    expect(document.getElementById('status-message').textContent).toContain('2')
  })

  it('treats nothing left to take back as news rather than as a fault', async () => {
    const app = anApp()
    invoke.mockRejectedValue(new Error("there is nothing to take back for 'day.os'"))
    await app.undoLastChange()
    const said = document.getElementById('status-message').textContent
    expect(said.toLowerCase()).toContain('nothing left')
  })

  it('says nothing is open rather than asking about a document that is not there', async () => {
    const app = anApp()
    app.currentFile = null
    await app.undoLastChange()
    expect(invoke.mock.calls.map(([cmd]) => cmd)).not.toContain('undo_overseer_file')
  })
})

describe('undo keeps what the viewer was looking at', () => {
  // A field the document marks `mutable="guarded"` belongs to whoever is looking, not to the
  // document, so it never reaches the file - and reading the file back cannot recover it. Undo
  // reads the file back, which is how one undo came to look as though it reverted several
  // changes: it took back the one write there was, and the reload put every guarded field on
  // the page back to what the document authored.
  beforeEach(() => {
    setupDOM()
    invoke.mockReset()
    invoke.mockImplementation((cmd) =>
      cmd === 'undo_overseer_file' ? Promise.resolve({ steps_left: 0 }) : Promise.resolve(null))
  })

  /// A document with one field the viewer has moved and one the document owns.
  const anAppLookingAtSomething = () => {
    const app = anApp()
    app.renderer = { renderDocument() {} }
    app.currentDocument = [{ name: 'day', parameters: {}, children: [
      { name: 'showing', parameters: {
        value: { Integer: 5 },
        mutable: { String: 'guarded' },
        _guarded_edit: { Boolean: true },
        _guarded_original_value: { Integer: 1 }
      } },
      { name: 'recorded', parameters: { value: { Integer: 3 } } }
    ] }]
    // The reload reads the file, where the guarded field says what the document authored and
    // the real field says what it said before the write being taken back.
    app.loadFile = async () => {
      app.currentDocument = [{ name: 'day', parameters: {}, children: [
        { name: 'showing', parameters: { value: { Integer: 1 }, mutable: { String: 'guarded' } } },
        { name: 'recorded', parameters: { value: { Integer: 2 } } }
      ] }]
    }
    return app
  }

  it("leaves the viewer's own field where they put it", async () => {
    const app = anAppLookingAtSomething()
    await app.undoLastChange()
    const [showing] = app.currentDocument[0].children
    expect(showing.parameters.value, 'undo reverted a field that was never written').toEqual({ Integer: 5 })
  })

  it('still takes back the write it was asked to', async () => {
    const app = anAppLookingAtSomething()
    await app.undoLastChange()
    const recorded = app.currentDocument[0].children[1]
    expect(recorded.parameters.value).toEqual({ Integer: 2 })
  })

  it('keeps knowing what the document authored, so the next save still reverts it', async () => {
    // Without this the restored value would be written as though the document had said it.
    const app = anAppLookingAtSomething()
    await app.undoLastChange()
    const [showing] = app.currentDocument[0].children
    expect(showing.parameters._guarded_original_value).toEqual({ Integer: 1 })
    expect(app.collectGuardedReverts(app.currentDocument))
      .toEqual([{ path: 'day/showing', value: { Integer: 1 } }])
  })

  it('touches nothing when the viewer has moved nothing', async () => {
    const app = anAppLookingAtSomething()
    app.currentDocument[0].children[0].parameters._guarded_edit = { Boolean: false }
    await app.undoLastChange()
    const [showing] = app.currentDocument[0].children
    expect(showing.parameters.value, 'a field nobody had moved was carried across').toEqual({ Integer: 1 })
  })
})
