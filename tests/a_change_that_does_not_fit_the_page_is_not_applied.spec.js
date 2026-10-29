import { describe, it, expect, beforeEach, vi } from 'vitest'

// An answer finds the nodes it changes by child index, and says each one's address beside it.
//
// When the backend described it against a tree of another shape than the page holds - the same
// text worked out two ways - the index reached a neighbour. A change of parameters then made the
// neighbour look like the node it was meant for, keeping its type: a project item's Talk button
// drew as a button labelled "moved", and the timer it was meant for went hidden. A press on Talk
// asked to fold a node two places along, which no fold could turn, and nothing happened. So what
// does not fit its address is not applied, and the page reads the document again whole.
//
// And a fold asked by a press whose answer is the whole document is done after drawing it: it
// was dropped, and Talk did nothing whenever its answer came whole.

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

const base = (name, node_type, parameters = {}, children = [], transparent = false) => ({
  name, node_type, parameters, children, is_hierarchy_transparent: transparent,
  source_id: '', source_fingerprint: 1, param_order: [], authored_dash: false,
})

// t / Items / Item__n / [ handle, (row: moved, mark, talk), Talk ]
const item = (n, handle) => base(`Item__${n}`, 'div', {}, [
  base('handle', 'string', { value: { String: handle } }),
  base('div', 'div', { layout: { String: 'horizontal' } }, [
    base('moved', 'string', { label: { String: 'moved' }, value: { String: '2h' } }),
    base('mark', 'mark', { type: { String: 'separator' } }),
    base('talk', 'button', { icon: { String: 'note' } }, [
      base('click', 'on', {}, [base('toggle_fold', 'toggle_fold', { path: { String: '../Talk' } })]),
    ]),
  ], true),
  base('Talk', 'div', { foldable: { Boolean: true } }, [
    base('said', 'string', { value: { String: `said about ${handle}` } }),
  ]),
])

const theDocument = () => [base('t', 'tab', { mutable: { Boolean: true } }, [
  base('Items', 'list', { key: { String: 'handle' } }, [item(1, 'a'), item(2, 'b')]),
])]

let app

const open = () => {
  app = new OverseerApp()
  app.currentDocument = theDocument()
  app.currentFile = 't.os'
  app._currentText = 'TEXT'
  app.renderer._filters = new Map()
  app.renderer.renderDocument(app.currentDocument)
  app.rereadFromDisk = vi.fn(async () => {})
}

const entry = (n) => app.currentDocument[0].children[0].children[n]
const drawn = (...path) => document.querySelector(`[data-path='${JSON.stringify(['t', 'Items', ...path])}']`)
const moved = { label: { String: 'moved' }, value: { String: '3h' } }

describe('a change found by index', () => {
  beforeEach(() => {
    setupDOM()
    sessionStorage.clear()
    invoke.mockReset()
    vi.spyOn(console, 'warn').mockImplementation(() => {})
  })

  it('is applied where its address says it is', () => {
    open()
    const touched = app.applyDocumentChanges(app.currentDocument, [
      { kind: 'parameters', address: 't/Items/[a]/moved', path: [0, 0, 0, 1, 0], parameters: moved },
    ])
    expect(entry(0).children[1].children[0].parameters.value).toEqual({ String: '3h' })
    expect(touched.misfit).toBeFalsy()
    expect(app.rereadFromDisk).not.toHaveBeenCalled()
  })

  it('is not applied to a neighbour its index reaches instead, and the document is read again', async () => {
    open()
    // The address is the timer; the index, two places along, is the Talk button.
    const touched = app.applyDocumentChanges(app.currentDocument, [
      { kind: 'parameters', address: 't/Items/[a]/moved', path: [0, 0, 0, 1, 2], parameters: moved },
    ])
    const talk = entry(0).children[1].children[2]
    expect(talk.parameters.icon, 'the button took the timer\'s parameters').toEqual({ String: 'note' })
    expect(talk.parameters.label).toBeUndefined()
    expect(touched.misfit).toBe(true)
    expect(touched.length, 'something was handed on to repaint').toBe(0)
    await app._readingWholeAfterAMisfit
    expect(app.rereadFromDisk).toHaveBeenCalledTimes(1)
  })

  it('stops at the first that does not fit', () => {
    open()
    app.applyDocumentChanges(app.currentDocument, [
      { kind: 'parameters', address: 't/Items/[a]/moved', path: [0, 0, 0, 1, 2], parameters: moved },
      { kind: 'parameters', address: 't/Items/[b]/moved', path: [0, 0, 1, 1, 0], parameters: moved },
    ])
    expect(entry(1).children[1].children[0].parameters.value).toEqual({ String: '2h' })
  })

  it('on a wrapper is said under its container, and fits', () => {
    open()
    app.applyDocumentChanges(app.currentDocument, [
      { kind: 'parameters', address: 't/Items/[a]', path: [0, 0, 0, 1], parameters: { layout: { String: 'vertical' } } },
    ])
    expect(entry(0).children[1].parameters.layout).toEqual({ String: 'vertical' })
    expect(app.rereadFromDisk).not.toHaveBeenCalled()
  })

  it('sent whole in place of another node is not put there', async () => {
    open()
    app.applyDocumentChanges(app.currentDocument, [
      { kind: 'subtree', address: 't/Items/[a]/moved', path: [0, 0, 0, 1, 2], node: base('moved', 'string', moved) },
    ])
    expect(entry(0).children[1].children[2].node_type).toBe('button')
    await app._readingWholeAfterAMisfit
    expect(app.rereadFromDisk).toHaveBeenCalledTimes(1)
  })
})

describe('a fold a press asks for', () => {
  beforeEach(() => {
    setupDOM()
    sessionStorage.clear()
    invoke.mockReset()
    vi.spyOn(console, 'warn').mockImplementation(() => {})
  })

  const pressTalk = async (index) => {
    const button = entry(index).children[1].children[2]
    await app.renderer.emitEvent(button, drawn(`Item__${index + 1}`, 'talk'), 'click')
  }
  const talkShows = (n) => !!drawn(`Item__${n}`, 'Talk', 'said')

  it('is not done to a node its address does not name', async () => {
    open()
    // Talk's address, and the index of the handle field.
    invoke.mockImplementation(async (cmd) => cmd === 'run_overseer_event'
      ? { text: 'TEXT', changes: [], nodes: null, wrote: false,
          folds: [{ address: 't/Items/[a]/Talk', path: [0, 0, 0, 0], how: 'toggle' }] }
      : null)
    await pressTalk(0)
    expect(talkShows(1)).toBe(true)
    await app._readingWholeAfterAMisfit
    expect(app.rereadFromDisk).toHaveBeenCalledTimes(1)
  })

  it('is done when the answer is the whole document', async () => {
    open()
    expect(talkShows(1)).toBe(true)
    invoke.mockImplementation(async (cmd) => cmd === 'run_overseer_event'
      ? { text: 'TEXT', changes: null, nodes: theDocument(), wrote: true,
          folds: [{ address: 't/Items/[a]/Talk', path: [0, 0, 0, 2], how: 'toggle' }] }
      : null)
    await pressTalk(0)
    expect(talkShows(1), 'the fold of a whole answer was dropped').toBe(false)
    expect(talkShows(2)).toBe(true)
  })
})
