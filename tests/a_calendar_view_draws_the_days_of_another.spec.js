import { describe, it, expect, beforeEach, vi } from 'vitest'
import { setupCalendarDOM, calendarDocument } from './helpers/calendar.js'

// A view - a calendar with `of=` - draws the days of the calendar it shows, those in its own
// range, at the days' own paths and in its own layout. What its own `entry=` gives a day, which
// the server works out at the day as `_view@<view>@<parameter>`, is drawn in that view alone.

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }))
import { OverseerApp } from '../src/main.js'

const view = (name, layout) => ({
  name, node_type: 'calendar', is_hierarchy_transparent: false,
  source_id: '', source_fingerprint: 1, param_order: [], authored_dash: false,
  parameters: {
    of: { String: '../Days' }, layout: { String: layout },
    _view_of: { String: 't/Days' }, _view_at: { String: `t/${name}` },
  },
})

// Days from the 2nd to the 6th; the calendar shows the 6th, the agenda the 4th to the 6th, and
// the heatmap all five, red where the value is over 3.
const documentWithViews = () => {
  const doc = calendarDocument({ from: '2026-11-02', to: '2026-11-06', layout: 'day', values: { '2026-11-03': 5 } })
  const tab = doc[0]
  for (const day of tab.children[0].children) {
    const p = day.parameters
    p._in_view = { Boolean: day.name === '2026-11-06' }
    p['_in_view@t/Recent'] = { Boolean: day.name >= '2026-11-04' }
    p['_in_view@t/Heat'] = { Boolean: true }
    p['_view@t/Heat@background-color'] = { Formula: 'value > 3 ? "red" : ""' }
    p['_computed__view@t/Heat@background-color'] = { String: p.value.Integer > 3 ? 'red' : '' }
  }
  // A view arrives with no children at all.
  tab.children.push(view('Recent', 'agenda'), view('Heat', 'year'))
  return doc
}

const render = (doc) => {
  const app = new OverseerApp()
  app.currentDocument = doc
  app._currentText = 'TEXT'
  app.renderer.renderDocument(app.currentDocument)
  return app
}

const days = (selector) => Array.from(document.querySelector(selector).children)
  .filter(el => el.dataset && el.dataset.path)
  .map(el => JSON.parse(el.dataset.path))

describe('a calendar view', () => {
  beforeEach(() => setupCalendarDOM())

  it('draws the days of the calendar it shows in its own range, at their own paths', () => {
    render(documentWithViews())
    expect(days('[data-calendar-view="t/Recent"]')).toEqual([
      ['t', 'Days', '2026-11-04'], ['t', 'Days', '2026-11-05'], ['t', 'Days', '2026-11-06'],
    ])
    expect(days('[data-calendar-view="t/Heat"]').length).toBe(5)
    expect(days('.overseer-calendar:not([data-calendar-view])')).toEqual([['t', 'Days', '2026-11-06']])
  })

  it('draws in its own layout', () => {
    render(documentWithViews())
    expect(document.querySelector('[data-calendar-view="t/Recent"]').classList.contains('calendar-agenda')).toBe(true)
    expect(document.querySelector('[data-calendar-view="t/Heat"]').classList.contains('calendar-year')).toBe(true)
  })

  it('styles the days with its own entry, and only in that view', () => {
    render(documentWithViews())
    const cell = (selector, day) => document.querySelector(`${selector} > [data-path='${JSON.stringify(['t', 'Days', day])}']`)
    expect(cell('[data-calendar-view="t/Heat"]', '2026-11-03').style.backgroundColor).toBe('red')
    expect(cell('[data-calendar-view="t/Heat"]', '2026-11-04').style.backgroundColor).toBe('')
    expect(cell('[data-calendar-view="t/Recent"]', '2026-11-05').style.backgroundColor).not.toBe('red')
  })

  it('takes a colour the document now gives a day, once updated', () => {
    const app = render(documentWithViews())
    const after = documentWithViews()
    const p = after[0].children[0].children.find(d => d.name === '2026-11-04').parameters
    p.value = { Integer: 9 }
    p['_computed__view@t/Heat@background-color'] = { String: 'red' }
    app.currentDocument = after
    app.renderer.restyleCalendarViews(after)
    const square = document.querySelector(`[data-calendar-view="t/Heat"] > [data-path='${JSON.stringify(['t', 'Days', '2026-11-04'])}']`)
    expect(square.style.backgroundColor).toBe('red')
  })
})
