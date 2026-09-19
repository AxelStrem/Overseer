import { describe, it, expect, beforeEach } from 'vitest'
import { OverseerApp } from '../src/main.js'

// Minimal DOM setup
function setupDOM(){
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
  </div>`
}

// Build a simple document with a markdown text node using new color syntax
function buildDoc(){
  return [
    { name:'main', node_type:'tab', parameters:{}, is_hierarchy_transparent:true, children:[
      { name:'Intro', node_type:'text', parameters:{ value:{ String:'This is <color=#FF0000 | red> and **<color=blue | bold blue>** text.' }, markdown:{ Boolean:true } }, children:[], is_hierarchy_transparent:false }
    ]}
  ]
}

// Helper to fetch rendered HTML for the text node
function getRenderedHTML(){
  const el = document.querySelector('[data-path] .field-value, [data-path].field-value')
  return el ? el.innerHTML : ''
}

describe('Markdown inline color syntax', () => {
  beforeEach(setupDOM)
  it('renders <color=... | ...> segments as span with inline style and preserves markdown inside', () => {
    const app = new OverseerApp()
    app.currentDocument = buildDoc()
    app.renderer.renderDocument(app.currentDocument)
    const html = getRenderedHTML()
    // Expect two colored spans
    expect(html).toMatch(/<span class=\"md-inline-color\" style=\"color:#FF0000\">red<\/span>/)
    expect(html).toMatch(/<span class=\"md-inline-color\" style=\"color:blue\">bold blue<\/span>/)
    // Bold nested inside second span should still be processed by markdown (marked wraps in <strong>)
    // Because we preprocess before markdown, the ** are outside color span; verify strong appears
    expect(html).toMatch(/<strong>.*<span class=\"md-inline-color\" style=\"color:blue\">bold blue<\/span>.*<\/strong>|<span class=\"md-inline-color\" style=\"color:blue\"><strong>bold blue<\/strong><\/span>/)
  })

  it('silently strips unsafe color formats, leaving plain text', () => {
    const app = new OverseerApp()
    const unsafe = 'Testing <color=javascript:alert(1) | exploit> end.'
    app.currentDocument = [ { name:'main', node_type:'tab', parameters:{}, is_hierarchy_transparent:true, children:[ { name:'T', node_type:'text', parameters:{ value:{ String: unsafe }, markdown:{ Boolean:true } }, children:[], is_hierarchy_transparent:false } ] } ]
    app.renderer.renderDocument(app.currentDocument)
    const html = getRenderedHTML()
    expect(html).toContain('exploit')
    expect(html).not.toContain('javascript:alert')
    expect(html).not.toMatch(/md-inline-color/)
  })
})
