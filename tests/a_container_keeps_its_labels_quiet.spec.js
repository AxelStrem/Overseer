import { describe, it, expect, beforeEach, vi } from 'vitest'

// `hide-labels` is said on a container and meant for the fields inside it. The resolver copies
// it down onto every descendant, so by the time it reaches here the question is answered from
// the field's own parameters - this is the half that decides what that means on the screen.
//
// Hidden, not left out. The label text is the only place the field says what it is, and a
// heading above the column or a popup under the mouse will want to read it; taking it out of
// the page would mean fetching it back from somewhere else.

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

/** A field as it arrives once the resolver has handed the container's instruction down. */
const field = (name, { label = 'Name', hidden, type = 'string' } = {}) => {
  const node = Object.assign(base(name, type), {
    parameters: { value: { String: 'something' }, label: { String: label } },
  })
  if (hidden !== undefined) node.parameters['hide-labels'] = { Boolean: hidden }
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

describe('a field told not to show its label', () => {
  beforeEach(setupDOM)

  it('keeps the label in the page and out of sight', () => {
    render(field('a', { hidden: true }))
    const label = fieldFor('a').querySelector('label')

    expect(label).not.toBeNull()
    expect(label.textContent).toBe('Name')
    expect(label.style.display).toBe('none')
  })

  it('shows it when nothing said to hide it', () => {
    render(field('a'))
    const label = fieldFor('a').querySelector('label')

    expect(label.style.display).not.toBe('none')
  })

  it('shows it again where a container inside said otherwise', () => {
    // The resolver hands down `false` from the nearest ancestor to state anything, so the
    // field arrives saying it outright.
    render(field('a', { hidden: false }))
    const label = fieldFor('a').querySelector('label')

    expect(label.style.display).not.toBe('none')
  })

  it('does not arrange a label it is not going to show', () => {
    // `label-beside` makes the field a flex row so the label and value sit on one line. With
    // no label to sit beside, that only changes how the value sizes.
    render(field('a', { hidden: true }))
    const container = fieldFor('a')

    expect(container.classList.contains('label-beside')).toBe(false)
    expect(container.classList.contains('label-above')).toBe(false)
  })

  it('leaves a checkbox visible after hiding the words around it', () => {
    // A checkbox is built inside its label, so that clicking the words toggles it. Hiding the
    // label would take the box with it.
    render(field('done', { hidden: true, type: 'checkbox' }))
    const container = document.querySelector('.checkbox-field')
    const box = container.querySelector('input[type=checkbox]')

    expect(box).not.toBeNull()
    expect(box.closest('label')).toBeNull()
    expect(container.querySelector('label').style.display).toBe('none')
  })
})
