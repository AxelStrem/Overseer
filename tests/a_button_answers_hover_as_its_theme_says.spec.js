import { describe, it, expect, beforeEach, vi } from 'vitest'
import { readFileSync } from 'node:fs'

// How a button answers hover is a key of the theme: `context theme { - button { - hover = lift } }`.
// The page writes it as `--ov-button-hover` and, once a button is in the page, marks it from the
// nearest one above it, so a container can change it for the buttons inside.

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

const key = (name, value) => ({
  name, node_type: '-', children: [], is_hierarchy_transparent: false,
  parameters: { value: { String: value } },
})

const theme = (hover) => ({
  name: 'theme', node_type: 'context', is_hierarchy_transparent: false,
  children: [key('accent', '#2563eb'), { name: 'button', node_type: '-', children: [key('hover', hover)], is_hierarchy_transparent: false, parameters: {} }],
  parameters: { _context_copy: { Boolean: true } },
})

const button = (name) => ({
  name, node_type: 'button', children: [], is_hierarchy_transparent: false,
  parameters: { label: { String: name } },
})

const div = (name, children) => ({
  name, node_type: 'div', parameters: {}, is_hierarchy_transparent: false, children,
})

const buttonAt = (path) => document.querySelector(`button[data-path='${JSON.stringify(path)}']`)
const settled = () => new Promise((resolve) => setTimeout(resolve, 0))

describe('a button in the theme', () => {
  beforeEach(() => {
    setupDOM()
    document.documentElement.removeAttribute('style')
  })

  it('answers hover as the nearest theme above it says', async () => {
    const app = new OverseerApp()
    app.currentDocument = [
      { name: 't', node_type: 'tab', parameters: {}, is_hierarchy_transparent: true, children: [
        button('outside'),
        div('side', [theme('lift'), button('inside'), div('back', [theme('glow'), button('again')])]),
      ] },
      theme('background'),
    ]
    app.renderer.renderDocument(app.currentDocument)
    await settled()

    expect(buttonAt(['t', 'outside']).classList.contains('ov-hover-background')).toBe(true)
    expect(buttonAt(['t', 'side', 'inside']).classList.contains('ov-hover-lift')).toBe(true)
    expect(buttonAt(['t', 'side', 'inside']).classList.contains('ov-hover-background')).toBe(false)
    expect(buttonAt(['t', 'side', 'back', 'again']).classList.contains('ov-hover-glow')).toBe(true)
  })

  it('keeps today\'s look where nothing says, or what it says is no way the page knows', async () => {
    const app = new OverseerApp()
    app.currentDocument = [
      { name: 't', node_type: 'tab', parameters: {}, is_hierarchy_transparent: true, children: [
        button('plain'),
        div('odd', [theme('wobble'), button('inside')]),
      ] },
    ]
    app.renderer.renderDocument(app.currentDocument)
    await settled()

    for (const path of [['t', 'plain'], ['t', 'odd', 'inside']]) {
      expect(Array.from(buttonAt(path).classList).filter((c) => c.startsWith('ov-hover-'))).toEqual([])
    }
  })

  it('has a rule for every way the theme can name', () => {
    const styles = readFileSync('src/styles.css', 'utf8')
    for (const way of ['border', 'glow', 'lift', 'grow', 'none']) {
      expect(styles, `nothing draws ${way}`).toMatch(new RegExp(`\\.overseer-button\\.ov-hover-${way}:hover`))
    }
  })
})
