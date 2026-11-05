import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'

// The drawer draws a document of its own beside the one open: a pane, with a renderer, a document
// and a channel of its own. Two documents can have nodes at the same paths; neither may find,
// fold, write or restyle the other's, and every command the pane sends names its document and the
// one open beside it. Undo, and what its calls do, stay the page's.

function setupDOM() {
  document.body.innerHTML = `
    <div id="app">
      <div id="toolbar"><button id="open-file-btn"></button><button id="new-file-btn"></button>
        </div>
      <button id="welcome-open-btn"></button><button id="welcome-new-btn"></button>
      <button id="error-back-btn"></button>
      <nav id="drawer"><div id="drawer-document" hidden></div></nav>
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
import { DocumentPane } from '../src/pane.js'

const base = (name, node_type, parameters = {}, children = [], transparent = false) => ({
  name, node_type, parameters, children, is_hierarchy_transparent: transparent,
  source_id: '', source_fingerprint: 1, param_order: [], authored_dash: false,
})

const theme = (accent) => base('theme', 'context', { _context_copy: { Boolean: true } }, [
  base('accent', '-', { value: { String: accent } }),
])

// The same shape in both: t / [ said, go, talk, Talk / note ], at the same paths.
const theDocument = (said, accent) => [
  base('t', 'tab', { mutable: { Boolean: true } }, [
    base('said', 'string', { value: { String: said } }),
    base('go', 'button', { label: { String: 'Go' } }, [
      base('click', 'on', {}, [base('set', 'set', { path: { String: '../said' } })]),
    ]),
    base('talk', 'button', { label: { String: 'Talk' } }, [
      base('click', 'on', {}, [base('toggle_fold', 'toggle_fold', { path: { String: '../Talk' } })]),
    ]),
    base('Talk', 'div', { foldable: { Boolean: true }, folded: { Boolean: true } }, [
      base('note', 'string', { value: { String: `a note in ${said}` } }),
    ]),
  ]),
  theme(accent),
]

let main
let pane

const drawBoth = () => {
  main = new OverseerApp()
  main.currentFile = 'tasks.os'
  main.currentDocument = theDocument('the open one', '#111111')
  main._currentText = 'TASKS'
  main.renderer.renderDocument(main.currentDocument)

  pane = new DocumentPane(main, document.getElementById('drawer-document'))
  pane.currentFile = 'drawer.os'
  pane.currentDocument = theDocument('the drawer', '#222222')
  pane._currentText = 'DRAWER'
  pane.renderer.renderDocument(pane.currentDocument)
}

const pathOf = (...path) => `[data-path='${JSON.stringify(['t', ...path])}']`
const inPage = (...path) => document.getElementById('content-display').querySelector(pathOf(...path))
const inPane = (...path) => document.getElementById('drawer-document').querySelector(pathOf(...path))
const press = async (app, name) => {
  const button = app.currentDocument[0].children.find((n) => n.name === name)
  await app.renderer.emitEvent(button, app.renderer.scopedOne(pathOf(name)), 'click')
  await new Promise((r) => setTimeout(r, 0))
}
// A folded div is not drawn.
const opened = (el) => !!el && !el.classList.contains('folded')
const pressesSent = () => invoke.mock.calls.filter(([cmd]) => cmd === 'run_overseer_event')

// What the server answers a press: `said` changed, and whatever calls it hands over.
const answering = (said, calls) => invoke.mockImplementation(async (cmd) =>
  cmd === 'run_overseer_event'
    ? {
      text: 'TEXT', nodes: null, wrote: true, calls,
      changes: [{ kind: 'parameters', path: [0, 0], address: 't/said', parameters: { value: { String: said } } }],
    }
    : null)

describe('two documents drawn side by side', () => {
  beforeEach(() => {
    setupDOM()
    sessionStorage.clear()
    invoke.mockReset()
    invoke.mockImplementation(async () => null)
    document.documentElement.removeAttribute('style')
  })

  afterEach(() => {
    if (pane) pane.close()
    pane = null
  })

  it('draws each in its own place, and each finds only what it drew', () => {
    drawBoth()
    expect(inPage('said').textContent).toContain('the open one')
    expect(inPane('said').textContent).toContain('the drawer')
    expect(main.renderer.scopedOne(pathOf('said'))).toBe(inPage('said'))
    expect(pane.renderer.scopedOne(pathOf('said'))).toBe(inPane('said'))
    expect(Array.from(main.renderer.scopedAll('[data-path]')).some((el) => el.closest('#drawer-document'))).toBe(false)
    expect(Array.from(pane.renderer.scopedAll('[data-path]')).every((el) => el.closest('#drawer-document'))).toBe(true)
  })

  it('folds and unfolds in one without touching the other', async () => {
    drawBoth()
    expect(opened(inPane('Talk'))).toBe(false)
    expect(opened(inPage('Talk'))).toBe(false)
    await press(pane, 'talk')
    expect(opened(inPane('Talk')), 'the drawer did not open its div').toBe(true)
    expect(opened(inPage('Talk')), 'the open document opened its div').toBe(false)
    await press(main, 'talk')
    expect(opened(inPage('Talk'))).toBe(true)
    await press(pane, 'talk')
    expect(opened(inPane('Talk'))).toBe(false)
    expect(opened(inPage('Talk')), 'the drawer folded the open document').toBe(true)
  })

  it('sends a press in the drawer as the drawer document\'s, naming the one open beside it', async () => {
    drawBoth()
    answering('pressed in the drawer')
    await press(pane, 'go')
    expect(pressesSent()).toHaveLength(1)
    const [, , options] = pressesSent()[0]
    expect(options.headers).toEqual({ 'Overseer-Document': 'drawer.os', 'Overseer-Open': 'tasks.os' })
    expect(inPane('said').textContent).toContain('pressed in the drawer')
    expect(inPage('said').textContent).toContain('the open one')
    expect(main.currentDocument[0].children[0].parameters.value).toEqual({ String: 'the open one' })
  })

  it('sends a press in the open document as it always went', async () => {
    drawBoth()
    answering('pressed in the page')
    await press(main, 'go')
    expect(pressesSent()).toHaveLength(1)
    expect(pressesSent()[0]).toHaveLength(2)
    expect(inPage('said').textContent).toContain('pressed in the page')
    expect(inPane('said').textContent).toContain('the drawer')
  })

  it('keeps the drawer\'s theme in the drawer', () => {
    drawBoth()
    const drawer = document.getElementById('drawer-document')
    expect(document.documentElement.style.getPropertyValue('--ov-accent')).toBe('#111111')
    expect(drawer.style.getPropertyValue('--ov-accent')).toBe('#222222')
  })

  it('undoes the open document, from the drawer as from the page', async () => {
    drawBoth()
    const undone = vi.spyOn(main, 'undoLastChange').mockResolvedValue(undefined)
    await pane.undoLastChange()
    expect(undone).toHaveBeenCalledTimes(1)
  })

  it('runs the calls of a press in the drawer against the page', async () => {
    drawBoth()
    const undone = vi.spyOn(main, 'undoLastChange').mockResolvedValue(undefined)
    answering('pressed', [{ name: 'undo', args: {} }])
    await press(pane, 'go')
    await new Promise((r) => setTimeout(r, 0))
    expect(undone).toHaveBeenCalledTimes(1)
  })

  it('names documents under any path in headers the desktop can send', async () => {
    drawBoth()
    main.currentFile = 'Заметки/дом.os'
    pane.currentFile = 'Заметки/ящик.os'
    answering('pressed in the drawer')
    await press(pane, 'go')
    const [, , options] = pressesSent()[0]
    // What Tauri's IPC makes of them, which takes Latin-1 alone.
    const headers = new Headers(options.headers)
    expect(decodeURIComponent(headers.get('Overseer-Document'))).toBe('Заметки/ящик.os')
    expect(decodeURIComponent(headers.get('Overseer-Open'))).toBe('Заметки/дом.os')
  })

  it('opens a document into the drawer through its own channel, and follows the open one', async () => {
    main = new OverseerApp()
    main.currentFile = 'tasks.os'
    invoke.mockImplementation(async (cmd) => {
      if (cmd === 'load_overseer_file') return 'DRAWER'
      if (cmd === 'parse_overseer_content') return theDocument('the drawer', '#222222')
      if (cmd === 'catch_up_overseer_document') return { text: 'DRAWER', changes: [] }
      return null
    })
    pane = await main.openDrawerDocument('drawer.os')
    expect(document.getElementById('drawer-document').hidden).toBe(false)
    expect(inPane('said').textContent).toContain('the drawer')
    const loads = invoke.mock.calls.filter(([cmd]) => cmd === 'load_overseer_file')
    expect(loads).toHaveLength(1)
    expect(loads[0][2].headers['Overseer-Document']).toBe('drawer.os')
    expect(document.getElementById('editor-screen').classList.contains('active')).toBe(false)

    main.currentFile = 'shopping.os'
    main.updateTitle()
    await new Promise((r) => setTimeout(r, 80))
    const caught = invoke.mock.calls.filter(([cmd]) => cmd === 'catch_up_overseer_document')
    expect(caught).toHaveLength(1)
    expect(caught[0][2].headers).toEqual({ 'Overseer-Document': 'drawer.os', 'Overseer-Open': 'shopping.os' })
  })

  it('leaves the page to its own document once the pane is closed', () => {
    drawBoth()
    main.closeDrawerDocument?.()
    pane.close()
    expect(document.getElementById('drawer-document').children).toHaveLength(0)
    expect(main.renderer.scopedOne(pathOf('said'))).toBe(inPage('said'))
  })
})
