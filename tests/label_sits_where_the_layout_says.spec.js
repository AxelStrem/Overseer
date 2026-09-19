import { describe, it, expect, beforeEach, vi } from 'vitest'
import { readFileSync } from 'node:fs'

// A field arranges two things, its label and its value, and does it the way the container it
// sits in does not: above the value inside a horizontal row, beside it inside a vertical column.
// The resolver decides that and leaves the answer under `_label_layout`; this is the half that
// reaches the screen.
//
// The label's display and margin are set inline rather than in the stylesheet. Not a preference:
// they were already being set inline, and an inline style overrules a stylesheet, so a rule
// written there would have no effect at all. The arrangement of the field itself is a class,
// because nothing sets that inline.

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

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }))
import { OverseerApp } from '../src/main.js'

const base = (name, node_type) => ({
  name, node_type, parameters: {}, children: [], is_hierarchy_transparent: false,
  source_id: '', source_fingerprint: 1, param_order: [], authored_dash: false,
})

/** A field as the resolver hands it over: a value, a label, and where the label goes. */
const field = (name, { label, layout, type = 'string' } = {}) => {
  const node = Object.assign(base(name, type), {
    parameters: { value: { String: 'something' } },
  })
  if (label !== undefined) node.parameters.label = { String: label }
  if (layout !== undefined) node.parameters._label_layout = { String: layout }
  return node
}

const render = (...fields) => {
  const app = new OverseerApp()
  app.currentDocument = [Object.assign(base('t', 'tab'), { children: fields })]
  app._currentText = 'TEXT'
  app.renderer._filters = new Map()
  app.renderer.renderDocument(app.currentDocument)
  return app
}

const fieldFor = (name) => {
  const value = document.querySelector(`[data-path='${JSON.stringify(['t', name])}']`)
  return value?.closest('.overseer-field') || document.querySelector('.overseer-field')
}

describe('where a field puts its label', () => {
  beforeEach(setupDOM)

  it('sets it beside the value inside a vertical container', () => {
    render(field('a', { label: 'name', layout: 'horizontal' }))
    const container = fieldFor('a')

    expect(container.classList.contains('label-beside')).toBe(true)
    expect(container.classList.contains('label-above')).toBe(false)

    const label = container.querySelector('label')
    expect(label.style.display).toBe('inline-block')
    expect(label.style.marginBottom, 'a label beside its value must not push the row taller')
      .toBe('0px')
  })

  it('sets it above the value inside a horizontal row', () => {
    render(field('a', { label: 'name', layout: 'vertical' }))
    const container = fieldFor('a')

    expect(container.classList.contains('label-above')).toBe(true)
    expect(container.classList.contains('label-beside')).toBe(false)

    const label = container.querySelector('label')
    expect(label.style.display).toBe('block')
    expect(label.style.marginBottom).toBe('4px')
  })

  it('leaves a field with nothing to place alone', () => {
    // `label=""` renders the value by itself. Making its container a flex row would change how
    // that value sizes, for a label that is not there - and most rows of a tight list are
    // written this way, so it would be most of the screen.
    render(field('a', { label: '', layout: 'horizontal' }))
    const container = fieldFor('a')

    expect(container.querySelector('label')).toBeNull()
    expect(container.classList.contains('label-beside')).toBe(false)
    expect(container.classList.contains('label-above')).toBe(false)
  })

  it('falls back to a label above when nothing said where it goes', () => {
    // An older document, a node built by hand, anything that reached the renderer without going
    // through the resolver. The answer that matches how every field looked before this existed.
    render(field('a', { label: 'name' }))
    const container = fieldFor('a')

    expect(container.classList.contains('label-above')).toBe(true)
    expect(container.querySelector('label').style.display).toBe('block')
  })

  it('does the same for a checkbox, which used to be the exception', () => {
    // A boolean pinned its label above the control in the stylesheet, because a row of fields
    // aligns only if they agree where a label goes. That is now the general rule rather than a
    // rule about checkboxes, so in a column it wears its label on the right like a checkbox
    // conventionally does.
    const node = Object.assign(base('flag', 'bool'), {
      parameters: {
        value: { Boolean: true },
        label: { String: 'irregular' },
        _label_layout: { String: 'horizontal' },
      },
    })
    render(node)

    const container = document.querySelector('.overseer-field')
    expect(container.classList.contains('label-beside')).toBe(true)
    expect(container.querySelector('input[type="checkbox"]')).toBeTruthy()
  })
})

// -- and the half of it that lives in the stylesheet -------------------------------------------

describe('the rule that arranges a field beside its label', () => {
  const styles = readFileSync('src/styles.css', 'utf8').replace(/\/\*[\s\S]*?\*\//g, '')

  /** The body of the rule this selector appears in, grouped with others or not. */
  const bodyOf = (selector) => {
    const pattern = /([^{}]+)\{([^}]*)\}/g
    let match
    while ((match = pattern.exec(styles)) !== null) {
      const [, selectors, body] = match
      if (selectors.split(',').some((one) => one.trim() === selector)) return body
    }
    return ''
  }

  it('makes the field the row that holds the two of them', () => {
    // Without this the class is inert and the label sits inline against the value with no gap,
    // which reads as one run-on word rather than a name and a value.
    expect(bodyOf('.overseer-field.label-beside')).toMatch(/display\s*:\s*flex/)
  })

  it('lines them up on the text rather than on the box', () => {
    expect(bodyOf('.overseer-field.label-beside')).toMatch(/align-items\s*:\s*baseline/)
  })

  it('does not let a long value push the field wider than its column', () => {
    // A flex item will not shrink below its content unless told it may, and these sit in rows
    // whose widths are percentages of a list.
    expect(bodyOf('.overseer-field.label-beside > .field-value')).toMatch(/min-width\s*:\s*0/)
  })
})
