import { describe, it, expect, beforeEach, vi } from 'vitest'

// A translucent background was drawn in several shades.
//
// `background-color` used to be handed down the tree, and every container and field inside a
// coloured div painted the colour it was handed once more. Painting an opaque colour twice looks the same,
// but a translucent one compounds level by level - and a tint is the one colour a document can
// state that reads right in both the light and the dark theme. A colour now stays on the node that
// states it, and what is inside lets it show through, while the page still tells a button what it
// sits on.

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

const TINT = '#6366F11A'

const node = (name, type, params, children = []) => ({
  name, node_type: type, parameters: params, children,
  is_hierarchy_transparent: false, param_order: [], authored_dash: false,
})

const aTintedCard = (innerParams = {}) => [
  node('root', 'tab', { label: { String: 'T' } }, [
    node('card', 'div', { 'background-color': { String: TINT } }, [
      node('inner', 'div', innerParams, [
        node('title', 'string', { value: { String: 'Climb a mountain' } }, []),
        node('go', 'button', { label: { String: 'Done' } }, []),
      ]),
    ]),
  ]),
]

const at = (...path) => document.querySelector(`[data-path='${JSON.stringify(['root', ...path])}']`)

const unpainted = (el) => ['', 'transparent'].includes(el.style.backgroundColor)

const drawn = (doc) => {
  const renderer = new OverseerRenderer()
  window.app = { currentDocument: doc, renderer }
  renderer.renderDocument(doc)
}

describe('a translucent colour is painted once', () => {
  beforeEach(() => { setupDOM() })

  it('is painted on the div that states it', () => {
    drawn(aTintedCard())
    expect(at('card').style.backgroundColor).toMatch(/^rgba\(99, 102, 241, 0\.10/)
  })

  it('is not painted again by a div inside it', () => {
    drawn(aTintedCard())
    const inner = at('card', 'inner')
    expect(inner, 'the inner div was not drawn').toBeTruthy()
    expect(unpainted(inner), `the inner div painted ${inner.style.backgroundColor}`).toBe(true)
  })

  it('is not painted again by a div that states nothing at all', () => {
    drawn(aTintedCard({}))
    const inner = at('card', 'inner')
    expect(unpainted(inner), `the inner div painted ${inner.style.backgroundColor}`).toBe(true)
  })

  it('is not painted again by a field inside it', () => {
    drawn(aTintedCard())
    const field = at('card', 'inner', 'title')
    expect(field, 'the field was not drawn').toBeTruthy()
    expect(unpainted(field), `the field painted ${field.style.backgroundColor}`).toBe(true)
    for (const el of field.querySelectorAll('*')) {
      expect(el.style.backgroundColor, `${el.tagName} inside the field`).not.toMatch(/rgba\(99, 102, 241/)
    }
  })

  it('still shades a button from the colour it hands on', () => {
    drawn(aTintedCard())
    const button = document.querySelector('.overseer-button')
    expect(button.style.getPropertyValue('--overseer-button-bg')).toMatch(/^rgba\(/)
  })

  it('leaves a div that states a colour of its own painted', () => {
    drawn(aTintedCard({ 'background-color': { String: '#ff000033' } }))
    expect(at('card', 'inner').style.backgroundColor).toBe('rgba(255, 0, 0, 0.2)')
  })
})
