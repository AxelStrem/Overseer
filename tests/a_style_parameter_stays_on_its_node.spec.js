import { describe, it, expect, beforeEach, vi } from 'vitest'
import { readFileSync } from 'node:fs'

// A style parameter styles its own node and what that node draws, never a node inside it: a
// container that says `border-radius=0` or `border-style=none` leaves the fields in it to the
// theme. It is set inline without `!important`, which beats the stylesheet's class rules - and so
// the theme's kind keys - on the node, and lets the theme be changed for what is inside.

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

const field = (name, parameters = {}) => ({
  name, node_type: 'string', children: [], is_hierarchy_transparent: false,
  parameters: Object.assign({ value: { String: name } }, parameters),
})

const div = (name, parameters, children) => ({
  name, node_type: 'div', children, is_hierarchy_transparent: false, parameters,
})

const render = (...nodes) => {
  const app = new OverseerApp()
  app.currentDocument = [{ name: 't', node_type: 'tab', parameters: {}, children: nodes, is_hierarchy_transparent: false }]
  app.renderer.renderDocument(app.currentDocument)
  return app
}

const at = (...path) => document.querySelector(`[data-path='${JSON.stringify(['t', ...path])}']`)
const valueOf = (...path) => at(...path).querySelector('.field-value')
// The radius reaches a field's value a moment after it is drawn.
const settle = () => new Promise((resolve) => setTimeout(resolve, 0))

const none = { BorderStyle: 'None' }
const zero = { CssSize: { Pixels: 0 } }

describe('a style parameter', () => {
  beforeEach(() => setupDOM())

  it('rounds its own value, and not the values of the fields inside a container', async () => {
    render(
      field('square', { 'border-radius': zero }),
      div('box', { 'border-radius': zero }, [field('inside')]),
    )
    await settle()
    expect(valueOf('square').style.borderRadius).toBe('0px')
    expect(at('box').style.borderRadius).toBe('0px')
    expect(valueOf('box', 'inside').style.borderRadius).toBe('')
  })

  it('takes a bare number for a radius in pixels', async () => {
    // `border-radius=0` arrives as `{ Integer: 0 }`, not as a CSS size.
    render(
      field('square', { 'border-radius': { Integer: 0 } }),
      div('box', { 'border-radius': { Integer: 0 } }, [field('inside')]),
      div('round', { 'border-radius': { Integer: 8 } }, []),
    )
    await settle()
    expect(valueOf('square').style.borderRadius).toBe('0px')
    expect(at('box').style.borderRadius).toBe('0px')
    expect(valueOf('box', 'inside').style.borderRadius).toBe('')
    expect(at('round').style.borderRadius).toBe('8px')
  })

  it('takes away its own frame, and leaves the frames of the fields inside a container', async () => {
    render(
      field('bare', { 'border-style': none }),
      div('box', { 'border-style': none }, [field('inside')]),
    )
    await settle()
    // jsdom keeps no `border: none`, so the shadow it takes away with the border stands for both.
    expect(valueOf('bare').style.boxShadow).toBe('none')
    expect(at('box').style.boxShadow).toBe('none')
    expect(valueOf('box', 'inside').style.boxShadow).toBe('')
  })

  it('makes its own text bold, and leaves the fields inside a container to the theme', () => {
    render(
      field('heavy', { 'font-weight': { String: 'bold' } }),
      div('box', { 'font-weight': { String: 'bold' } }, [field('inside')]),
    )
    expect(valueOf('heavy').style.fontWeight).toBe('bold')
    expect(at('box').style.fontWeight).toBe('bold')
    expect(valueOf('box', 'inside').style.fontWeight).toBe('')
    // The browser hands a weight down by itself; the stylesheet puts what is inside back.
    expect(at('box').classList.contains('ov-own-font')).toBe(true)
    const css = readFileSync('src/styles.css', 'utf8')
    expect(css).toMatch(/:where\(\.ov-own-font\) :where\(\[data-path\]\)\s*\{[^}]*font-weight: normal/)
  })

  it('is set without !important', async () => {
    render(field('styled', {
      'border-style': none,
      'border-radius': zero,
      'font-size': { CssSize: { Pixels: 18 } },
      'font-color': { String: 'red' },
      'font-weight': { String: 'bold' },
      shadow: { String: 'soft' },
    }))
    await settle()
    for (const element of [at('styled'), valueOf('styled')]) {
      for (const property of ['box-shadow', 'border-top-left-radius', 'font-size', 'color', 'font-weight']) {
        expect(element.style.getPropertyPriority(property), property).toBe('')
      }
    }
  })
})
