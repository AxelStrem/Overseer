import { describe, it, expect, beforeEach, vi } from 'vitest'
import { readFileSync } from 'node:fs'

// A colour or font given to a node is that node's alone.
//
// A container's `font-color` and `font-size` used to be copied onto everything inside it, and the
// page then pressed them onto every value within as well, past the classes that size them. Now
// the container keeps them, and what is inside it is drawn in the theme - a container that wants
// its contents to follow says so with `context theme`.

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }))
import { OverseerRenderer } from '../src/renderer.js'

function setupDOM() {
  document.body.innerHTML = `
    <div id="app"><div id="tab-container"></div><div id="content-display"></div>
      <div id="status-bar"><span id="status-message"></span><span id="status-info"></span>
        <span id="file-path"></span></div>
      <div id="welcome-screen" class="screen"></div><div id="editor-screen" class="screen"></div>
      <div id="error-screen" class="screen"></div><div id="error-message"></div></div>`
}

const node = (name, type, params, children = []) => ({
  name, node_type: type, parameters: params, children,
  is_hierarchy_transparent: false, param_order: [], authored_dash: false,
})

const aDocument = () => [
  node('root', 'tab', { label: { String: 'T' } }, [
    node('card', 'div', { 'font-color': { String: '#ff0000' }, 'font-size': { String: '30px' } }, [
      node('plain', 'string', { value: { String: 'as the theme' } }),
      node('own', 'string', { value: { String: 'its own' }, 'font-color': { String: '#00ff00' } }),
    ]),
  ]),
]

const at = (...path) => document.querySelector(`[data-path='${JSON.stringify(['root', ...path])}']`)

const drawn = (doc) => {
  const renderer = new OverseerRenderer()
  window.app = { currentDocument: doc, renderer }
  renderer.renderDocument(doc)
}

describe('a font given to a container stays on it', () => {
  beforeEach(() => { setupDOM() })

  it('is set on the container', () => {
    drawn(aDocument())
    const card = at('card')
    expect(card.style.color).toBe('rgb(255, 0, 0)')
    expect(card.classList.contains('ov-own-font')).toBe(true)
  })

  it('is not pressed onto a value inside it', () => {
    drawn(aDocument())
    const value = at('card', 'plain').querySelector('.field-value')
    expect(value, 'the field was not drawn').toBeTruthy()
    expect(value.style.color).not.toBe('rgb(255, 0, 0)')
    expect(value.style.fontSize).not.toBe('30px')
  })

  it('leaves a field inside it its own colour', () => {
    drawn(aDocument())
    const value = at('card', 'own').querySelector('.field-value')
    expect(value.style.color).toBe('rgb(0, 255, 0)')
  })

  it('is stopped by the stylesheet from reaching what is inside', () => {
    // The browser hands `color` and `font-size` down by itself; the stylesheet puts what is drawn
    // inside back in the theme, with no weight of its own so any class still wins.
    const css = readFileSync('src/styles.css', 'utf8')
    expect(css).toMatch(/:where\(\.ov-own-font\) :where\(\[data-path\]\)\s*\{[^}]*color: var\(--ov-text\)/)
  })
})
