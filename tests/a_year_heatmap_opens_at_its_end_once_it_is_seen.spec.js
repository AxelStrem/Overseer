import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'
import { setupCalendarDOM, calendarDocument, calendarElement } from './helpers/calendar.js'

// A year heatmap wider than the screen scrolls sideways and opens at its end, the latest days.
// One drawn on a tab not yet open has no size, so it is placed when it first gets one; where the
// reader scrolled it, it keeps their place, through a redraw too.

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }))
import { OverseerApp } from '../src/main.js'

// jsdom lays nothing out: a year is 832 pixels wide in a 368 one while `shown`, and nothing while
// hidden, and the ResizeObserver is told by hand when its size changes.
const WIDE = 832, SCREEN = 368
let shown = true
let observers = []
const saved = {}
const fake = (name, get, set) => {
  saved[name] = Object.getOwnPropertyDescriptor(window.Element.prototype, name)
  Object.defineProperty(window.Element.prototype, name, { configurable: true, get, set })
}

const resized = () => { for (const o of observers) o.callback([]) }

const render = (app, doc) => {
  app.currentDocument = doc
  app._currentText = 'TEXT'
  app.renderer.renderDocument(app.currentDocument)
}
const year = () => calendarDocument({ from: '2025-11-22', to: '2026-11-21', layout: 'year' })
const scrollBy = (el, to) => { el.scrollLeft = to; el.dispatchEvent(new window.Event('scroll')) }
// The scroll event a scroll set by the page itself is followed by.
const settle = (el) => el.dispatchEvent(new window.Event('scroll'))

describe('a year heatmap on a narrow screen', () => {
  beforeEach(() => {
    setupCalendarDOM()
    shown = true
    observers = []
    const year = (el) => el.classList && el.classList.contains('calendar-year')
    fake('clientWidth', function () { return year(this) && shown ? SCREEN : 0 })
    fake('scrollWidth', function () { return year(this) && shown ? WIDE : 0 })
    fake('scrollLeft', function () { return this._left || 0 }, function (v) {
      this._left = Math.max(0, Math.min(Number(v) || 0, this.scrollWidth - this.clientWidth))
    })
    globalThis.ResizeObserver = class {
      constructor(callback) { this.callback = callback; observers.push(this) }
      observe() { }
      disconnect() { observers = observers.filter(o => o !== this) }
    }
  })
  afterEach(() => {
    for (const [name, d] of Object.entries(saved)) {
      if (d) Object.defineProperty(window.Element.prototype, name, d)
      else delete window.Element.prototype[name]
    }
    delete globalThis.ResizeObserver
  })

  it('opens scrolled to its end', () => {
    render(new OverseerApp(), year())
    resized()
    expect(calendarElement().scrollLeft).toBe(WIDE - SCREEN)
  })

  it('drawn while hidden, goes to its end once it is shown', () => {
    shown = false
    render(new OverseerApp(), year())
    resized()
    expect(calendarElement().scrollLeft).toBe(0)
    shown = true
    resized()
    expect(calendarElement().scrollLeft).toBe(WIDE - SCREEN)
  })

  it('keeps the place the reader scrolled it to, through a redraw', () => {
    const app = new OverseerApp()
    render(app, year())
    resized()
    settle(calendarElement())
    scrollBy(calendarElement(), 100)
    resized()
    expect(calendarElement().scrollLeft).toBe(100)
    render(app, year())
    resized()
    expect(calendarElement().scrollLeft).toBe(100)
  })

  it('follows its end again once the reader scrolls back to it', () => {
    const app = new OverseerApp()
    render(app, year())
    resized()
    scrollBy(calendarElement(), 100)
    scrollBy(calendarElement(), WIDE - SCREEN)
    render(app, year())
    resized()
    expect(calendarElement().scrollLeft).toBe(WIDE - SCREEN)
  })
})
