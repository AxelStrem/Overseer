import { describe, it, expect, beforeEach, vi } from 'vitest'
import { readFileSync } from 'node:fs'

// A height in parts (`1fr`) is a share of the height a column has left once its other children are
// drawn, as a width in parts is of a row's; at the top of a document the column is the window, so
// a document can fill the page down as it already does across. jsdom lays nothing out: what is
// asserted is what the browser is given.

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

const base = (name, node_type, parameters = {}, children = []) => ({
  name, node_type, parameters, children, is_hierarchy_transparent: false,
})
const line = (name) => base(name, 'text', { value: { String: name } })
const box = (name, parameters) => base(name, 'div', parameters, [line(name + '_a')])

const render = (doc) => {
  const app = new OverseerApp()
  app.currentDocument = doc
  app.renderer._filters = new Map()
  app.renderer.renderDocument(app.currentDocument)
  return app
}

const drawn = (path) => document.querySelector(`[data-path='${JSON.stringify(path)}']`)
const styles = readFileSync('src/styles.css', 'utf8').replace(/\/\*[\s\S]*?\*\//g, '')

describe('a height in parts', () => {
  beforeEach(setupDOM)

  it('is a share of a column, not a CSS height', () => {
    render([base('page', 'div', { layout: { String: 'vertical' }, height: { CssSize: { Fraction: 1 } } }, [
      box('fixed', { height: { CssSize: { Pixels: 40 } } }),
      box('rest', { height: { CssSize: { Fraction: 2.5 } } }),
    ])])
    const page = drawn(['page'])
    expect(page.classList.contains('takes-height-parts')).toBe(true)
    expect(page.style.getPropertyValue('--ov-height-parts')).toBe('1')
    expect(page.style.height).toBe('')
    const fixed = drawn(['page', 'fixed'])
    expect(fixed.style.height).toBe('40px')
    expect(fixed.classList.contains('takes-height-parts')).toBe(false)
    expect(drawn(['page', 'rest']).style.getPropertyValue('--ov-height-parts')).toBe('2.5')
  })

  it('is read by the column stylesheet', () => {
    expect(styles).toMatch(/\.layout-vertical > \.takes-height-parts,[^{]*\{\s*flex: var\(--ov-height-parts\) 1 0;/)
  })

  it('leaves a fit-content div where its aligning parent places it, not stretched', () => {
    render([base('page', 'div', { alignment: { String: 'center' } }, [
      box('bar', { width: { CssSize: 'FitContent' } }),
    ])])
    expect(drawn(['page', 'bar']).classList.contains('fits-content')).toBe(true)
    expect(styles).toMatch(/\.overseer-div\.align-center > \.overseer-div:first-child:not\(\.fits-content\)/)
  })

  it('makes the page a column down to the document only when the top of the document asks', () => {
    expect(styles).toMatch(/#main:has\(#content-display > \.takes-height-parts[^{]*\{\s*display: flex;\s*flex-direction: column;/)
    expect(styles).toMatch(/#content-display:has\(> \.takes-height-parts[^{]*\{\s*flex: 1 1 auto;\s*display: flex;\s*flex-direction: column;/)
  })
})
