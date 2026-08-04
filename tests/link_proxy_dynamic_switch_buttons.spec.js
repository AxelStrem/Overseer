import { describe, it, expect, beforeEach, vi } from 'vitest'

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

function findElementByPath(pathArray) {
  const all = Array.from(document.querySelectorAll('[data-path]'))
  for (const el of all) {
    try {
      const p = JSON.parse(el.dataset.path || '[]')
      if (Array.isArray(p) && p.length === pathArray.length && p.every((v,i)=>v===pathArray[i])) return el
    } catch(_) {}
  }
  return null
}

function toIsoDay(dateStr) {
  // Accept YYYY-MM-DD or RFC3339 or YYYY.MM.DD; return RFC3339 at 00:00:00Z
  if (/^\d{4}-\d{2}-\d{2}$/.test(dateStr)) return `${dateStr}T00:00:00Z`
  const m = dateStr.match(/^(\d{4})[./-](\d{2})[./-](\d{2})$/)
  if (m) return `${m[1]}-${m[2]}-${m[3]}T00:00:00Z`
  if (/Z$/.test(dateStr)) return dateStr
  const d = new Date(dateStr)
  if (!isNaN(d.getTime())) return new Date(Date.UTC(d.getFullYear(), d.getMonth(), d.getDate())).toISOString()
  return '2025-01-01T00:00:00Z'
}

function shiftDay(ts, delta) {
  const d = new Date(ts)
  const nd = new Date(Date.UTC(d.getUTCFullYear(), d.getUTCMonth(), d.getUTCDate() + delta))
  return nd.toISOString()
}

describe('Dynamic link switches to existing list item via Prev/Next buttons', () => {
  beforeEach(() => setupDOM())

  it('navigates to 2025-08-25 and shows Tuesday from History', async () => {
    const deepClone = (o) => JSON.parse(JSON.stringify(o))
    let currentDoc = null
    const { invoke } = await import('@tauri-apps/api/core')

    // Backend mock: handle button click events by updating Selected/selected_date
    invoke.mockImplementation((cmd, args) => {
      if (cmd === 'serialize_overseer_nodes') { currentDoc = deepClone(args.nodes); return Promise.resolve('DOC') }
      if (cmd === 'get_next_timer_due_ms') return Promise.resolve(null)
      if (cmd === 'scheduler_tick') return Promise.resolve(null)
      if (cmd === 'parse_overseer_content') return Promise.resolve(deepClone(currentDoc))
      if (cmd === 'parse_overseer_content_selective') return Promise.resolve(deepClone(currentDoc))
      if (cmd === 'execute_overseer_event') {
        const nodes = deepClone(args.nodes)
        // Resolve clicked node by path
        const find = (nodesArr, parts) => {
          let cur = { children: nodesArr }
          for (const seg of parts) {
            const [b,o] = seg.includes('#')?seg.split('#'):[seg,'0']
            const ms = (cur.children||[]).filter(c=>c && c.name===b)
            const idx = parseInt(o,10)
            if (idx>=ms.length) return null
            cur = ms[idx]
          }
          return cur
        }
        const path = args.nodePath || []
        const clicked = find(nodes, path)
        // Locate Selected/selected_date
        const weight = nodes.find(n=>n.name==='weight_minimal')
        const selected = weight.children.find(n=>n.name==='Selected')
        const dateNode = selected.children.find(n=>n.name==='selected_date')
        const ts = (dateNode.parameters?.value?.Timestamp) || (dateNode.parameters?.value?.String) || (dateNode.parameters?._computed_value?.Timestamp) || '2025-08-24T00:00:00Z'
        // Decide delta by button name
        const name = clicked?.name || ''
        const delta = name.toLowerCase().includes('next') ? +1 : (name.toLowerCase().includes('prev') ? -1 : 0)
        const newTs = shiftDay(toIsoDay(ts), delta)
        dateNode.parameters = Object.assign({}, dateNode.parameters || {}, { value: { Timestamp: newTs }, precision: { String: 'day' } })
        return Promise.resolve(nodes)
      }
      return Promise.reject(new Error('unknown command: ' + cmd))
    })

    const app = new OverseerApp()
    const renderer = app.renderer

    // Build document mirroring examples/weight_tracker/weight_tracker_new.os essentials
    currentDoc = [
      { name: 'WeightRecord', node_type: 'div', parameters: {}, children: [
        { name: 'date', node_type: 'timestamp', parameters: { value: { Timestamp: '2025-08-24T00:00:00Z' }, precision: { String: 'day' } }, children: [] },
        { name: 'test_data', node_type: 'string', parameters: { value: { String: 'test' } }, children: [] }
      ]},
      { name: 'weight_minimal', node_type: 'tab', parameters: {}, children: [
        { name: 'Selected', node_type: 'div', parameters: {}, children: [
          { name: 'selected_date', node_type: 'timestamp', parameters: { value: { Timestamp: '2025-08-24T00:00:00Z' }, precision: { String: 'day' } }, children: [] },
          { name: 'SelectedWeightRecord', node_type: 'div', parameters: { link: { String: '/weight_minimal/History[key=$(../selected_date)]' } }, children: [] },
          { name: 'Prev', node_type: 'button', parameters: { label: { String: '< Prev Day' } }, children: [ { name: 'click', node_type: 'on', parameters: {}, children: [ { name: 'set', node_type: 'set', parameters: {}, children: [] } ] } ] },
          { name: 'Next', node_type: 'button', parameters: { label: { String: '> Next Day' } }, children: [ { name: 'click', node_type: 'on', parameters: {}, children: [ { name: 'set', node_type: 'set', parameters: {}, children: [] } ] } ] }
        ]},
        { name: 'History', node_type: 'list', parameters: { entry: { Template: 'WeightRecord' }, key: { String: 'date' }, keyPrecision: { String: 'day' } }, children: [
          { name: 'WeightRecord__1', node_type: 'div', parameters: {}, children: [
            { name: 'date', node_type: 'string', parameters: { value: { String: '2025.08.24' } }, children: [] },
            { name: 'test_data', node_type: 'string', parameters: { value: { String: 'Monday' } }, children: [] }
          ]},
          { name: 'WeightRecord__2', node_type: 'div', parameters: {}, children: [
            { name: 'date', node_type: 'string', parameters: { value: { String: '2025.08.25' } }, children: [] },
            { name: 'test_data', node_type: 'string', parameters: { value: { String: 'Tuesday' } }, children: [] }
          ]},
          { name: 'WeightRecord__3', node_type: 'div', parameters: {}, children: [
            { name: 'date', node_type: 'string', parameters: { value: { String: '2025.08.26' } }, children: [] },
            { name: 'test_data', node_type: 'string', parameters: { value: { String: 'Wednesday' } }, children: [] }
          ]}
        ] }
      ]}
    ]

    app.currentDocument = deepClone(currentDoc)
    renderer.renderDocument(app.currentDocument)

  const linkContainer = findElementByPath(['weight_minimal','Selected','SelectedWeightRecord'])
  expect(linkContainer).toBeTruthy()
  // Initial: 2025-08-24 matches History[date=2025.08.24] with keyPrecision='day' normalization, so not phantom
  expect(linkContainer.hasAttribute('data-link-phantom')).toBe(false)
    // Read the 'test_data' string specifically from the linked subtree
    const getLinkedStringByName = (container, name) => {
      const candidates = Array.from(container.querySelectorAll('.string-field'))
      for (const el of candidates) {
        const holder = el.closest('[data-path]')
        if (!holder) continue
        try {
          const p = JSON.parse(holder.dataset.path || '[]')
          if (Array.isArray(p) && p[p.length - 1] === name) {
            return (el.querySelector('.field-value') || {}).textContent || ''
          }
        } catch (_) {}
      }
      return ''
    }
    const initialStr = getLinkedStringByName(linkContainer, 'test_data')
    expect(initialStr).toBe('Monday')

    // Click Next to go to 2025-08-25
    const nextBtn = findElementByPath(['weight_minimal','Selected','Next'])
    expect(nextBtn).toBeTruthy()
    nextBtn.dispatchEvent(new Event('click'))
    await new Promise(r => setTimeout(r, 0))

    // After navigation, link should bind to existing History item (not phantom)
    const lc2 = findElementByPath(['weight_minimal','Selected','SelectedWeightRecord'])
    expect(lc2).toBeTruthy()
    expect(lc2.hasAttribute('data-link-phantom')).toBe(false)
  const val2 = getLinkedStringByName(lc2, 'test_data')
  expect(val2).toBe('Tuesday')
  })
})
