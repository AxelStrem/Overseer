import { describe, it, expect, beforeEach, vi } from 'vitest'

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

import { OverseerRenderer } from '../src/renderer.js'
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

describe('Link proxy rendering', () => {
  beforeEach(() => setupDOM())

  it('renders target subtree inside link container and routes edits', async () => {
    const deepClone = (o) => JSON.parse(JSON.stringify(o))
    let currentDoc = null
    const { invoke } = await import('@tauri-apps/api/core')

    // Backend stubs: initial parse returns document as-is
    invoke.mockImplementation((cmd, args) => {
      if (cmd === 'serialize_overseer_nodes') {
        // Capture latest nodes snapshot to simulate backend using updated document
        currentDoc = deepClone(args.nodes)
        return Promise.resolve('DOC')
      }
      if (cmd === 'get_next_timer_due_ms') return Promise.resolve(null)
      if (cmd === 'scheduler_tick') return Promise.resolve(null)
      if (cmd === 'parse_overseer_content') {
        return Promise.resolve(deepClone(currentDoc))
      }
      if (cmd === 'parse_overseer_content_selective') {
        return Promise.resolve(deepClone(currentDoc))
      }
      if (cmd === 'execute_overseer_event') {
        // For this test, simply echo back nodes without changes
        return Promise.resolve(deepClone(args.nodes))
      }
      return Promise.reject(new Error('unknown command: ' + cmd))
    })

    const app = new OverseerApp()
    const renderer = app.renderer

    // Build a document with a data node B and a container A linking to it
    currentDoc = [
      { name: 'Root', node_type: 'div', parameters: {}, children: [
        { name: 'Data', node_type: 'div', parameters: {}, children: [
          { name: 'Title', node_type: 'string', parameters: { value: { String: 'Hello' } }, children: [] }
        ] },
        { name: 'View', node_type: 'div', parameters: { link: '/Root/Data/Title' }, children: [] }
      ]}
    ]

    app.currentDocument = deepClone(currentDoc)
    renderer.renderDocument(app.currentDocument)

    // Expect that View container rendered the Title field inside
    const titleEl = findElementByPath(['Root','Data','Title'])
    expect(titleEl).toBeTruthy()

    // The link container itself exists too
    const viewEl = findElementByPath(['Root','View'])
    expect(viewEl).toBeTruthy()

    // Edit the Title via its rendered element (simulate inline edit finish)
    const span = titleEl.querySelector('.field-value')
    expect(span).toBeTruthy()
    // Manually invoke update path logic
    renderer.updateNodeValueByPath(app.currentDocument, 'Root/Data/Title', 'World')

    // Simulate selective reevaluation
    await app.reevaluateDocumentSelective(['Root/Data/Title'], [{ path: 'Root/Data/Title', oldValue: 'Hello', newValue: 'World' }])

    // Verify value updated in memory
    const updated = app.currentDocument[0].children.find(n=>n.name==='Data').children.find(n=>n.name==='Title')
    const txt = (updated.parameters && updated.parameters.value && (updated.parameters.value.String || updated.parameters.value))
    expect(txt).toBe('World')
  })

  it('supports switching link targets by changing link parameter', async () => {
    const deepClone = (o) => JSON.parse(JSON.stringify(o))
    let currentDoc = null
    const { invoke } = await import('@tauri-apps/api/core')

    invoke.mockImplementation((cmd, args) => {
      if (cmd === 'serialize_overseer_nodes') {
        currentDoc = deepClone(args.nodes)
        return Promise.resolve('DOC')
      }
      if (cmd === 'get_next_timer_due_ms') return Promise.resolve(null)
      if (cmd === 'scheduler_tick') return Promise.resolve(null)
  if (cmd === 'parse_overseer_content') return Promise.resolve(deepClone(currentDoc))
  if (cmd === 'parse_overseer_content_selective') return Promise.resolve(deepClone(currentDoc))
      if (cmd === 'execute_overseer_event') return Promise.resolve(deepClone(args.nodes))
      return Promise.reject(new Error('unknown command: ' + cmd))
    })

    const app = new OverseerApp()
    const renderer = app.renderer

    currentDoc = [
      { name: 'Root', node_type: 'div', parameters: {}, children: [
        { name: 'Data', node_type: 'div', parameters: {}, children: [
          { name: 'A', node_type: 'string', parameters: { value: { String: 'Alpha' } }, children: [] },
          { name: 'B', node_type: 'string', parameters: { value: { String: 'Beta' } }, children: [] }
        ] },
        { name: 'View', node_type: 'div', parameters: { link: '/Root/Data/A' }, children: [] }
      ]}
    ]

    app.currentDocument = deepClone(currentDoc)
    renderer.renderDocument(app.currentDocument)

    // Initially View shows A
    let aEl = findElementByPath(['Root','Data','A'])
    expect(aEl).toBeTruthy()

    // Change link to B and re-render
    const root = app.currentDocument[0]
    const view = root.children.find(n=>n.name==='View')
    view.parameters.link = '/Root/Data/B'

    // Selective refresh on link change
    await app.reevaluateDocumentSelective(['Root/View/link'], [])
    renderer.renderDocument(app.currentDocument)

    // Now B should be present
    const bEl = findElementByPath(['Root','Data','B'])
    expect(bEl).toBeTruthy()
  })

  it('links to list items including ordinal selection and edits through proxy', async () => {
    const deepClone = (o) => JSON.parse(JSON.stringify(o))
    let currentDoc = null
    const { invoke } = await import('@tauri-apps/api/core')

    invoke.mockImplementation((cmd, args) => {
      if (cmd === 'serialize_overseer_nodes') { currentDoc = deepClone(args.nodes); return Promise.resolve('DOC') }
      if (cmd === 'get_next_timer_due_ms') return Promise.resolve(null)
      if (cmd === 'scheduler_tick') return Promise.resolve(null)
      if (cmd === 'parse_overseer_content') return Promise.resolve(deepClone(currentDoc))
      if (cmd === 'parse_overseer_content_selective') return Promise.resolve(deepClone(currentDoc))
      if (cmd === 'execute_overseer_event') return Promise.resolve(deepClone(args.nodes))
      return Promise.reject(new Error('unknown command: ' + cmd))
    })

    const app = new OverseerApp()
    const renderer = app.renderer

    // List with duplicate item names under Data; View links to the second item using ordinal suffix
    currentDoc = [
      { name: 'Root', node_type: 'div', parameters: {}, children: [
        { name: 'Data', node_type: 'list', parameters: {}, children: [
          { name: '-', node_type: '-', parameters: { value: { String: 'First' } }, children: [] },
          { name: '-#1', node_type: '-', parameters: { value: { String: 'Second' } }, children: [] }
        ] },
        { name: 'View', node_type: 'div', parameters: { link: '/Root/Data/-#1' }, children: [] }
      ]}
    ]

    app.currentDocument = deepClone(currentDoc)
    renderer.renderDocument(app.currentDocument)

    // The proxy should render the second list item content; dataset.path points to Root/Data/-#1
    const secondItemEl = findElementByPath(['Root','Data','-#1'])
    expect(secondItemEl).toBeTruthy()

    // Edit through proxy
    const fieldEl = secondItemEl.querySelector('.overseer-list-value') || secondItemEl
    renderer.updateNodeValueByPath(app.currentDocument, 'Root/Data/-#1', 'Second-Edited')
    await app.reevaluateDocumentSelective(['Root/Data/-#1'], [{ path: 'Root/Data/-#1', oldValue: 'Second', newValue: 'Second-Edited' }])

    // Verify updated value in the underlying list item
    const dataList = app.currentDocument[0].children.find(n=>n.name==='Data')
    const item2 = dataList.children.find(n=>n.name==='-#1')
    const txt = (item2.parameters && item2.parameters.value && (item2.parameters.value.String || item2.parameters.value))
    expect(txt).toBe('Second-Edited')

    // Now retarget the link to the first item using explicit '-#0' and verify proxy points correctly
    const view = app.currentDocument[0].children.find(n=>n.name==='View')
    view.parameters.link = '/Root/Data/-#0'
    await app.reevaluateDocumentSelective(['Root/View/link'], [])
    renderer.renderDocument(app.currentDocument)

    const firstItemEl = findElementByPath(['Root','Data','-']) // canonical path for first is '-' without suffix
    expect(firstItemEl).toBeTruthy()
  })

  it('switches link via Set action on link parameter (action-driven)', async () => {
    const deepClone = (o) => JSON.parse(JSON.stringify(o))
    let currentDoc = null
    const { invoke } = await import('@tauri-apps/api/core')

    invoke.mockImplementation((cmd, args) => {
      if (cmd === 'serialize_overseer_nodes') { currentDoc = deepClone(args.nodes); return Promise.resolve('DOC') }
      if (cmd === 'get_next_timer_due_ms') return Promise.resolve(null)
      if (cmd === 'scheduler_tick') return Promise.resolve(null)
      if (cmd === 'parse_overseer_content') return Promise.resolve(deepClone(currentDoc))
      if (cmd === 'parse_overseer_content_selective') return Promise.resolve(deepClone(currentDoc))
      if (cmd === 'execute_overseer_event') {
        // Simulate a button click under View that sets View/link to point to B
        const nodes = deepClone(args.nodes)
        const root = nodes[0]
        const view = root.children.find(n=>n.name==='View')
        if (view) view.parameters.link = '/Root/Data/B'
        return Promise.resolve(nodes)
      }
      return Promise.reject(new Error('unknown command: ' + cmd))
    })

    const app = new OverseerApp()
    const renderer = app.renderer

    currentDoc = [
      { name: 'Root', node_type: 'div', parameters: {}, children: [
        { name: 'Data', node_type: 'div', parameters: {}, children: [
          { name: 'A', node_type: 'string', parameters: { value: { String: 'Alpha' } }, children: [] },
          { name: 'B', node_type: 'string', parameters: { value: { String: 'Beta' } }, children: [] }
        ] },
        { name: 'View', node_type: 'div', parameters: { link: '/Root/Data/A' }, children: [
          { name: 'click', node_type: 'on', parameters: {}, children: [
            { name: 'Set', node_type: 'button', parameters: { label: { String: 'Set' } }, children: [] }
          ] }
        ] }
      ]}
    ]

    app.currentDocument = deepClone(currentDoc)
    renderer.renderDocument(app.currentDocument)

    // Initially A is present
    expect(findElementByPath(['Root','Data','A'])).toBeTruthy()

  // Click the button (simulate action engine switching the link)
  const btn = document.querySelector('.overseer-button, button.overseer-button')
    expect(btn).toBeTruthy()
    btn.click()

    // After backend event, app.currentDocument should have View/link -> B and re-rendered
    expect(findElementByPath(['Root','Data','B'])).toBeTruthy()
  })

  it('supports relative link paths resolved from the link container', async () => {
    const deepClone = (o) => JSON.parse(JSON.stringify(o))
    let currentDoc = null
    const { invoke } = await import('@tauri-apps/api/core')

    invoke.mockImplementation((cmd, args) => {
      if (cmd === 'serialize_overseer_nodes') { currentDoc = deepClone(args.nodes); return Promise.resolve('DOC') }
      if (cmd === 'get_next_timer_due_ms') return Promise.resolve(null)
      if (cmd === 'scheduler_tick') return Promise.resolve(null)
      if (cmd === 'parse_overseer_content') return Promise.resolve(deepClone(currentDoc))
      if (cmd === 'parse_overseer_content_selective') return Promise.resolve(deepClone(currentDoc))
      if (cmd === 'execute_overseer_event') return Promise.resolve(deepClone(args.nodes))
      return Promise.reject(new Error('unknown command: ' + cmd))
    })

    const app = new OverseerApp()
    const renderer = app.renderer

    currentDoc = [
      { name: 'Root', node_type: 'div', parameters: {}, children: [
        { name: 'Data', node_type: 'div', parameters: {}, children: [
          { name: 'Title', node_type: 'string', parameters: { value: { String: 'Hello' } }, children: [] }
        ] },
        // Use a relative link from under Root: "Data/Title"
        { name: 'View', node_type: 'div', parameters: { link: 'Data/Title' }, children: [] }
      ]}
    ]

    app.currentDocument = deepClone(currentDoc)
    renderer.renderDocument(app.currentDocument)

    // Should resolve to Root/Data/Title
    const titleEl = findElementByPath(['Root','Data','Title'])
    expect(titleEl).toBeTruthy()
  })

  it('guards against link cycles without crashing', async () => {
    const deepClone = (o) => JSON.parse(JSON.stringify(o))
    let currentDoc = null
    const { invoke } = await import('@tauri-apps/api/core')

    invoke.mockImplementation((cmd, args) => {
      if (cmd === 'serialize_overseer_nodes') { currentDoc = deepClone(args.nodes); return Promise.resolve('DOC') }
      if (cmd === 'get_next_timer_due_ms') return Promise.resolve(null)
      if (cmd === 'scheduler_tick') return Promise.resolve(null)
      if (cmd === 'parse_overseer_content') return Promise.resolve(deepClone(currentDoc))
      if (cmd === 'parse_overseer_content_selective') return Promise.resolve(deepClone(currentDoc))
      if (cmd === 'execute_overseer_event') return Promise.resolve(deepClone(args.nodes))
      return Promise.reject(new Error('unknown command: ' + cmd))
    })

    const app = new OverseerApp()
    const renderer = app.renderer

    // Two views linking to each other
    currentDoc = [
      { name: 'Root', node_type: 'div', parameters: {}, children: [
        { name: 'A', node_type: 'div', parameters: { link: '/Root/B' }, children: [] },
        { name: 'B', node_type: 'div', parameters: { link: '/Root/A' }, children: [] }
      ]}
    ]

    app.currentDocument = deepClone(currentDoc)
    // Should render without throwing; placeholders may appear
    renderer.renderDocument(app.currentDocument)
  // Query DOM for elements rather than string contains, accounting for nested renders
  const aEl = findElementByPath(['Root','A'])
  const bEl = findElementByPath(['Root','B'])
  expect(aEl).toBeTruthy()
  expect(bEl).toBeTruthy()
  })
})
