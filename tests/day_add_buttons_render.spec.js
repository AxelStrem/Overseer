import { describe, it, expect, beforeEach, vi } from 'vitest'

// The Add buttons live inside the day record so their `../intake` target resolves to the day
// they are rendered for. That places them inside the selected-day link proxy, which is a
// different render path from an ordinary top-level button - so this checks they arrive as
// real, clickable <button> elements that emit an event, rather than inert text.

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

function deepClone(o) { return JSON.parse(JSON.stringify(o)) }

const addButton = (name, label) => ({
  name, node_type: 'button', parameters: { label: { String: label } }, is_hierarchy_transparent: false,
  children: [
    {
      name: 'click', node_type: 'on', parameters: {}, is_hierarchy_transparent: false, children: [
        {
          name: 'append', node_type: 'append',
          parameters: { list: { String: '../intake' } },
          children: [], is_hierarchy_transparent: false
        },
      ]
    },
  ]
})

function buildDoc() {
  return [{
    name: 'tracker_v2', node_type: 'tab', parameters: {}, is_hierarchy_transparent: false, children: [
      {
        name: 'Selected', node_type: 'div', parameters: {}, is_hierarchy_transparent: false, children: [
          {
            name: 'SelectedDay', node_type: 'div',
            parameters: { mutable: { Boolean: true }, link: { String: '/tracker_v2/History[key=$(../selected_date)]' } },
            is_hierarchy_transparent: false, children: []
          },
        ]
      },
      {
        name: 'History', node_type: 'list',
        parameters: { entry: { Template: 'DayRecord' }, key: { String: 'date' } },
        is_hierarchy_transparent: false, children: [
          {
            name: 'DayRecord__1', node_type: 'div',
            parameters: { _from_template: true, _original_type: 'DayRecord' },
            is_hierarchy_transparent: false, children: [
              { name: 'date', node_type: 'timestamp', parameters: { value: { String: '2026-08-04' } }, children: [], is_hierarchy_transparent: false },
              { name: 'intake', node_type: 'list', parameters: { entry: { Template: 'MealRecord' } }, children: [], is_hierarchy_transparent: false },
              addButton('add_by_portions', '+ Add by portions'),
              addButton('add_by_grams', '+ Add by weight'),
            ]
          },
        ]
      },
    ]
  }]
}

describe('day Add buttons', () => {
  beforeEach(() => setupDOM())

  it('render as clickable buttons and emit an event', async () => {
    const { invoke } = await import('@tauri-apps/api/core')
    const events = []
    invoke.mockImplementation((cmd, args) => {
      if (cmd === 'get_next_timer_due_ms' || cmd === 'scheduler_tick') return Promise.resolve(null)
      if (cmd === 'execute_overseer_event') {
        events.push({ path: args.node_path || args.nodePath, event: args.event_name || args.eventName })
        return Promise.resolve(deepClone(args.nodes))
      }
      if (cmd === 'serialize_overseer_nodes') return Promise.resolve('DOC')
      return Promise.resolve(null)
    })

    const app = new OverseerApp()
    app.currentDocument = buildDoc()
    app.renderer.renderDocument(app.currentDocument)

    for (const [name, label] of [['add_by_portions', '+ Add by portions'], ['add_by_grams', '+ Add by weight']]) {
      const container = Array.from(document.querySelectorAll('[data-path]')).find(el => {
        try { return JSON.parse(el.dataset.path || '[]').slice(-1)[0] === name } catch { return false }
      })
      expect(container, `${name} should render`).toBeTruthy()

      const btn = container.matches('button') ? container : container.querySelector('button')
      expect(btn, `${name} should be an actual <button>, got <${container.tagName.toLowerCase()}>`).toBeTruthy()
      expect(btn.textContent).toContain(label)

      btn.dispatchEvent(new Event('click', { bubbles: true }))
      await new Promise(r => setTimeout(r, 10))
    }

    expect(events.length, 'both buttons should reach the backend').toBe(2)
    expect(events[0].event).toBe('click')
    expect(events[0].path.slice(-1)[0]).toBe('add_by_portions')
    expect(events[1].path.slice(-1)[0]).toBe('add_by_grams')
    // The path must address the day, so `../intake` resolves to that day's list.
    expect(events[0].path).toContain('DayRecord__1')
  })
})
