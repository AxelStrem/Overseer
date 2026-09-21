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

describe('a change goes up as an instruction, not as a document', () => {
  // The page used to send the whole document with every change and have that text written over
  // the file, so anything that had written in between was gone without a word. It names what
  // changed instead, and the backend applies that to whatever the file says at the time.
  beforeEach(() => {
    setupDOM()
    invoke.mockReset()
    invoke.mockResolvedValue(null)
  })

  const withAField = () => {
    const app = anApp()
    app.currentDocument = [{
      name: 'day', node_type: 'tab', parameters: {}, children: [
        { name: 'n', node_type: 'int', parameters: { value: { Integer: 1 } }, children: [] }
      ]
    }]
    return app
  }

  const callsTo = (name) => invoke.mock.calls.filter(([cmd]) => cmd === name)

  it('names the field and the value rather than sending the document', async () => {
    const app = withAField()
    invoke.mockImplementation((cmd) =>
      cmd === 'write_overseer_values'
        ? Promise.resolve({ text: 'TEXT', changes: [], nodes: null })
        : Promise.resolve(null))

    await app.reevaluateDocumentSelective(['day/n'], [{ path: 'day/n', oldValue: 1, newValue: 2 }])

    const sent = callsTo('write_overseer_values')
    expect(sent, 'the edit did not go as an instruction').toHaveLength(1)
    expect(sent[0][1].values).toHaveLength(1)
    expect(sent[0][1].values[0].node_path).toEqual(['day', 'n'])
    expect(sent[0][1].values[0].value).toEqual({ Integer: 2 })
  })

  it('carries several values as one write rather than one write each', async () => {
    // A change that rewrites two fields together is one change. Written one after another it
    // would be two file writes and two steps to take back for something typed once.
    const app = withAField()
    app.currentDocument[0].children.push(
      { name: 'm', node_type: 'int', parameters: { value: { Integer: 5 } }, children: [] })
    invoke.mockImplementation((cmd) =>
      cmd === 'write_overseer_values'
        ? Promise.resolve({ text: 'TEXT', changes: [], nodes: null })
        : Promise.resolve(null))

    await app.reevaluateDocumentSelective(
      ['day/n', 'day/m'],
      [
        { path: 'day/n', oldValue: 1, newValue: 2 },
        { path: 'day/m', oldValue: 5, newValue: 6 },
      ])

    const sent = callsTo('write_overseer_values')
    expect(sent, 'the change was split into a write each').toHaveLength(1)
    expect(sent[0][1].values.map((v) => v.node_path)).toEqual([['day', 'n'], ['day', 'm']])
    expect(sent[0][1].values.map((v) => v.value)).toEqual([{ Integer: 2 }, { Integer: 6 }])
    expect(callsTo('parse_overseer_content_selective_update'), 'the document went up as well')
      .toHaveLength(0)
  })

  it('still sends the document when the change names no value at all', async () => {
    // Not every call is a write. Some say only that something changed and the derived values
    // want working out again, and those have nothing to name.
    const app = withAField()
    await app.reevaluateDocumentSelective(['day/n'])
    expect(callsTo('write_overseer_values')).toHaveLength(0)
  })

  it('sends no document text with it', async () => {
    // The point of the change. A document measured about 5.5 seconds to send on the desktop,
    // and sending it is what made the write able to overwrite somebody else's.
    const app = withAField()
    invoke.mockImplementation((cmd) =>
      cmd === 'write_overseer_values'
        ? Promise.resolve({ text: 'TEXT', changes: [], nodes: null })
        : Promise.resolve(null))

    await app.reevaluateDocumentSelective(['day/n'], [{ path: 'day/n', oldValue: 1, newValue: 2 }])

    const [, args] = callsTo('write_overseer_values')[0]
    expect(Object.keys(args)).not.toContain('content')
    expect(callsTo('parse_overseer_content_selective_update'), 'the document went up as well')
      .toHaveLength(0)
  })

  it('does not then save the whole document to say the same thing', async () => {
    // The write happened as part of applying it. A save afterwards would send the document up
    // to repeat it, and would be a second undo step for one change.
    vi.useFakeTimers()
    try {
      const app = withAField()
      invoke.mockImplementation((cmd) =>
        cmd === 'write_overseer_values'
          ? Promise.resolve({ text: 'TEXT', changes: [], nodes: null })
          : Promise.resolve(null))

      app.markDocumentModified()
      await app.reevaluateDocumentSelective(['day/n'], [{ path: 'day/n', oldValue: 1, newValue: 2 }])
      await vi.runAllTimersAsync()

      expect(wrote(), 'the change was written twice').toHaveLength(0)
      expect(app.isDocumentModified, 'it still thinks something is unsaved').toBe(false)
    } finally {
      vi.useRealTimers()
    }
  })

  it('still sends the document when there is no file to name', async () => {
    // A document being edited before it has been saved anywhere has no address to write to, so
    // the old path is still what answers for it.
    const app = withAField()
    app.currentFile = null
    await app.reevaluateDocumentSelective(['day/n'], [{ path: 'day/n', oldValue: 1, newValue: 2 }])
    expect(callsTo('write_overseer_values')).toHaveLength(0)
  })
})

describe('keeping up with what the file says', () => {
  // The page holds the text the file had when it last heard, and a save built on a document
  // something else has written since is refused rather than allowed to overwrite it. A write
  // made by naming a change moves the file without the page sending anything - so unless the
  // page is told, that baseline is stale from the first change onwards and every later save is
  // refused. Which is exactly what happened in use: a third edit in a row, or a second tag, and
  // the page insisted the document had changed since it was opened.
  beforeEach(() => {
    setupDOM()
    invoke.mockReset()
    invoke.mockResolvedValue(null)
  })

  it('takes on what the answer says the file now holds', () => {
    const app = anApp()
    app._originalText = 'STALE'
    app.alreadyWritten({ wrote: true, text: 'WHAT THE PAGE SEES' })
    expect(app._originalText).toBe('WHAT THE PAGE SEES')
  })

  it('prefers the file text when the file and the page differ', () => {
    // They differ when a guarded field is in play: the page is shown the day it moved to, and
    // the file keeps what the document authored. The baseline has to be the file's version.
    const app = anApp()
    app.alreadyWritten({
      wrote: true,
      text: 'WITH THE VIEWERS DAY',
      file_text: 'WHAT THE DOCUMENT AUTHORED'
    })
    expect(app._originalText).toBe('WHAT THE DOCUMENT AUTHORED')
  })

  it('leaves the baseline alone when nothing was written', () => {
    // A press that moved only the viewer. The file is untouched, so what the page already holds
    // is still right - and adopting the text it was shown would make the next save disagree
    // with the file for no reason.
    const app = anApp()
    app._originalText = 'WHAT THE FILE SAYS'
    app.alreadyWritten({ wrote: false, text: 'WITH THE VIEWERS DAY' })
    expect(app._originalText).toBe('WHAT THE FILE SAYS')
  })

  it('survives an answer that says nothing about it', () => {
    const app = anApp()
    app._originalText = 'WHAT THE FILE SAYS'
    expect(() => app.alreadyWritten(undefined)).not.toThrow()
    expect(app._originalText).toBe('WHAT THE FILE SAYS')
  })

  it('stops a following save from being built on a baseline the page itself moved', async () => {
    // The whole point, end to end: a change goes as an instruction, and then something that
    // still saves text - a cascade, a list button - does so against what the file now says.
    vi.useFakeTimers()
    try {
      const app = anApp()
      app.currentDocument = [{
        name: 'day', node_type: 'tab', parameters: {}, children: [
          { name: 'n', node_type: 'int', parameters: { value: { Integer: 1 } }, children: [] }
        ]
      }]
      invoke.mockImplementation((cmd) =>
        cmd === 'write_overseer_values'
          ? Promise.resolve({ wrote: true, text: 'AFTER THE WRITE', changes: [], nodes: null })
          : Promise.resolve(null))

      await app.reevaluateDocumentSelective(['day/n'], [{ path: 'day/n', oldValue: 1, newValue: 2 }])
      expect(app._originalText, 'the page kept a baseline the file no longer has')
        .toBe('AFTER THE WRITE')

      // Now a save that does send text. It must say it is working from the current file.
      app._currentText = 'AFTER THE WRITE'
      await app.saveFile()
      const sent = invoke.mock.calls.filter(([cmd]) => cmd === 'save_overseer_file_from_text')
      expect(sent).toHaveLength(1)
      expect(sent[0][1].original).toBe('AFTER THE WRITE')
    } finally {
      vi.useRealTimers()
    }
  })
})

describe('a write in flight is not overtaken by a save', () => {
  // The field handler marks the document changed - which schedules a save of the whole text -
  // and only then asks for the change to be written. On a small document the write answers well
  // inside the wait and calls the save off. On a large one it does not: the save fires while the
  // write is still in flight, sends the text as it was *before* the edit, and whichever lands
  // second wins. That is why an edit to a big document appeared to apply, took a second or two
  // doing it, and was gone after a refresh - while the same edit to a small one stuck, and while
  // presses were fine throughout, a press scheduling no save at all.
  beforeEach(() => {
    setupDOM()
    invoke.mockReset()
    invoke.mockResolvedValue(null)
    vi.useFakeTimers()
  })
  afterEach(() => { vi.useRealTimers() })

  const anAppWithAField = () => {
    const app = anApp()
    app.currentDocument = [{
      name: 'day', node_type: 'tab', parameters: {}, children: [
        { name: 'n', node_type: 'int', parameters: { value: { Integer: 1 } }, children: [] }
      ]
    }]
    return app
  }

  /// A backend that takes its time, as a large document does.
  const slowToAnswer = (ms) => {
    invoke.mockImplementation((cmd) => {
      if (cmd === 'write_overseer_values') {
        return new Promise((resolve) => setTimeout(
          () => resolve({ wrote: true, text: 'AFTER THE WRITE', changes: [], nodes: null }), ms))
      }
      return Promise.resolve(null)
    })
  }

  it('does not send the pre-edit text while the write is still going', async () => {
    const app = anAppWithAField()
    slowToAnswer(1500)

    app.markDocumentModified()
    const editing = app.reevaluateDocumentSelective(
      ['day/n'], [{ path: 'day/n', oldValue: 1, newValue: 2 }])

    // Past the wait the save would otherwise keep, and well short of the answer.
    await vi.advanceTimersByTimeAsync(900)
    expect(wrote(), 'a save of the old text went out while the write was in flight')
      .toHaveLength(0)

    await vi.advanceTimersByTimeAsync(1000)
    await editing
    expect(wrote(), 'and none afterwards either, the write having done it').toHaveLength(0)
  })

  it('still saves when the change was never going to be written for us', async () => {
    // A document with no file behind it has no address to write to, so the save is all there is
    // and holding it back would lose the change.
    const app = anAppWithAField()
    slowToAnswer(1500)
    app.currentFile = null
    app.markDocumentModified()
    await vi.runAllTimersAsync()
    expect(wrote()).toHaveLength(0)
  })
})
