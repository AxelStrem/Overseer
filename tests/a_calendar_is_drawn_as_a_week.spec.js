import { describe, it, expect, beforeEach, vi } from 'vitest'
import { setupCalendarDOM, calendarDocument, calendarElement, drawnInOrder } from './helpers/calendar.js'

// `layout="week"`: seven columns, Monday first, each day headed by its weekday and its date.

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }))
import { OverseerApp } from '../src/main.js'

const render = (doc) => {
  const app = new OverseerApp()
  app.currentDocument = doc
  app._currentText = 'TEXT'
  app.renderer.renderDocument(app.currentDocument)
}

describe('a calendar drawn as a week', () => {
  beforeEach(() => setupCalendarDOM())

  it('is seven columns, Monday first, each headed by its weekday and date', () => {
    render(calendarDocument({ from: '2026-11-02', to: '2026-11-08', layout: 'week' }))
    expect(calendarElement().style.gridTemplateColumns).toBe('repeat(7, minmax(0, 1fr))')
    expect(drawnInOrder()).toEqual([
      '#Mon 2', '#Tue 3', '#Wed 4', '#Thu 5', '#Fri 6', '#Sat 7', '#Sun 8',
      '2026-11-02', '2026-11-03', '2026-11-04', '2026-11-05', '2026-11-06', '2026-11-07', '2026-11-08',
    ])
  })

  it('keeps a day under its weekday when the range starts mid-week', () => {
    render(calendarDocument({ from: '2026-11-05', to: '2026-11-11', layout: 'week', header: false }))
    expect(drawnInOrder()).toEqual([
      '-', '-', '-', '2026-11-05', '2026-11-06', '2026-11-07', '2026-11-08',
      '2026-11-09', '2026-11-10', '2026-11-11',
    ])
  })
})
