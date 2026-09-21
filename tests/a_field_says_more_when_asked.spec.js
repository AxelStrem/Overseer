import { describe, it, expect, beforeEach, vi, afterEach } from 'vitest'

// A field can say something about itself that is only read when somebody asks - the mouse resting
// on it. Given text, that text appears. Asked for with no text, the label appears instead, and
// that is the useful half: a label can be hidden, so a field in a column too narrow to name can
// carry its name here and show nothing but its value.
//
// The text is settled when the field is built, while the label is still there to read from. A
// table takes the label out of every cell afterwards - its heading names the column - so asking
// later would find nothing, which is exactly the case this exists for.

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

/// A field as the resolver hands it over, with whatever it has been told to say.
const field = (name, { label = 'Weight', saying, hidden } = {}) => {
  const node = Object.assign(base(name, 'string'), {
    parameters: { value: { String: '82.5' }, label: { String: label } },
  })
  if (saying !== undefined) {
    node.parameters['hover-text'] = typeof saying === 'boolean'
      ? { Boolean: saying } : { String: saying }
  }
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

const hoverOver = (element) => {
  element.dispatchEvent(new window.MouseEvent('mouseover', { bubbles: true }))
  vi.advanceTimersByTime(400)
  return document.querySelector('.overseer-hover-text')
}

const leave = (element) => {
  element.dispatchEvent(new window.MouseEvent('mouseout', { bubbles: true }))
}

/// A list drawn as a table, as the resolver hands it over. The list says `hover-text` once and
/// inheritance puts it on every field, so each cell answers with the name of its own column.
const aTable = () => {
  const columns = JSON.stringify([
    { name: 'handle', label: 'id', width: '10%', span: false },
    { name: 'title', label: 'title', width: '50%', span: false },
    { name: 'points', label: 'pts', width: '10%', span: false },
  ])
  const cell = (name, value, label) => Object.assign(base(name, 'string'), {
    parameters: {
      value: { String: value },
      label: { String: label },
      // What the resolver leaves on every field once the list has said it.
      'hover-text': { Boolean: true },
    },
  })
  const row = (n, handle, title, points) => Object.assign(base(`Row__${n}`, 'div'), {
    parameters: { layout: { String: 'horizontal' }, _effective_layout: { String: 'horizontal' } },
    children: [
      cell('handle', handle, 'id'),
      cell('title', title, 'title'),
      cell('points', points, 'pts'),
    ],
  })
  return [Object.assign(base('t', 'tab'), {
    children: [Object.assign(base('Rows', 'list'), {
      parameters: {
        view: { String: 'table' },
        header: { Boolean: true },
        _columns: { String: columns },
        _effective_layout: { String: 'vertical' },
        'hover-text': { Boolean: true },
      },
      children: [row(1, 'h1', 'the first thing', '3'), row(2, 'h2', 'the second', '5')],
    })],
  })]
}

describe('a cell of a table asked what column it is in', () => {
  beforeEach(() => {
    setupDOM()
    vi.useFakeTimers()
  })
  afterEach(() => {
    vi.useRealTimers()
    document.querySelectorAll('.overseer-hover-text').forEach((n) => n.remove())
  })

  it('answers with the name of its column, which the cell itself no longer shows', () => {
    // The case this was filed for. A table takes the label out of every cell, because the
    // heading names the column - so the cell shows a value and nothing else, and the name is
    // only in the heading, which is a long way up a scrolled table.
    const app = new OverseerApp()
    app.currentDocument = aTable()
    app._currentText = 'TEXT'
    app.renderer._filters = new Map()
    app.renderer.renderDocument(app.currentDocument)

    const cells = Array.from(document.querySelectorAll('.overseer-field'))
    expect(cells.length, 'the table did not render').toBeGreaterThan(2)

    // Every cell has lost its label element, and every cell still knows what it is.
    for (const cell of cells) {
      expect(cell.querySelector(':scope > label'), 'a cell kept its label').toBeNull()
    }
    const saying = cells.slice(0, 3).map((c) => c.dataset.hoverText)
    expect(saying).toEqual(['id', 'title', 'pts'])
  })

  it('shows that name when the mouse rests on the cell', () => {
    const app = new OverseerApp()
    app.currentDocument = aTable()
    app._currentText = 'TEXT'
    app.renderer._filters = new Map()
    app.renderer.renderDocument(app.currentDocument)

    const points = Array.from(document.querySelectorAll('.overseer-field'))
      .find((c) => c.dataset.hoverText === 'pts')
    expect(points, 'no cell says it is the points column').toBeTruthy()

    points.dispatchEvent(new window.MouseEvent('mouseover', { bubbles: true }))
    vi.advanceTimersByTime(400)
    const popup = document.querySelector('.overseer-hover-text')
    expect(popup.textContent).toBe('pts')
  })
})

describe('a field asked what it is', () => {
  beforeEach(() => {
    setupDOM()
    vi.useFakeTimers()
  })
  afterEach(() => {
    vi.useRealTimers()
    document.querySelectorAll('.overseer-hover-text').forEach((n) => n.remove())
  })

  it('says nothing when it has not been asked to', () => {
    render(field('weight'))
    expect(fieldFor('weight').dataset.hoverText).toBeUndefined()
  })

  it('says the text it was given', () => {
    render(field('weight', { saying: 'Measured before breakfast' }))
    expect(fieldFor('weight').dataset.hoverText).toBe('Measured before breakfast')
  })

  it('says its label when asked with no text', () => {
    // The useful half. Nothing has to be written twice for a field to name itself.
    render(field('weight', { saying: true, label: 'Weight in kg' }))
    expect(fieldFor('weight').dataset.hoverText).toBe('Weight in kg')
  })

  it('still says its label when the label is not shown', () => {
    // The pairing this was filed for: hiding a label loses the naming everywhere there is no
    // heading to carry it, and this is where the naming goes.
    render(field('weight', { saying: true, label: 'Weight in kg', hidden: true }))
    const container = fieldFor('weight')

    expect(container.querySelector('label').style.display).toBe('none')
    expect(container.dataset.hoverText).toBe('Weight in kg')
  })

  it('says nothing when asked to repeat a label it has not got', () => {
    render(field('weight', { saying: true, label: '' }))
    expect(fieldFor('weight').dataset.hoverText).toBeUndefined()
  })

  it('shows it after a moment and takes it away again', () => {
    render(field('weight', { saying: 'Measured before breakfast' }))
    const container = fieldFor('weight')

    expect(document.querySelector('.overseer-hover-text')).toBeNull()
    const popup = hoverOver(container)
    expect(popup, 'nothing appeared').toBeTruthy()
    expect(popup.textContent).toBe('Measured before breakfast')
    expect(popup.style.display).toBe('block')

    leave(container)
    expect(popup.style.display).toBe('none')
  })

  it('waits before showing, so running past a field shows nothing', () => {
    // Drawn across a table, a popup per column passed over would be a flicker rather than help.
    render(field('weight', { saying: 'Measured before breakfast' }))
    const container = fieldFor('weight')

    container.dispatchEvent(new window.MouseEvent('mouseover', { bubbles: true }))
    vi.advanceTimersByTime(100)
    leave(container)
    vi.advanceTimersByTime(400)

    const popup = document.querySelector('.overseer-hover-text')
    expect(popup === null || popup.style.display === 'none').toBe(true)
  })

  it('draws one popup however many fields ask', () => {
    // One element and one listener for the page. A table of five hundred rows has a few thousand
    // cells, and at most one of them is being asked at a time.
    render(
      field('weight', { saying: 'one' }),
      field('mood', { saying: 'two' }))
    const first = document.querySelectorAll('.overseer-field')[0]
    const second = document.querySelectorAll('.overseer-field')[1]

    hoverOver(first)
    leave(first)
    hoverOver(second)

    expect(document.querySelectorAll('.overseer-hover-text')).toHaveLength(1)
    expect(document.querySelector('.overseer-hover-text').textContent).toBe('two')
  })
})
