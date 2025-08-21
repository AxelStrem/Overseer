import { describe, it, expect, beforeEach } from 'vitest'

// Minimal DOM container expected by renderer
function setupDOM() {
  document.body.innerHTML = `
    <div id="app">
      <div id="toolbar">
        <button id="open-file-btn"></button>
        <button id="new-file-btn"></button>
        <button id="save-file-btn"></button>
        <button id="reload-file-btn"></button>
      </div>
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

// Build a minimal resolved document representing examples/basic/list_corruption.os
// Before edit: list L has two items resolved from template T -> instances T__1, T__2 with bg color inherited
function buildResolvedBefore() {
  return [
    {
      name: 'T', node_type: 'div', parameters: { 'background-color': { Color: { Hex: '#000000ff' } }, _effective_layout: { String: 'horizontal' } }, children: [
        { name: 'f', node_type: 'int', parameters: { value: { Integer: 0 } }, children: [] }
      ]
    },
    {
      name: 'L', node_type: 'list', parameters: { entry: { Template: 'T' }, _effective_layout: { String: 'horizontal' } }, children: [
        // Instances of template name as type, with _original_type=div, children cloned
        { name: 'T__1', node_type: 'T', parameters: { _original_type: { String: 'div' } }, children: [
          { name: 'f', node_type: 'int', parameters: { value: { Integer: 2 } }, children: [] }
        ]},
        { name: 'T__2', node_type: 'T', parameters: { _original_type: { String: 'div' } }, children: [
          { name: 'f', node_type: 'int', parameters: { value: { Integer: 3 } }, children: [] }
        ]}
      ]
    }
  ]
}

// After edit (buggy): backend selective result only updated field value but list item nodes came back as generic '-' items
function buildSelectivelyResolvedAfterBuggy() {
  return [
    {
      name: 'T', node_type: 'div', parameters: { 'background-color': { Color: { Hex: '#000000ff' } }, _effective_layout: { String: 'horizontal' } }, children: [
        { name: 'f', node_type: 'int', parameters: { value: { Integer: 0 } }, children: [] }
      ]
    },
    {
      name: 'L', node_type: 'list', parameters: { entry: { Template: 'T' }, _effective_layout: { String: 'horizontal' } }, children: [
        { name: '-', node_type: '-', parameters: { value: { Integer: 2 } }, children: [] },
        { name: '-#1', node_type: '-', parameters: { value: { Integer: 3 } }, children: [] }
      ]
    }
  ]
}

// Load the actual renderer module
import path from 'path'
import { fileURLToPath } from 'url'
const __filename = fileURLToPath(import.meta.url)
const __dirname = path.dirname(__filename)

// Use relative path from tests to src/renderer.js
import { OverseerRenderer } from '../src/renderer.js'

// Provide a minimal window.app for renderer hooks
function setupApp(renderer, doc) {
  global.window.app = {
    currentDocument: doc,
    renderer,
    markDocumentModified: () => {},
    startScheduler: () => {},
  }
}

describe('List corruption selective update regression', () => {
  beforeEach(() => setupDOM())

  it('keeps list items templated after editing a child field (no corruption)', () => {
    const renderer = new OverseerRenderer()
    const before = buildResolvedBefore()
    setupApp(renderer, before)

    // Initial full render
    renderer.renderDocument(before)
    const htmlBefore = document.getElementById('content-display').innerHTML

    // Simulate selective backend result after editing L/T__1/f
    const afterBuggy = buildSelectivelyResolvedAfterBuggy()

    // Attempt selective DOM update for the edited field path
    const changed = ['L/T__1/f']
    const ok = renderer.updateSelectiveFields(before, afterBuggy, changed, [{ path: 'L/T__1/f', oldValue: '2', newValue: '2' }])

    // The update should not corrupt DOM into generic '-' items
    const htmlAfter = document.getElementById('content-display').innerHTML

    // Expect the DOM to still contain T__1 and T__2 instances and not generic overseer-list-item wrappers only
    expect(ok).toBe(true)
    expect(htmlAfter).toContain('data-name="T__1"')
    expect(htmlAfter).toContain('data-name="T__2"')
    expect(htmlAfter).not.toContain('data-path="[\"L\",\"-\"]"')

    // Assert snapshot-style: container class for list stays layout-horizontal
    expect(htmlAfter).toContain('overseer-list layout-horizontal')

    // If the implementation is buggy, this test will fail, flagging the regression
  })
})
