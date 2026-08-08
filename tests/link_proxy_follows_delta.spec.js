import { describe, it, expect, beforeEach, vi } from 'vitest'

// A link proxy declares (link="/History[key=$(../selected_date)]") and resolves that at render
// time, here rather than in the backend. So moving the selected date moves what the proxy
// shows without changing the proxy node at all - and an interaction answered with "what
// changed" reports only the date, because that genuinely is all that changed in the document.
//
// Repainting only what the backend named would leave the date reading one day and the day's
// contents showing another. The client has to re-derive what it derives.

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

const record = (date, note) => ({
  name: 'DayRecord', node_type: 'div', parameters: {}, is_hierarchy_transparent: false, children: [
    { name: 'date', node_type: 'timestamp', parameters: { value: { Timestamp: `${date}T00:00:00Z` }, precision: { String: 'day' } }, children: [], is_hierarchy_transparent: false },
    { name: 'note', node_type: 'string', parameters: { value: { String: note }, mutable: { Boolean: true } }, children: [], is_hierarchy_transparent: false },
  ],
})

function buildDoc(selected) {
  return [{
    name: 'tracker', node_type: 'tab', parameters: {}, is_hierarchy_transparent: false, children: [
      { name: 'Selected', node_type: 'div', parameters: {}, is_hierarchy_transparent: false, children: [
        { name: 'selected_date', node_type: 'timestamp',
          parameters: { value: { Timestamp: `${selected}T00:00:00Z` }, precision: { String: 'day' } },
          children: [], is_hierarchy_transparent: false },
        { name: 'Prev', node_type: 'button', parameters: { label: { String: '< Prev Day' } },
          is_hierarchy_transparent: false, children: [
            { name: 'click', node_type: 'on', parameters: {}, is_hierarchy_transparent: false, children: [] },
          ]},
        { name: 'SelectedDay', node_type: 'div',
          parameters: { link: { String: '/tracker/History[key=$(../selected_date)]' } },
          children: [], is_hierarchy_transparent: false },
      ]},
      // Hidden, as it is in the real document: what is on screen is the proxy's view of it.
      { name: 'History', node_type: 'list',
        parameters: { entry: { Template: 'DayRecord' }, key: { String: 'date' }, keyPrecision: { String: 'day' }, hidden: { Boolean: true } },
        is_hierarchy_transparent: false,
        children: [record('2026-08-06', 'six'), record('2026-08-05', 'five')] },
    ],
  }]
}

/** What the proxy is showing - not the whole page, which would include the list it reads. */
const shownInProxy = () => {
  const proxy = Array.from(document.querySelectorAll('[data-path]')).find(el => {
    try { return JSON.parse(el.dataset.path || '[]').slice(-1)[0] === 'SelectedDay' } catch { return false }
  })
  return proxy ? (proxy.textContent || '').trim() : ''
}

describe('a link proxy whose target depends on a changed value', () => {
  let app

  beforeEach(async () => {
    setupDOM()
    const { invoke } = await import('@tauri-apps/api/core')
    invoke.mockImplementation(async (cmd) => {
      if (cmd === 'execute_overseer_event_update') {
        // What the backend reports for a day-navigation click: the date moved, and nothing
        // else in the document did. The proxy is not mentioned, and correctly so.
        return {
          text: 'TEXT-AFTER',
          changes: [{
            kind: 'parameters',
            address: 'tracker/Selected/selected_date',
            path: [0, 0, 0],
            parameters: { value: { Timestamp: '2026-08-05T00:00:00Z' }, precision: { String: 'day' } },
          }],
          nodes: null,
        }
      }
      return null
    })

    app = new OverseerApp()
    app.currentDocument = buildDoc('2026-08-06')
    app._currentText = 'TEXT-BEFORE'
    app.renderer.renderDocument(app.currentDocument)
  })

  it('shows the newly selected day, not the one it was showing before', async () => {
    expect(shownInProxy(), 'the fixture should start on the sixth').toContain('six')

    const prev = app.currentDocument[0].children[0].children[1]
    await app.renderer.emitEvent(prev, null, 'click')
    await new Promise(r => setTimeout(r, 30))

    expect(
      shownInProxy(),
      'the proxy is still showing the previously selected day'
    ).toContain('five')
  })

  it('does not fall back to rebuilding the document to achieve it', async () => {
    let fullRenders = 0
    const render = app.renderer.renderDocument.bind(app.renderer)
    app.renderer.renderDocument = (d) => { fullRenders++; return render(d) }

    const prev = app.currentDocument[0].children[0].children[1]
    await app.renderer.emitEvent(prev, null, 'click')
    await new Promise(r => setTimeout(r, 30))

    expect(fullRenders, 'the whole document was rebuilt, which is the cost being avoided').toBe(0)
  })
})
