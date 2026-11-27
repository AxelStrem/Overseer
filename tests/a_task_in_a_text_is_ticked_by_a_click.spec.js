import { describe, it, expect, vi } from 'vitest'

// A task list in a markdown text, `- [ ] buy milk` / `- [x] buy milk`, draws a box on each task,
// and a click on it rewrites that one task's mark in the text, as an edit of the text would.

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
import { withTaskTicked, listItemLines } from '../src/renderer.js'

const base = (name, node_type, parameters = {}, children = []) => ({
  name, node_type, parameters, children, is_hierarchy_transparent: false,
  source_id: '', source_fingerprint: 1, param_order: [], authored_dash: false,
})

let written
const draw = (body, { mutable = true, onTab = false, formula = false } = {}) => {
  setupDOM()
  const app = new OverseerApp()
  window.app = app
  written = []
  app.markDocumentModified = () => {}
  app.reevaluateDocumentSelective = (paths, changes) => { written.push(...changes) }
  const params = { value: formula ? { Formula: '"' + body + '"' } : { String: body } }
  if (formula) params._computed_value = { String: body }
  const tabParams = {}
  if (mutable && onTab) tabParams.mutable = { Boolean: true }
  else if (mutable) params.mutable = { Boolean: true }
  app.currentDocument = [base('t', 'tab', tabParams, [base('body', 'text', params)])]
  app.currentFile = 'notes/t.os'
  app.renderer.renderDocument(app.currentDocument)
  return app
}
const settle = () => new Promise((r) => setTimeout(r, 20))
const boxes = () => Array.from(document.querySelectorAll('.text-content .ov-task'))
const click = (input) => input.dispatchEvent(new window.MouseEvent('click', { bubbles: true, cancelable: true }))

describe('a task in a text is ticked by a click', () => {
  it('ticks and unticks one task of a text and nothing else', () => {
    const text = 'Shopping:\n\n- [ ] milk\n- [x] bread\n  - [ ] rye\n\n```\n- [ ] not a task\n```\n\n1. [X] numbered\n\nSay [ ] in prose.'
    expect(withTaskTicked(text, 0, true)).toBe(text.replace('- [ ] milk', '- [x] milk'))
    expect(withTaskTicked(text, 1, false)).toBe(text.replace('- [x] bread', '- [ ] bread'))
    expect(withTaskTicked(text, 2, true)).toBe(text.replace('  - [ ] rye', '  - [x] rye'))
    expect(withTaskTicked(text, 3, false)).toBe(text.replace('1. [X] numbered', '1. [ ] numbered'))
    expect(withTaskTicked(text, 4, true)).toBeNull()
  })

  it('finds tasks in quotes, loose lists and texts with any line ending', () => {
    expect(withTaskTicked('> - [ ] a\n> - [ ] b', 1, true)).toBe('> - [ ] a\n> - [x] b')
    expect(withTaskTicked('- [ ] a\n\n  more of a\n\n* [ ] b', 1, true)).toBe('- [ ] a\n\n  more of a\n\n* [x] b')
    expect(withTaskTicked('intro\r\n\r\n- [ ] a\r\n- [ ] b\r\n', 1, true)).toBe('intro\r\n\r\n- [ ] a\r\n- [x] b\r\n')
    expect(withTaskTicked('- a\n  ```\n  - [ ] code\n  ```\n- [ ] real', 0, true)).toBe('- a\n  ```\n  - [ ] code\n  ```\n- [x] real')
  })

  it('counts every list item, a task or not, in the order they are drawn', () => {
    const lines = listItemLines('- one\n  - [ ] two\n- ( ) three').map(({ item, line }) => [item.task, line])
    expect(lines).toEqual([[false, 0], [true, 1], [false, 2]])
  })

  it('draws a box on each task, ticked as written, and strikes a done task through', () => {
    draw('- [ ] milk\n- [x] bread\n  - [ ] rye')
    const [milk, bread, rye] = boxes()
    expect([milk.checked, bread.checked, rye.checked]).toEqual([false, true, false])
    expect(boxes().map((b) => b.dataset.taskIndex)).toEqual(['0', '1', '2'])
    expect(boxes().every((b) => !b.disabled)).toBe(true)
    const done = document.querySelector('li.ov-task-item.done')
    expect(done.querySelector(':scope > .ov-task-text').textContent).toBe('bread')
    expect(done.querySelector('li.ov-task-item:not(.done) .ov-task-text').textContent).toBe('rye')
  })

  it('a click on the second task writes that one line as done, and a second click undoes it', async () => {
    const app = draw('- [ ] milk\n- [ ] bread')
    click(boxes()[1])
    await settle()
    expect(written).toEqual([{ path: 't/body', oldValue: '- [ ] milk\n- [ ] bread', newValue: '- [ ] milk\n- [x] bread' }])
    expect(app.currentDocument[0].children[0].parameters.value).toEqual({ String: '- [ ] milk\n- [x] bread' })
    expect(boxes().map((b) => b.checked)).toEqual([false, true])
    expect(document.querySelector('textarea')).toBeNull()

    click(boxes()[1])
    await settle()
    expect(written[1].newValue).toBe('- [ ] milk\n- [ ] bread')
  })

  it('a text that takes its mutability from above can be ticked', async () => {
    draw('- [ ] milk', { onTab: true })
    expect(boxes()[0].disabled).toBe(false)
    click(boxes()[0])
    await settle()
    expect(written.map((w) => w.newValue)).toEqual(['- [x] milk'])
  })

  it('keeps the boxes shut and writes nothing in a text that cannot be changed or is worked out', async () => {
    draw('- [ ] milk', { mutable: false })
    expect(boxes()[0].disabled).toBe(true)
    click(boxes()[0])
    draw('- [ ] milk', { formula: true })
    expect(boxes()[0].disabled).toBe(true)
    click(boxes()[0])
    await settle()
    expect(written).toEqual([])
  })
})
