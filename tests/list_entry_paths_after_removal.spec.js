import { describe, it, expect, beforeEach, vi } from 'vitest'

// Pressing a button on one list entry and then on another, without saving in between.
//
// Two things make this harder than it looks. Entries of a list are named by position -
// Task__1, Task__2 - whatever the list is keyed by, so removing one renames every entry after
// it. And every event is sent with the document's *text*, which is only replaced when the
// answer comes back, so a press made while another is in flight is computed against a
// document in which the first press never happened.
//
// Together those produced the reported fault: marking two tasks done in a row left one of
// them open and closed a different one, and which one depended on the timing.

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

const base = (name, node_type) => ({
  name, node_type, parameters: {}, children: [], is_hierarchy_transparent: false,
  source_id: '', source_fingerprint: 1, param_order: [], authored_dash: false,
})

const valued = (name, type, variant, value) => Object.assign(base(name, type), {
  parameters: { value: { [variant]: value } },
})

const doneButton = () => Object.assign(base('done', 'button'), {
  parameters: { value: { String: 'done' } },
  children: [base('click', 'on')],
})

/** One entry: named by its position, identified by its `added` key. */
const task = (position, entry) => Object.assign(base(`Task__${position}`, 'div'), {
  parameters: { _ui_sort_key: { Integer: entry.sort } },
  children: [
    valued('added', 'timestamp', 'Timestamp', entry.key),
    valued('title', 'string', 'String', entry.text),
    doneButton(),
  ],
})

// Keyed by `added` and sorted for display, as tasks/Open is. The sort matters: it means the
// row someone clicks second is not the second entry in the document.
const listOf = (entries) => Object.assign(base('Open', 'list'), {
  parameters: { key: { String: 'added' } },
  children: entries.map((e, i) => task(i + 1, e)),
})

const docOf = (entries) => ([Object.assign(base('tasks', 'tab'), {
  parameters: { mutable: { Boolean: true } },
  children: [listOf(entries)],
})])

const ENTRIES = [
  { key: '2026-08-01T09:00:00Z', text: 'urgent', sort: -38 },
  { key: '2026-08-02T09:00:00Z', text: 'middling', sort: -13 },
  { key: '2026-08-03T09:00:00Z', text: 'calm', sort: 0 },
]

const buttons = () => Array.from(document.querySelectorAll('[data-path]'))
  .filter(e => { try { return JSON.parse(e.dataset.path).slice(-1)[0] === 'done' } catch { return false } })

/** The title shown against each done button, in the order the page shows them. */
const rowsOnScreen = () => buttons().map((b) => {
  const path = JSON.parse(b.dataset.path)
  const titlePath = JSON.stringify([...path.slice(0, -1), 'title'])
  const el = document.querySelector(`[data-path='${titlePath}']`)
  return (el?.textContent || '').trim()
})

describe('pressing done on two entries in a row', () => {
  let app, sent, live, delay

  beforeEach(async () => {
    setupDOM()
    sent = []
    live = ENTRIES.slice()
    delay = 0

    const { invoke } = await import('@tauri-apps/api/core')
    invoke.mockImplementation(async (cmd, args) => {
      if (cmd !== 'execute_overseer_event_update') return null
      const text = args.content
      const addressed = (args.node_path || [])[2]
      sent.push({ text, addressed })

      // The server is stateless: it rebuilds the document from the text it was handed and
      // runs the event against that. A press computed against an older text cannot know about
      // one that has not been applied yet - which is the whole hazard being tested.
      const from = text === 'TEXT-BEFORE' ? ENTRIES : live
      const kept = from.filter((e, i) => `Task__${i + 1}` !== addressed)
      live = kept.slice()
      if (delay) await new Promise(r => setTimeout(r, delay))
      return {
        text: `TEXT-${live.map(e => e.text).join('+')}`,
        changes: [{ kind: 'subtree', address: 'tasks/Open', path: [0, 0], node: listOf(kept) }],
        nodes: null,
      }
    })

    app = new OverseerApp()
    app.currentDocument = docOf(ENTRIES)
    app._currentText = 'TEXT-BEFORE'
    app.renderer.renderDocument(app.currentDocument)
  })

  it('addresses the entry whose row was clicked, not the one in that position', async () => {
    expect(rowsOnScreen(), 'the page is not sorted').toEqual(['urgent', 'middling', 'calm'])
    buttons()[1].dispatchEvent(new Event('click', { bubbles: true }))
    await new Promise(r => setTimeout(r, 50))

    expect(sent.map(s => s.addressed)).toEqual(['Task__2'])
    expect(live.map(e => e.text), 'the wrong entry was closed').toEqual(['urgent', 'calm'])
  })

  it('renames the surviving entries on the page when one is removed', async () => {
    buttons()[1].dispatchEvent(new Event('click', { bubbles: true }))
    await new Promise(r => setTimeout(r, 50))

    expect(rowsOnScreen()).toEqual(['urgent', 'calm'])
    expect(
      buttons().map(b => JSON.parse(b.dataset.path)[2]),
      'the page is still offering the names the entries had before the removal'
    ).toEqual(['Task__1', 'Task__2'])
  })

  it('closes both when the second is pressed after the first has landed', async () => {
    buttons()[1].dispatchEvent(new Event('click', { bubbles: true }))
    await new Promise(r => setTimeout(r, 50))
    buttons()[1].dispatchEvent(new Event('click', { bubbles: true }))
    await new Promise(r => setTimeout(r, 50))

    expect(live.map(e => e.text)).toEqual(['urgent'])
  })

  it('closes both when the second is pressed before the first comes back', async () => {
    delay = 30
    const all = buttons()
    all[1].dispatchEvent(new Event('click', { bubbles: true }))   // middling
    all[2].dispatchEvent(new Event('click', { bubbles: true }))   // calm
    await new Promise(r => setTimeout(r, 200))

    expect(sent.length, 'a press was lost').toBe(2)
    expect(
      sent[1].text,
      'the second press was computed against the document as it stood before the first'
    ).not.toBe('TEXT-BEFORE')
    expect(live.map(e => e.text), 'a task that was pressed is still open').toEqual(['urgent'])
  })
})
