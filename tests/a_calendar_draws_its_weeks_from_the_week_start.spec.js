import { describe, it, expect, beforeEach, vi } from 'vitest'
import { setupCalendarDOM, calendarDocument, drawnInOrder } from './helpers/calendar.js'

// A calendar's weeks start on the `week_start` it sees - the copy of the context the resolver
// puts in front of a container's children, settled from the locale where the settings say
// "system" - and on Monday where it sees none.

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }))
import { OverseerApp } from '../src/main.js'

const render = (doc) => {
  const app = new OverseerApp()
  app.currentDocument = doc
  app._currentText = 'TEXT'
  app.renderer.renderDocument(app.currentDocument)
}

describe('a calendar drawn from its week_start', () => {
  beforeEach(() => setupCalendarDOM())

  it('starts a month on the week_start, the first day under its weekday', () => {
    // November 2026 begins on a Sunday: with weeks from Sunday there is no blank before it.
    render(calendarDocument({ from: '2026-11-01', to: '2026-11-30', layout: 'month', weekStart: 'sunday' }))
    const drawn = drawnInOrder()
    expect(drawn.slice(0, 7)).toEqual(['#Sun', '#Mon', '#Tue', '#Wed', '#Thu', '#Fri', '#Sat'])
    expect(drawn.slice(7, 14)).toEqual(['#1', '#2', '#3', '#4', '#5', '#6', '#7'])
    expect(drawn[14]).toBe('2026-11-01')
  })

  it('starts a week on the week_start', () => {
    render(calendarDocument({ from: '2026-11-04', to: '2026-11-10', layout: 'week', weekStart: 'wednesday' }))
    expect(drawnInOrder().slice(0, 7)).toEqual(['#Wed 4', '#Thu 5', '#Fri 6', '#Sat 7', '#Sun 8', '#Mon 9', '#Tue 10'])
  })

  it('runs a year\'s weekdays down from the week_start', () => {
    render(calendarDocument({ from: '2026-01-01', to: '2026-12-31', layout: 'year', weekStart: 'sunday' }))
    const drawn = drawnInOrder()
    expect(drawn.slice(0, 8)).toEqual(['#', '#Sun', '#', '#Tue', '#', '#Thu', '#', '#'])
    // 1 January 2026 is a Thursday, the fifth day of a week from Sunday.
    expect(drawn.slice(8, 16)).toEqual(['#Jan', '-', '-', '-', '-', '2026-01-01', '2026-01-02', '2026-01-03'])
  })

  it('starts on Monday where it sees no week_start, or one that is no day', () => {
    render(calendarDocument({ from: '2026-11-02', to: '2026-11-08', layout: 'month', weekStart: 'someday' }))
    expect(drawnInOrder().slice(0, 7)).toEqual(['#Mon', '#Tue', '#Wed', '#Thu', '#Fri', '#Sat', '#Sun'])
  })
})
