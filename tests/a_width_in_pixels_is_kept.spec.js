import { describe, it, expect, beforeEach, vi } from 'vitest'
import { readFileSync } from 'node:fs'

// A width in pixels is a size the document asked for. In a row it was once squeezed like any
// other child, so a row of fixed-width columns given a container that scrolls sideways shrank to
// the page instead of scrolling. A width in percent is a share of the row and still shrinks.
// An empty list that takes drops keeps some height, or there is nothing to drop onto.

function setupDOM() {
  document.body.innerHTML = `
    <div id="app">
      <div id="toolbar"><button id="open-file-btn"></button><button id="new-file-btn"></button>
        </div>
      <button id="welcome-open-btn"></button><button id="welcome-new-btn"></button>
      <button id="error-back-btn"></button>
      <div id="tab-container"></div><div id="content-display"></div>
      <div id="status-bar"><span id="status-message"></span><span id="status-info"></span>
        <span id="file-path"></span></div>
      <div id="welcome-screen" class="screen"></div><div id="editor-screen" class="screen"></div>
      <div id="error-screen" class="screen"></div><div id="error-message"></div>
    </div>`
}

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(async () => null) }))
import { OverseerApp } from '../src/main.js'

const line = (name) => ({
  name, node_type: 'text', children: [], is_hierarchy_transparent: false,
  parameters: { value: { String: name } },
})

const box = (name, parameters) => ({
  name, node_type: 'div', children: [line(name + '_a')], is_hierarchy_transparent: false, parameters,
})

const render = (...nodes) => {
  const app = new OverseerApp()
  app.currentDocument = [{
    name: 't', node_type: 'tab', parameters: {}, is_hierarchy_transparent: false,
    children: [box('row', { layout: { String: 'horizontal' } })],
  }]
  app.currentDocument[0].children[0].children = nodes
  app.renderer.renderDocument(app.currentDocument)
  return app
}

const drawn = (name) => document.querySelector(`[data-path='${JSON.stringify(['t', 'row', name])}']`)
const styles = readFileSync('src/styles.css', 'utf8').replace(/\/\*[\s\S]*?\*\//g, '')

describe('a width in pixels', () => {
  beforeEach(() => setupDOM())

  it('is kept in a row, where a share of the row or no width is not', () => {
    render(
      box('fixed', { width: { CssSize: { Pixels: 820 } } }),
      box('share', { width: { CssSize: { Percentage: 40 } } }),
      box('fit', { width: { CssSize: 'FitContent' } }),
      box('free', {}),
    )
    expect(drawn('fixed').style.width).toBe('820px')
    expect(drawn('fixed').classList.contains('keeps-width')).toBe(true)
    for (const name of ['share', 'fit', 'free']) {
      expect(drawn(name).classList.contains('keeps-width')).toBe(false)
    }
  })

  it('is not shrunk by the stylesheet', () => {
    expect(styles).toMatch(/\.layout-horizontal > \.keeps-width[^{]*\{\s*flex-shrink: 0;/)
  })
})

describe('an empty list that takes drops', () => {
  it('keeps a height to drop onto', () => {
    expect(styles).toMatch(/\.overseer-list\[data-movable="1"\] \{\s*min-height: \d+px;/)
  })
})
