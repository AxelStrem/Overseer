import { describe, it, expect, beforeEach, vi } from 'vitest'

// `view="progress"` draws an int or a float as a bar, full at `limit=` - 100 for an int, 1 for a
// float. Only a `layout` the field states itself stands the bar upright.

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

const number = (name, type, value, extra = {}) => ({
  name, node_type: type, children: [], is_hierarchy_transparent: false,
  parameters: {
    value: type === 'float' ? { Float: value } : { Integer: value },
    view: { String: 'progress' },
    ...extra,
  },
})

const fieldAt = (name) => document.querySelector(`[data-path='${JSON.stringify(['t', name])}']`)
const fillOf = (name) => fieldAt(name).querySelector('.progress-fill')

function render(fields, tabParameters = {}) {
  const app = new OverseerApp()
  app.currentDocument = [
    { name: 't', node_type: 'tab', parameters: tabParameters, is_hierarchy_transparent: false, children: fields },
  ]
  app.renderer.renderDocument(app.currentDocument)
  return app
}

describe('a number drawn as a progress bar', () => {
  beforeEach(() => setupDOM())

  it('is full at 100 for an int and at 1 for a float, and at its limit where it states one', () => {
    render([
      number('percent', 'int', 40),
      number('share', 'float', 0.25),
      number('pages', 'int', 30, { limit: { Integer: 120 } }),
    ])
    expect(fillOf('percent').style.width).toBe('40%')
    expect(fillOf('share').style.width).toBe('25%')
    expect(fillOf('pages').style.width).toBe('25%')
    expect(fieldAt('percent').querySelector('.progress-track').getAttribute('aria-valuemax')).toBe('100')
  })

  it('never draws past its ends', () => {
    render([number('over', 'int', 140), number('under', 'float', -0.5)])
    expect(fillOf('over').style.width).toBe('100%')
    expect(fillOf('under').style.width).toBe('0%')
  })

  it('stands upright only where the field itself says layout="vertical"', () => {
    render([
      number('upright', 'int', 60, { layout: { String: 'vertical' } }),
      number('lying', 'int', 60),
    ], { layout: { String: 'vertical' } })
    expect(fieldAt('upright').classList.contains('progress-vertical')).toBe(true)
    expect(fillOf('upright').style.height).toBe('60%')
    expect(fieldAt('lying').classList.contains('progress-vertical')).toBe(false)
    expect(fillOf('lying').style.width).toBe('60%')
  })

  it('takes the colour it is given', () => {
    render([number('tinted', 'int', 50, { color: { String: '#ff0000' } })])
    expect(fillOf('tinted').style.background).toContain('rgb(255, 0, 0)')
  })

  it('keeps its figure hidden in the field, and redraws when the value changes', () => {
    const app = render([number('percent', 'int', 40, { 'hover-text': { String: '40% done' } })])
    const field = fieldAt('percent')
    expect(field.querySelector('.field-value').classList.contains('progress-figure')).toBe(true)
    expect(field.dataset.hoverText).toBe('40% done')

    const changed = number('percent', 'int', 70, { 'hover-text': { String: '70% done' } })
    app.renderer.updateNumericElement(field, changed)
    expect(fillOf('percent').style.width).toBe('70%')
    expect(field.dataset.hoverText).toBe('70% done')
  })
})
