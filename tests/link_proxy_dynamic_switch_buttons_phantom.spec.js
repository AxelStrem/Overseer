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

vi.mock('@tauri-apps/api/tauri', () => ({ invoke: vi.fn() }))
import { OverseerApp } from '../src/main.js'

function findElementByPath(pathArray){
  const all = Array.from(document.querySelectorAll('[data-path]'))
  for (const el of all) {
    try { const p = JSON.parse(el.dataset.path||'[]'); if (Array.isArray(p) && p.length===pathArray.length && p.every((v,i)=>v===pathArray[i])) return el } catch(_){}}
  return null
}

function toIsoDay(dateStr){
  if (/^\d{4}-\d{2}-\d{2}$/.test(dateStr)) return `${dateStr}T00:00:00Z`
  const m = dateStr.match(/^(\d{4})[.\/-](\d{2})[.\/-](\d{2})$/)
  if (m) return `${m[1]}-${m[2]}-${m[3]}T00:00:00Z`
  if (/Z$/.test(dateStr)) return dateStr
  const d = new Date(dateStr); if (!isNaN(d.getTime())) return new Date(Date.UTC(d.getUTCFullYear(), d.getUTCMonth(), d.getUTCDate())).toISOString()
  return '2025-01-01T00:00:00Z'
}
function shiftDay(ts, delta){ const d=new Date(ts); const nd=new Date(Date.UTC(d.getUTCFullYear(), d.getUTCMonth(), d.getUTCDate()+delta)); return nd.toISOString() }
function normDayString(s){ const m = (s||'').toString().match(/(\d{4})[.\/-](\d{2})[.\/-](\d{2})/); return m?`${m[1]}.${m[2]}.${m[3]}`:'' }

// Minimal doc mirroring the example
function buildDoc(){
  return [
    { name: 'WeightRecord', node_type: 'div', parameters: {}, children: [
      { name: 'date', node_type: 'timestamp', parameters: { value: { Timestamp: '2025-08-24T00:00:00Z' }, precision: { String: 'day' } }, children: [] },
      { name: 'test_data', node_type: 'string', parameters: { value: { String: 'test' } }, children: [] }
    ]},
    { name: 'weight_minimal', node_type: 'tab', parameters: {}, children: [
      { name: 'Selected', node_type: 'div', parameters: {}, children: [
        { name: 'selected_date', node_type: 'timestamp', parameters: { value: { Timestamp: '2025-08-26T00:00:00Z' }, precision: { String: 'day' } }, children: [] },
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
}

describe('Dynamic link via Prev/Next shows phantom for missing dates', () => {
  beforeEach(() => setupDOM())

  it('navigates to a missing date and shows phantom with matching date', async () => {
    const { invoke } = await import('@tauri-apps/api/tauri')
    const deepClone = (o) => JSON.parse(JSON.stringify(o))

    let currentDoc = buildDoc()

    // Mock backend to shift selected_date on button clicks
    invoke.mockImplementation((cmd, args) => {
      if (cmd === 'serialize_overseer_nodes') { currentDoc = deepClone(args.nodes); return Promise.resolve('DOC') }
      if (cmd === 'get_next_timer_due_ms') return Promise.resolve(null)
      if (cmd === 'scheduler_tick') return Promise.resolve(null)
      if (cmd === 'parse_overseer_content') return Promise.resolve(deepClone(currentDoc))
      if (cmd === 'parse_overseer_content_selective') return Promise.resolve(deepClone(currentDoc))
      if (cmd === 'execute_overseer_event') {
        const nodes = deepClone(args.nodes)
        const find = (nodesArr, parts) => { let cur = { children: nodesArr }; for (const seg of parts){ const [b,o]=seg.includes('#')?seg.split('#'):[seg,'0']; const ms=(cur.children||[]).filter(c=>c && c.name===b); const idx=parseInt(o,10); if (idx>=ms.length) return null; cur=ms[idx] } return cur }
        const clicked = find(nodes, args.nodePath||[])
        const root = nodes.find(n=>n.name==='weight_minimal')
        const selected = root.children.find(n=>n.name==='Selected')
        const dateNode = selected.children.find(n=>n.name==='selected_date')
        const ts = (dateNode.parameters?.value?.Timestamp) || (dateNode.parameters?.value?.String) || '2025-08-26T00:00:00Z'
        const name = (clicked?.name||'').toLowerCase(); const delta = name.includes('next')?+1:(name.includes('prev')?-1:0)
        const newTs = shiftDay(toIsoDay(ts), delta)
        dateNode.parameters = Object.assign({}, dateNode.parameters||{}, { value: { Timestamp: newTs }, precision: { String: 'day' } })
        return Promise.resolve(nodes)
      }
      return Promise.reject(new Error('unknown cmd '+cmd))
    })

    const app = new OverseerApp()
    app.currentDocument = deepClone(currentDoc)
    app.renderer.renderDocument(app.currentDocument)

    const linkPath = ['weight_minimal','Selected','SelectedWeightRecord']
    const dateText = () => {
      const container = findElementByPath(linkPath)
      const all = Array.from(container.querySelectorAll('[data-path]'))
      for (const el of all){
        try { const p=JSON.parse(el.dataset.path||'[]'); if (Array.isArray(p) && p[p.length-1]==='date'){ const v=el.querySelector('.field-value, .text-content')||el; return (v.textContent||'').trim() } } catch(_){}
      }
      return ''
    }

    // Initial (selected_date=2025-08-26): binds existing History item, not phantom
    const linkEl = findElementByPath(linkPath)
    expect(linkEl).toBeTruthy()
    expect(linkEl.hasAttribute('data-link-phantom')).toBe(false)
    expect(normDayString(dateText())).toBe('2025.08.26')

    // Click Next -> 2025-08-27 (missing). Should show phantom and date updates.
    const nextBtn = findElementByPath(['weight_minimal','Selected','Next'])
    expect(nextBtn).toBeTruthy()
    nextBtn.dispatchEvent(new Event('click'))
    await new Promise(r=>setTimeout(r,0))

    const linkEl2 = findElementByPath(linkPath)
    expect(linkEl2).toBeTruthy()
    expect(linkEl2.hasAttribute('data-link-phantom')).toBe(true)
    expect(normDayString(dateText())).toBe('2025.08.27')

    // Click Next again -> 2025-08-28 (still missing), stays phantom and updates date
    nextBtn.dispatchEvent(new Event('click'))
    await new Promise(r=>setTimeout(r,0))
    const linkEl3 = findElementByPath(linkPath)
    expect(linkEl3).toBeTruthy()
    expect(linkEl3.hasAttribute('data-link-phantom')).toBe(true)
    expect(normDayString(dateText())).toBe('2025.08.28')

    // Click Prev -> back to 2025-08-27 (missing), still phantom
    const prevBtn = findElementByPath(['weight_minimal','Selected','Prev'])
    expect(prevBtn).toBeTruthy()
    prevBtn.dispatchEvent(new Event('click'))
    await new Promise(r=>setTimeout(r,0))
    const linkEl4 = findElementByPath(linkPath)
    expect(linkEl4.hasAttribute('data-link-phantom')).toBe(true)
    expect(normDayString(dateText())).toBe('2025.08.27')

    // Click Prev -> back to 2025-08-26 (existing), binds real item and clears phantom flag
    prevBtn.dispatchEvent(new Event('click'))
    await new Promise(r=>setTimeout(r,0))
    const linkEl5 = findElementByPath(linkPath)
    expect(linkEl5.hasAttribute('data-link-phantom')).toBe(false)
    expect(normDayString(dateText())).toBe('2025.08.26')
  })
})
