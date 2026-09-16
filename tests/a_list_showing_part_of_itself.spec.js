import { describe, it, expect, beforeEach, vi } from 'vitest'

// What the reader sees of a list that is only showing part of itself.
//
// The resolver leaves entries past the window uninstantiated - no template copied onto them, no
// values worked out - which is what takes the food tracker from seven seconds to one and a half.
// Two things follow for the renderer, and both matter more than they look.
//
// It must not draw them. There is nothing there to draw: the fields those rows would show were
// never created, so a row drawn from one would be blank or broken rather than merely stale.
//
// And it must say how many there were. Three days of forty-three, with no word about the other
// forty, reads as a history that has been deleted. The count has to come from the resolver, since
// counting what is on screen cannot find what was never built.

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

/** A day in view: instantiated, with its fields. */
const inView = (n) => Object.assign(base(`Day__${n}`, 'div'), {
  parameters: { layout: { String: 'horizontal' }, _effective_layout: { String: 'horizontal' } },
  children: [
    Object.assign(base('date', 'string'), {
      parameters: { value: { String: `2026-09-${String(n).padStart(2, '0')}` }, label: { String: '' } },
    }),
  ],
})

/** A day out of view: as it was written, and marked. */
const outOfView = (n) => Object.assign(base('-', '-'), {
  parameters: { _out_of_view: true },
  children: [
    Object.assign(base('date', 'string'), {
      parameters: { value: { String: `2026-08-${String(n).padStart(2, '0')}` } },
    }),
  ],
})

const docOf = (children, listParams = {}) => ([Object.assign(base('t', 'tab'), {
  children: [
    Object.assign(base('Days', 'list'), {
      parameters: Object.assign({ _effective_layout: { String: 'vertical' } }, listParams),
      children,
    }),
  ],
})])

const render = (children, listParams) => {
  const app = new OverseerApp()
  app.currentDocument = docOf(children, listParams)
  app._currentText = 'TEXT'
  app.renderer._filters = new Map()
  app.renderer.renderDocument(app.currentDocument)
  return app
}

const note = () => document.querySelector('.overseer-entries-left-out')
/** Every date actually on screen. A field renders its value into a `.field-value` span. */
const renderedDates = () => Array.from(document.querySelectorAll('.field-value'))
  .map(e => e.textContent)
  .filter(v => typeof v === 'string' && v.startsWith('20'))
/** The entry elements the list drew, the note not counted as one. */
const drawnEntries = () => Array.from(document.querySelector('.overseer-list').children)
  .filter(e => !e.classList.contains('overseer-entries-left-out'))

describe('a list showing part of itself', () => {
  beforeEach(setupDOM)

  it('draws the entries in view and not the ones left out', () => {
    render([inView(12), inView(13), outOfView(3), outOfView(4)])
    expect(renderedDates()).toEqual(['2026-09-12', '2026-09-13'])
    // Not merely absent from the text: no element was made for them at all.
    expect(drawnEntries().length).toBe(2)
  })

  it('says how many were left out', () => {
    render([inView(12), outOfView(3), outOfView(4)], { _left_out_of_view: 40 })
    expect(note()).not.toBeNull()
    expect(note().textContent).toContain('40')
  })

  it('says it in the singular when there is one', () => {
    render([inView(12), outOfView(3)], { _left_out_of_view: 1 })
    expect(note().textContent).toBe('1 earlier entry not shown')
  })

  it('says nothing when everything is in view', () => {
    // The common case, and the one where a note would be noise: every document that does not
    // window a list must look exactly as it did.
    render([inView(12), inView(13)])
    expect(note()).toBeNull()
    expect(renderedDates().filter(d => d.startsWith('2026-09')).length).toBeGreaterThan(0)
  })

  it('says nothing when the count is zero', () => {
    render([inView(12)], { _left_out_of_view: 0 })
    expect(note()).toBeNull()
  })

  it('does not leave two notes behind when the list is drawn again', () => {
    // A redraw is the ordinary case - every edit causes one - and a note appended each time would
    // stack up under the list.
    const app = render([inView(12), outOfView(3)], { _left_out_of_view: 40 })
    app.renderer.renderDocument(app.currentDocument)
    expect(document.querySelectorAll('.overseer-entries-left-out').length).toBe(1)
  })
})
