import { describe, it, expect, beforeEach, vi } from 'vitest'
import { setupCalendarDOM, calendarDocument, calendarElement, drawnInOrder } from './helpers/calendar.js'

// `layout="day"`: the first day in range, headed by its date written out. The others stay in the
// page, hidden by the stylesheet, so a day drawn again after an edit is hidden as well.

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }))
import { OverseerApp } from '../src/main.js'

const render = (doc) => {
  const app = new OverseerApp()
  app.currentDocument = doc
  app._currentText = 'TEXT'
  app.renderer.renderDocument(app.currentDocument)
}

describe('a calendar drawn as a day', () => {
  beforeEach(() => setupCalendarDOM())

  it('heads its first day with the date written out', () => {
    render(calendarDocument({ from: '2026-11-04', to: '2026-11-04', layout: 'day' }))
    expect(drawnInOrder()).toEqual(['#Wednesday, 4 November 2026', '2026-11-04'])
    expect(calendarElement().classList.contains('calendar-day')).toBe(true)
  })

  it('leaves the date out when it says header=false', () => {
    render(calendarDocument({ from: '2026-11-04', to: '2026-11-04', layout: 'day', header: false }))
    expect(drawnInOrder()).toEqual(['2026-11-04'])
  })
})
