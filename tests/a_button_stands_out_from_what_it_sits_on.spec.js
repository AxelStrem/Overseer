import { describe, it, expect, beforeEach, vi } from 'vitest'

// A button on a coloured div used to be exactly the colour of the div.
//
// `background-color` is inherited down the tree, so a button inside a coloured container is
// handed that colour - and the renderer then applied it as though the button had asked for it.
// The result was a button the same shade as the thing it sat on, findable only by hovering.
//
// It is brightened now, with hover brighter again and press darker, so it reads as a raised
// thing that answers being pressed. A document that states a colour still gets that colour,
// and all three can be stated.

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

/// A coloured container with a button in it. `handedDown` marks the button's colour as one the
/// styling inheritance gave it, which is what the resolver does.
const aDocumentWhere = (buttonParams = {}, containerColour = '#204080', handedDown = true) => {
  const params = { label: { String: 'Press' }, ...buttonParams }
  if (handedDown) {
    params['background-color'] = { String: containerColour }
    params['_template_background-color'] = { Boolean: true }
  }
  return [
    node('root', 'tab', { label: { String: 'T' } }, [
      node('panel', 'div', { 'background-color': { String: containerColour } }, [
        node('go', 'button', params),
      ]),
    ]),
  ]
}

const theButton = () => document.querySelector('.overseer-button')

const drawn = (doc) => {
  const renderer = new OverseerRenderer()
  window.app = { currentDocument: doc, renderer }
  renderer.renderDocument(doc)
  return theButton()
}

describe('a button stands out from what it sits on', () => {
  beforeEach(() => { setupDOM() })

  it('is brighter than the colour it was handed', () => {
    const button = drawn(aDocumentWhere())
    const its = button.style.getPropertyValue('--overseer-button-bg')
    expect(its, 'the button was given no colour of its own').toBeTruthy()
    expect(its, 'it kept the colour it was handed').not.toBe('#204080')
  })

  it('gets brighter again for hover and darker for press', () => {
    const button = drawn(aDocumentWhere())
    const base = button.style.getPropertyValue('--overseer-button-bg')
    const hover = button.style.getPropertyValue('--overseer-button-hover')
    const press = button.style.getPropertyValue('--overseer-button-press')
    const brightness = (c) => {
      const [r, g, b] = c.match(/[0-9.]+/g).slice(0, 3).map(Number)
      return 0.299 * r + 0.587 * g + 0.114 * b
    }
    expect(hover).toBeTruthy()
    expect(press).toBeTruthy()
    expect(brightness(hover)).toBeGreaterThan(brightness(base))
    expect(brightness(press)).toBeLessThan(brightness(base))
  })

  it('keeps a colour the document states for the button itself', () => {
    // Saying it is the point of saying it, so nothing is derived over the top.
    const doc = aDocumentWhere({ 'background-color': { String: '#c0392b' } }, '#204080', false)
    const button = drawn(doc)
    expect(button.style.getPropertyValue('--overseer-button-bg')).toBe('#c0392b')
  })

  it('takes a hover and a press the document states', () => {
    const doc = aDocumentWhere({
      'hover-color': { String: '#00ff00' },
      'press-color': { String: '#ff0000' },
    })
    const button = drawn(doc)
    expect(button.style.getPropertyValue('--overseer-button-hover')).toBe('#00ff00')
    expect(button.style.getPropertyValue('--overseer-button-press')).toBe('#ff0000')
  })

  it('puts dark text on a light button and light text on a dark one', () => {
    // Making it visible is the point; making its label invisible instead would not be a fix.
    const onDark = drawn(aDocumentWhere({}, '#101820'))
    expect(onDark.style.color).toBe('rgb(255, 255, 255)')

    setupDOM()
    const onLight = drawn(aDocumentWhere({}, '#f5e6a8'))
    expect(onLight.style.color).toBe('rgb(26, 26, 26)')
  })

  it('leaves the text colour alone when the document states one', () => {
    const button = drawn(aDocumentWhere({ 'font-color': { String: '#123456' } }))
    expect(button.style.color).not.toBe('rgb(255, 255, 255)')
  })

  it('leaves a button on nothing to the stylesheet', () => {
    // No colour anywhere to derive from. Inventing one would be worse than the stylesheet's.
    const doc = [
      node('root', 'tab', { label: { String: 'T' } }, [
        node('go', 'button', { label: { String: 'Press' } }),
      ]),
    ]
    const button = drawn(doc)
    expect(button.style.getPropertyValue('--overseer-button-bg')).toBe('')
  })

  it('leaves a named colour alone rather than guessing at its value', () => {
    // `rebeccapurple` means nothing to arithmetic done here, and a shade invented for it would
    // not be a shade of it.
    const button = drawn(aDocumentWhere({}, 'rebeccapurple'))
    expect(button.style.getPropertyValue('--overseer-button-bg')).toBe('')
  })
})
