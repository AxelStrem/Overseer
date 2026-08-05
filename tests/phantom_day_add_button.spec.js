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
        <button id="save-file-btn"></button>
        <button id="reload-file-btn"></button>
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

// History holds 2026-08-04 only; the selected date is a day with nothing logged.
function buildDoc() {
  return [{
    name: 'tracker_v2', node_type: 'tab', parameters: {}, is_hierarchy_transparent: false, children: [
      {
        name: '', node_type: 'div', parameters: { hidden: { Boolean: true } }, is_hierarchy_transparent: true, children: [
          {
            name: 'DayRecord', node_type: 'div', parameters: {}, is_hierarchy_transparent: false, children: [
              { name: 'date', node_type: 'timestamp', parameters: { precision: { String: 'day' }, value: { String: '2026-08-04' } }, children: [], is_hierarchy_transparent: false },
              { name: 'intake', node_type: 'list', parameters: { entry: { Template: 'MealRecord' } }, children: [], is_hierarchy_transparent: false },
              addButton('add_by_portions', '+ Add by portions'),
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
              addButton('add_by_portions', '+ Add by portions'),
            ]
          },
        ]
      },
    ]
  }]
}

describe('logging food on a day with nothing tracked', () => {
  beforeEach(() => setupDOM())

  it('materializes the phantom day so the event carries a resolvable path', async () => {
    const { invoke } = await import('@tauri-apps/api/core')
    const events = []
    invoke.mockImplementation((cmd, args) => {
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
    app.currentDocument = buildDoc()
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

    expect(events.length, 'the click should reach the backend').toBeGreaterThan(0)
    const sent = events[0].path
    expect(
      sent.includes('<phantom>'),
      `the event path must be resolvable, got ${JSON.stringify(sent)}`
    ).toBe(false)
    expect(sent.slice(-1)[0]).toBe('add_by_portions')

    const historyAfter = app.currentDocument[0].children.find(c => c.name === 'History').children.length
    expect(historyAfter, 'the phantom day should have become a real history entry').toBe(historyBefore + 1)
  })
})
