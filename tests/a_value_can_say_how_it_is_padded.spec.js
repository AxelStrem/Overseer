import { describe, it, expect, beforeEach, vi } from 'vitest'
import { readFileSync } from 'node:fs'

// A field can say what its own value box is padded by.
//
// `padding` on a field styles the container around the value, not the box the text sits in - and
// that box carries `padding: 6px 8px` from the stylesheet. So two fields stacked in a column keep
// twelve pixels between their lines, six under one and six over the next, and nothing in the
// document could say otherwise. A name with a smaller sub-name beneath it wants those two facing
// sides closed and the left and right kept, which is the sentence `value-padding` lets it say.
//
// jsdom lays nothing out, so what these check is the inline style handed to the box - which is
// where the decision lives, since an inline style is what overrules the sheet.

function setupDOM() {
  document.body.innerHTML = `
    <div id="app">
      <div id="toolbar"><button id="open-file-btn"></button><button id="new-file-btn"></button>
        <button id="save-file-btn"></button><button id="reload-file-btn"></button></div>
      <button id="welcome-open-btn"></button><button id="welcome-new-btn"></button>
      <button id="error-back-btn"></button>
      <div id="tab-container"></div><div id="content-display"></div>
      <div id="status-bar"><span id="status-message"></span><span id="status-info"></span>
        <span id="file-path"></span></div>
      <div id="welcome-screen" class="screen"></div><div id="editor-screen" class="screen"></div>
      <div id="error-screen" class="screen"></div><div id="error-message"></div>
    </div>`
}

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }))
import { OverseerApp } from '../src/main.js'

const base = (name, node_type) => ({
  name, node_type, parameters: {}, children: [], is_hierarchy_transparent: false,
  source_id: '', source_fingerprint: 1, param_order: [], authored_dash: false,
})

const field = (name, value, params = {}) => Object.assign(base(name, 'string'), {
  parameters: Object.assign({ value: { String: value } }, params),
})

const render = (children) => {
  const app = new OverseerApp()
  app.currentDocument = [Object.assign(base('t', 'tab'), {
    children: [Object.assign(base('stack', 'div'), {
      parameters: {
        layout: { String: 'vertical' }, _effective_layout: { String: 'vertical' },
        margin: { CssSize: { Pixels: 0 } },
      },
      children,
    })],
  })]
  app._currentText = 'TEXT'
  app.renderer._filters = new Map()
  app.renderer.renderDocument(app.currentDocument)
  return app
}

const boxOf = (name) => document
  .querySelector(`[data-path='${JSON.stringify(['t', 'stack', name])}']`)
  .querySelector('.field-value')

describe('a value can say how it is padded', () => {
  beforeEach(setupDOM)

  it('takes the padding the field states', () => {
    render([field('name', 'Paloma ice cream', { 'value-padding': { String: '0 8px' } })])
    expect(boxOf('name').style.padding).toBe('0px 8px')
  })

  it('leaves the stylesheet to it when the field says nothing', () => {
    // The common case, and the one that must not change: every other field in every document
    // keeps the 6px 8px the sheet gives it.
    render([field('name', 'Paloma ice cream')])
    expect(boxOf('name').style.padding).toBe('')
  })

  it('closes the facing sides of two stacked fields', () => {
    // The case it exists for. Both boxes lose their vertical padding and keep their horizontal,
    // so the two lines sit together without the text going flush to the edge.
    render([
      field('name', 'Paloma ice cream',
            { 'font-size': { CssSize: { Pixels: 20 } }, 'value-padding': { String: '0 8px' } }),
      field('sub_name', 'pistachio & crushed chocolate',
            { 'font-size': { CssSize: { Pixels: 12 } }, 'value-padding': { String: '0 8px' } }),
    ])
    for (const name of ['name', 'sub_name']) {
      expect(boxOf(name).style.padding).toBe('0px 8px')
    }
  })

  it('accepts any css padding, not just two values', () => {
    render([
      field('a', 'x', { 'value-padding': { String: '0' } }),
      field('b', 'y', { 'value-padding': { String: '2px 8px 0' } }),
    ])
    expect(boxOf('a').style.padding).toBe('0px')
    expect(boxOf('b').style.padding).toBe('2px 8px 0px')
  })

  it('is ignored when empty rather than collapsing the box', () => {
    // An empty parameter is a document that has not decided, not one asking for no padding.
    render([field('name', 'x', { 'value-padding': { String: '' } })])
    expect(boxOf('name').style.padding).toBe('')
  })

  it('the stylesheet is the thing being overruled', () => {
    // If `.field-value` ever stops carrying its own vertical padding, this parameter stops being
    // needed - and this test says where to look.
    const styles = readFileSync('src/styles.css', 'utf8').replace(/\/\*[\s\S]*?\*\//g, '')
    // The base rule, anchored to the start of a line: there are more specific ones - a list
    // marked tight trims this to 2px - and the card's list is not one of them.
    const rule = styles.match(/^\.field-value\s*\{([^}]*)\}/m)
    expect(rule, 'the value box has no rule of its own').not.toBeNull()
    expect(rule[1]).toMatch(/padding:\s*6px\s+8px/)
  })
})
