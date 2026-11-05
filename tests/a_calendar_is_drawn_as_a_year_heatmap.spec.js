import { describe, it, expect, beforeEach, vi } from 'vitest'
import { setupCalendarDOM, calendarDocument, calendarElement, drawnInOrder } from './helpers/calendar.js'

// `layout="year"`: the heatmap. Weeks are columns and weekdays rows, Monday on top, so the page
// flows the days down a column and on to the next; each column starts with the name of the
// month that begins in it, and the first column names Mon, Wed and Fri. A day shows its value on
// hover.

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }))
import { OverseerApp } from '../src/main.js'

const render = (doc) => {
  const app = new OverseerApp()
  app.currentDocument = doc
  app._currentText = 'TEXT'
  app.renderer.renderDocument(app.currentDocument)
}

// 2026-01-01 is a Thursday, so the year spans 53 weeks.
const year = (more = {}) => calendarDocument({ from: '2026-01-01', to: '2026-12-31', layout: 'year', ...more })

// The drawing cut into its columns: the weekday column, then one per week.
const columns = (drawn, headed = true) => {
  const height = headed ? 8 : 7
  const out = []
  for (let i = 0; i < drawn.length; i += height) out.push(drawn.slice(i, i + height))
  return out
}

describe('a calendar drawn as a year heatmap', () => {
  beforeEach(() => setupCalendarDOM())

  it('runs the weekdays down, Monday first, and the weeks across', () => {
    render(year())
    expect(calendarElement().style.gridTemplateRows).toBe('auto repeat(7, var(--ov-calendar-square))')
    const cols = columns(drawnInOrder())
    expect(cols[0]).toEqual(['#', '#Mon', '#', '#Wed', '#', '#Fri', '#', '#'])
    expect(cols[1]).toEqual(['#Jan', '-', '-', '-', '2026-01-01', '2026-01-02', '2026-01-03', '2026-01-04'])
    expect(cols).toHaveLength(1 + 53)
    expect(drawnInOrder().filter(d => /^\d{4}-/.test(d))).toHaveLength(365)
  })

  it('names each month above the week it begins in', () => {
    render(year())
    const months = columns(drawnInOrder()).slice(1).map(c => c[0].slice(1)).filter(Boolean)
    expect(months).toEqual(['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', 'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec'])
    // February begins on a Sunday, in the fifth week.
    expect(columns(drawnInOrder())[5]).toEqual(['#Feb', '2026-01-26', '2026-01-27', '2026-01-28', '2026-01-29', '2026-01-30', '2026-01-31', '2026-02-01'])
  })

  it('leaves the names out when it says header=false', () => {
    render(year({ header: false }))
    expect(calendarElement().querySelectorAll('.overseer-grid-heading')).toHaveLength(0)
    expect(drawnInOrder().slice(0, 7)).toEqual(['-', '-', '-', '2026-01-01', '2026-01-02', '2026-01-03', '2026-01-04'])
  })

  it('shows a day\'s date and value on hover', () => {
    render(year({ values: { '2026-03-04': 7 } }))
    const cell = Array.from(calendarElement().children).find(el => el.dataset.path && el.dataset.path.includes('2026-03-04'))
    cell.firstElementChild.dispatchEvent(new window.MouseEvent('mouseover', { bubbles: true }))
    expect(cell.title).toBe('2026-03-04: 7')
  })
})
