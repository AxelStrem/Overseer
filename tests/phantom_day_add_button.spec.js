import { describe, it, expect, beforeEach, vi } from 'vitest'

// Navigating to a date with nothing logged shows a phantom preview of a day, built from the
// template. Its rendered paths carry a literal `<phantom>` segment that exists only in the
// DOM, so an event addressed that way cannot be resolved by the backend. Editing a field
// materializes the preview into a real entry first; a button has to do the same, or logging
// food is impossible on precisely the days that have none.

function setupDOM() {
  document.body.innerHTML = `
    <div id="app">
      <div id="toolbar">
        <button id="open-file-btn"></button>
        <button id="new-file-btn"></button>
      </div>
      <button id="welcome-open-btn"></button>
      <button id="welcome-new-btn"></button>
      <button id="error-back-btn"></button>
      <div id="tab-container"></div>
      <div id="content-display"></div>
      <div id="status-bar">
        <span id="status-message"></span>
        <span id="status-info"></span>
        <span id="file-path"></span>
      </div>
      <div id="welcome-screen" class="screen"></div>
      <div id="editor-screen" class="screen"></div>
      <div id="error-screen" class="screen"></div>
      <div id="error-message"></div>
    </div>
  `
}

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }))

import { OverseerApp } from '../src/main.js'
import { answerEnsureEntry } from './helpers/ensure-entry.js'

function deepClone(o) { return JSON.parse(JSON.stringify(o)) }

const addButton = (name, label) => ({
  name, node_type: 'button', parameters: { label: { String: label } }, is_hierarchy_transparent: false,
  children: [{
    name: 'click', node_type: 'on', parameters: {}, is_hierarchy_transparent: false,
    children: [{
      name: 'append', node_type: 'append',
      parameters: { list: { String: '../intake' } },
      children: [], is_hierarchy_transparent: false
    }]
  }]
})

/// The buttons in a row of their own, which is how the tracker has them - loose in the day's
/// row they wrapped separately. The wrapper is unnamed, so an address looks through it and an
/// action path does not.
///
/// Nested twice below, because that is the depth the real document has: the day holds an unnamed
/// div of fields, and the row of buttons sits inside that. One wrapper was not enough to tell a
/// rule that drops every wrapper from one that drops the first.
const buttonsInARow = (...buttons) => ({
  name: '', node_type: 'div',
  parameters: { layout: { String: 'horizontal' }, margin: { Integer: 0 } },
  is_hierarchy_transparent: true, children: buttons,
})

// History holds 2026-08-04 only; the selected date is a day with nothing logged.
function buildDoc(nested = false) {
  return [{
    name: 'tracker_v2', node_type: 'tab', parameters: {}, is_hierarchy_transparent: false, children: [
      {
        name: '', node_type: 'div', parameters: { hidden: { Boolean: true } }, is_hierarchy_transparent: true, children: [
          {
            name: 'DayRecord', node_type: 'div', parameters: {}, is_hierarchy_transparent: false, children: [
              { name: 'date', node_type: 'timestamp', parameters: { precision: { String: 'day' }, value: { String: '2026-08-04' } }, children: [], is_hierarchy_transparent: false },
              { name: 'intake', node_type: 'list', parameters: { entry: { Template: 'MealRecord' } }, children: [], is_hierarchy_transparent: false },
              nested
                ? buttonsInARow(buttonsInARow(addButton('add_by_portions', '+ Add by portions')))
                : addButton('add_by_portions', '+ Add by portions'),
            ]
          },
        ]
      },
      {
        name: 'Selected', node_type: 'div', parameters: {}, is_hierarchy_transparent: false, children: [
          { name: 'selected_date', node_type: 'timestamp', parameters: { precision: { String: 'day' }, value: { String: '2026-08-06' } }, children: [], is_hierarchy_transparent: false },
          {
            name: 'SelectedDay', node_type: 'div',
            parameters: {
              mutable: { Boolean: true },
              link: { String: '/tracker_v2/History[key=$(../selected_date)]' },
              'phantom-materialize': { String: 'prepend-on-edit' },
            },
            is_hierarchy_transparent: false, children: []
          },
        ]
      },
      {
        name: 'History', node_type: 'list',
        parameters: { entry: { Template: 'DayRecord' }, key: { String: 'date' }, keyPrecision: { String: 'day' } },
        is_hierarchy_transparent: false, children: [
          {
            name: 'DayRecord__1', node_type: 'div',
            parameters: { _from_template: true, _original_type: 'DayRecord' },
            is_hierarchy_transparent: false, children: [
              { name: 'date', node_type: 'timestamp', parameters: { value: { String: '2026-08-04' } }, children: [], is_hierarchy_transparent: false },
              { name: 'intake', node_type: 'list', parameters: { entry: { Template: 'MealRecord' } }, children: [], is_hierarchy_transparent: false },
              nested
                ? buttonsInARow(buttonsInARow(addButton('add_by_portions', '+ Add by portions')))
                : addButton('add_by_portions', '+ Add by portions'),
            ]
          },
        ]
      },
    ]
  }]
}

describe('logging food on a day with nothing tracked', () => {
  beforeEach(() => setupDOM())

  const pressAddOnAMissingDay = async (nested) => {
    const { invoke } = await import('@tauri-apps/api/core')
    const events = []
    const madeWith = []
    invoke.mockImplementation((cmd, args) => {
      if (cmd === 'ensure_overseer_entry') madeWith.push(args.wanted)
      // Making a preview real is a backend instruction now; the page used to do it itself in
      // its own copy of the document. See `helpers/ensure-entry.js`.
      const madeReal = answerEnsureEntry(() => app, cmd, args)
      if (madeReal !== null) return madeReal
      if (cmd === 'get_next_timer_due_ms' || cmd === 'scheduler_tick') return Promise.resolve(null)
      if (cmd === 'execute_overseer_event') {
        events.push({ path: args.node_path || args.nodePath, event: args.event_name || args.eventName })
        return Promise.resolve(deepClone(args.nodes))
      }
      if (cmd === 'serialize_overseer_nodes') return Promise.resolve('DOC')
      if (cmd === 'parse_overseer_content' || cmd === 'parse_overseer_content_selective') {
        return Promise.resolve(deepClone(window.app.currentDocument))
      }
      return Promise.resolve(null)
    })

    const app = new OverseerApp()
    // Opened from somewhere, which every document a view is edited through is:
    // making a preview real is a write, and a write needs a file to reach.
    app.currentFile = '/documents/test.os'
    app.currentDocument = buildDoc(nested)
    app.renderer.renderDocument(app.currentDocument)

    const linkEl = Array.from(document.querySelectorAll('[data-path]')).find(el => {
      try { return JSON.parse(el.dataset.path || '[]').join('/') === 'tracker_v2/Selected/SelectedDay' } catch { return false }
    })
    expect(linkEl, 'the selected-day proxy should render').toBeTruthy()
    expect(
      linkEl.hasAttribute('data-link-phantom'),
      'a date with no history entry should render as a phantom preview'
    ).toBe(true)

    const btn = Array.from(linkEl.querySelectorAll('[data-path]')).find(el => {
      try { return JSON.parse(el.dataset.path || '[]').slice(-1)[0] === 'add_by_portions' } catch { return false }
    })
    expect(btn, 'the Add button should render inside the phantom day').toBeTruthy()
    expect(
      JSON.parse(btn.dataset.path).includes('<phantom>'),
      'precondition: the button path should be phantom-qualified before the click'
    ).toBe(true)

    const historyBefore = app.currentDocument[0].children.find(c => c.name === 'History').children.length

    const clickable = btn.matches('button') ? btn : btn.querySelector('button')
    expect(clickable, 'the Add button should be a real button').toBeTruthy()
    clickable.dispatchEvent(new Event('click', { bubbles: true }))
    await new Promise(r => setTimeout(r, 50))

    // One instruction: the day is made and the press runs in it, as one change and one step to
    // take back. Two used to be sent - make the day, then press - which was two writes.
    expect(madeWith.length, 'the click should reach the backend').toBe(1)
    expect(events.length, 'the press went as a second instruction after the day was made').toBe(0)
    const then = madeWith[0].then
    expect(then, 'the press did not ride with the instruction that makes the day').toBeTruthy()
    expect(then.event).toBe('click')
    expect(
      then.within.includes('<phantom>'),
      `the press must be named from the entry down, got ${JSON.stringify(then.within)}`
    ).toBe(false)
    expect(then.within.slice(-1)[0]).toBe('add_by_portions')

    const history = app.currentDocument[0].children.find(c => c.name === 'History')
    expect(history.children.length, 'the phantom day should have become a real history entry').toBe(historyBefore + 1)

    // And it has to name something. The backend looks for an `on click` on the node the press
    // names, from the entry it has just made, and finding nothing there says nothing about why.
    const entry = history.children[history.children.length - 1]
    const landed = app.renderer.findNodeByPath(app.currentDocument,
      ['tracker_v2', 'History', entry.name, ...then.within])
    expect(landed, `the press names nothing in the new entry: ${JSON.stringify(then.within)}`).toBeTruthy()
    expect(landed.node_type).toBe('button')
  }

  it('materializes the phantom day so the event carries a resolvable path', async () => {
    await pressAddOnAMissingDay(false)
  })

  it('does the same when the buttons sit in a row of their own', async () => {
    // Which is how the tracker has them. An unnamed wrapper is looked through by an address and
    // not by an action path, so a path built for one scheme does not answer in the other.
    await pressAddOnAMissingDay(true)
  })
})
