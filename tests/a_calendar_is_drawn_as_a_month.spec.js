import { describe, it, expect, beforeEach, vi } from 'vitest'
import { setupCalendarDOM, calendarDocument, calendarElement, drawnInOrder } from './helpers/calendar.js'

// `layout="month"`: weeks as rows of seven columns, Monday first, the weekdays named above and
// each day's date above it. The days of the neighbouring months that fill the first and the last
// week are blanks, their dates dimmed.

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }))
import { OverseerApp } from '../src/main.js'

const render = (doc) => {
  const app = new OverseerApp()
  app.currentDocument = doc
  app._currentText = 'TEXT'
  app.renderer.renderDocument(app.currentDocument)
}

describe('a calendar drawn as a month', () => {
  beforeEach(() => setupCalendarDOM())

  // November 2026 begins on a Sunday and ends on a Monday: six weeks.
  const november = (more = {}) => calendarDocument({ from: '2026-11-01', to: '2026-11-30', layout: 'month', ...more })

  it('is seven columns, Monday first', () => {
    render(november())
    expect(calendarElement().style.gridTemplateColumns).toBe('repeat(7, minmax(0, 1fr))')
    expect(drawnInOrder().slice(0, 7)).toEqual(['#Mon', '#Tue', '#Wed', '#Thu', '#Fri', '#Sat', '#Sun'])
  })

  it('puts the first day under its weekday, with blanks before it and dimmed dates above them', () => {
    render(november())
    const drawn = drawnInOrder()
    expect(drawn.slice(7, 14)).toEqual(['#26', '#27', '#28', '#29', '#30', '#31', '#1'])
    expect(drawn.slice(14, 21)).toEqual(['-', '-', '-', '-', '-', '-', '2026-11-01'])
    const dimmed = Array.from(calendarElement().querySelectorAll('.overseer-grid-heading.outside')).map(h => h.textContent)
    expect(dimmed).toEqual(['26', '27', '28', '29', '30', '31', '1', '2', '3', '4', '5', '6'])
  })

  it('draws every day once, in weeks of seven', () => {
    render(november())
    const drawn = drawnInOrder()
    expect(drawn.filter(d => /^\d{4}-/.test(d))).toHaveLength(30)
    // The weekday row, then six weeks of a row of dates and a row of days; the last week ends
    // with its Monday, the 30th.
    expect(drawn.length).toBe(7 + 5 * 14 + 7 + 1)
    expect(drawn.slice(-8)).toEqual(['#30', '#1', '#2', '#3', '#4', '#5', '#6', '2026-11-30'])
  })

  it('leaves the headings out when it says header=false', () => {
    render(november({ header: false }))
    expect(calendarElement().querySelectorAll('.overseer-grid-heading')).toHaveLength(0)
    expect(drawnInOrder().slice(0, 7)).toEqual(['-', '-', '-', '-', '-', '-', '2026-11-01'])
  })
})
