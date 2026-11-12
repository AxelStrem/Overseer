import { describe, it, expect, beforeEach, vi } from 'vitest'
import { setupCalendarDOM, calendarDocument } from './helpers/calendar.js'

// A day that only a `layout="year"` draws comes from the server marked `_drawn_by_style`, with
// nothing under it worked out. It is drawn as a square of its style and nothing more: its hover
// gives its date, and a field's value. A day drawn in full elsewhere is drawn as it always was.

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }))
import { OverseerApp } from '../src/main.js'

const base = (name, node_type, parameters = {}, children = []) => ({
  name, node_type, parameters, children, is_hierarchy_transparent: false,
  source_id: '', source_fingerprint: 1, param_order: [], authored_dash: false,
})

const field = (name, parameters) => base(name, 'int', { label: { String: name }, ...parameters })

// Days from the 2nd to the 6th of a calendar of divs; the calendar shows the 6th, the heatmap all
// five, and the 2nd to the 5th are drawn by the heatmap alone.
const documentOfDivs = () => {
  const days = []
  for (let d = 2; d <= 6; d++) {
    const name = `2026-11-0${d}`
    const total = d === 3 ? 3000 : 500
    const square = d < 6
    const doubled = square
      ? field('doubled', { value: { Formula: 'total * 2' } })
      : field('doubled', { value: { Formula: 'total * 2' }, _computed_value: { Integer: total * 2 } })
    days.push(base(name, 'div', {
      _in_view: { Boolean: d === 6 },
      '_in_view@t/Heat': { Boolean: true },
      '_view@t/Heat@background-color': { Formula: 'doubled > 2000 ? "red" : ""' },
      '_computed__view@t/Heat@background-color': { String: total > 1000 ? 'red' : '' },
      ...(square ? { _drawn_by_style: { Boolean: true } } : {}),
    }, [field('total', { value: { Integer: total } }), doubled]))
  }
  const heat = base('Heat', 'calendar', {
    of: { String: 'Days' }, layout: { String: 'year' },
    _view_of: { String: 't/Days' }, _view_at: { String: 't/Heat' },
  })
  const calendar = base('Days', 'calendar', { entry: { Template: 'Day' }, span: { String: 'day' }, layout: { String: 'day' } }, days)
  return [base('t', 'tab', {}, [calendar, heat])]
}

const render = (doc) => {
  const app = new OverseerApp()
  app.currentDocument = doc
  app._currentText = 'TEXT'
  app.renderer.renderDocument(app.currentDocument)
  return app
}

const square = (selector, day) => document.querySelector(`${selector} > [data-path='${JSON.stringify(['t', 'Days', day])}']`)

const hover = (cell) => {
  cell.dispatchEvent(new window.MouseEvent('mouseover', { bubbles: true }))
  return cell.title
}

describe('a day only a year draws', () => {
  beforeEach(() => setupCalendarDOM())

  it('is a square of its colour with nothing drawn in it', () => {
    render(documentOfDivs())
    const red = square('[data-calendar-view="t/Heat"]', '2026-11-03')
    expect(red.style.backgroundColor).toBe('red')
    expect(red.querySelectorAll('[data-path]')).toHaveLength(0)
    expect(square('[data-calendar-view="t/Heat"]', '2026-11-04').style.backgroundColor).toBe('')
  })

  it('shows its date alone on hover', () => {
    render(documentOfDivs())
    expect(hover(square('[data-calendar-view="t/Heat"]', '2026-11-03'))).toBe('2026-11-03')
  })

  it('leaves a day drawn in full elsewhere as it is', () => {
    render(documentOfDivs())
    const own = square('.overseer-calendar:not([data-calendar-view])', '2026-11-06')
    expect(own.querySelectorAll('[data-path]').length).toBeGreaterThan(0)
    expect(own.textContent).toContain('1000')
    expect(square('[data-calendar-view="t/Heat"]', '2026-11-06').querySelectorAll('[data-path]').length).toBeGreaterThan(0)
  })

  it('shows a field\'s value on hover', () => {
    const doc = calendarDocument({ from: '2026-03-01', to: '2026-03-07', layout: 'year', values: { '2026-03-04': 7 } })
    for (const day of doc[0].children[0].children) day.parameters._drawn_by_style = { Boolean: true }
    render(doc)
    const cell = square('.overseer-calendar', '2026-03-04')
    cell.firstElementChild.dispatchEvent(new window.MouseEvent('mouseover', { bubbles: true }))
    expect(cell.title).toBe('2026-03-04: 7')
  })
})

// Such a day comes from the server without its children, and a change after a write can still
// name something under it - a press setting `Days/<date>/total`. The page holds nothing there to
// change, so it passes over it, rather than taking it for a misfit and reading the document whole.
describe('a change under a square sent without its children', () => {
  beforeEach(() => setupCalendarDOM())

  const sentBare = () => {
    const doc = documentOfDivs()
    for (const day of doc[0].children[0].children) {
      if (day.parameters._drawn_by_style) delete day.children
    }
    return doc
  }

  it('is passed over, whatever kind it is', () => {
    const app = render(sentBare())
    app.readWholeAfterAMisfit = vi.fn()
    const square = app.currentDocument[0].children[0].children[1]
    const touched = app.applyDocumentChanges(app.currentDocument, [
      { kind: 'parameters', address: 't/Days/2026-11-03/total', path: [0, 0, 1, 0], parameters: { value: { Integer: 9 } } },
      { kind: 'subtree', address: 't/Days/2026-11-03/total', path: [0, 0, 1, 0], node: field('total', { value: { Integer: 9 } }) },
      { kind: 'subtree', address: 't/Days/2026-11-03/meals/breakfast', path: [0, 0, 1, 2, 0], node: field('breakfast', {}) },
      { kind: 'entries', address: 't/Days/2026-11-03/meals', path: [0, 0, 1, 2], entries: [] },
    ])
    expect(touched.misfit).toBeFalsy()
    expect(touched).toHaveLength(0)
    expect(app.readWholeAfterAMisfit).not.toHaveBeenCalled()
    expect(square.children).toBeUndefined()
  })

  it('leaves the square drawn as it was', () => {
    const app = render(sentBare())
    app.applyDocumentChanges(app.currentDocument, [
      { kind: 'parameters', address: 't/Days/2026-11-03/total', path: [0, 0, 1, 0], parameters: { value: { Integer: 9 } } },
    ])
    expect(square('[data-calendar-view="t/Heat"]', '2026-11-03').style.backgroundColor).toBe('red')
  })
})
