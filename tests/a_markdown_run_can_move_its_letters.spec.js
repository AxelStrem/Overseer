import { describe, it, expect, beforeEach, vi } from 'vitest'

// `<motion=wave | ...>` in markdown draws each letter of the run in its own span, numbered so that
// the stylesheet can move each a little behind the one before it. It shares the mark of
// `<color=... | ...>`, and can stand beside a colour in it.

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }))
import { OverseerApp } from '../src/main.js'
import { nameAsText } from '../src/renderer.js'

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

const drawn = (text) => {
  const holder = document.createElement('div')
  holder.innerHTML = new OverseerApp().renderer.renderMarkdown(text)
  return holder
}
const letters = (el) => Array.from(el.querySelectorAll('.md-letter'))

describe('a markdown run can move its letters', () => {
  beforeEach(() => setupDOM())

  it('draws each letter of a moving run in its own numbered span, and keeps its words whole', () => {
    const el = drawn('Say <motion=wave | hi you> now')
    const run = el.querySelector('.md-motion.md-motion-wave')
    expect(run).toBeTruthy()
    expect(letters(run).map((l) => l.textContent)).toEqual(['h', 'i', 'y', 'o', 'u'])
    expect(letters(run).map((l) => l.style.getPropertyValue('--i'))).toEqual(['0', '1', '2', '3', '4'])
    expect(Array.from(run.querySelectorAll('.md-word')).map((w) => w.textContent)).toEqual(['hi', 'you'])
    expect(el.textContent.trim()).toBe('Say hi you now')
  })

  it('takes a colour beside the motion, in either order', () => {
    for (const text of ['<color=#E91E63 motion=rainbow | ok>', '<motion=rainbow color=#E91E63 | ok>']) {
      const run = drawn(text).querySelector('.md-motion-rainbow')
      expect(run.classList.contains('md-inline-color')).toBe(true)
      expect(run.style.color).toBe('rgb(233, 30, 99)')
      expect(letters(run)).toHaveLength(2)
    }
  })

  it('leaves markdown and icons inside the run whole', () => {
    const el = drawn('<motion=jitter | **bold** <icon=clock> &amp; done>')
    const run = el.querySelector('.md-motion-jitter')
    expect(run.querySelector('strong').textContent).toBe('bold')
    expect(letters(run.querySelector('strong'))).toHaveLength(4)
    expect(run.querySelector('.icon')).toBeTruthy()
    expect(run.querySelector('.icon').closest('.md-letter')).toBeNull()
    expect(letters(run).some((l) => l.textContent === '&')).toBe(true)
    expect(run.textContent).toBe('bold  & done')
  })

  it('keeps a letter made of several code points as one letter', () => {
    const run = drawn('<motion=pulse | a👍🏽b>').querySelector('.md-motion-pulse')
    expect(letters(run).map((l) => l.textContent)).toEqual(['a', '👍🏽', 'b'])
  })

  it('numbers each run from the start', () => {
    const el = drawn('<motion=wave | ab> and <motion=wave | cd>')
    const runs = el.querySelectorAll('.md-motion-wave')
    expect(runs).toHaveLength(2)
    expect(letters(runs[1]).map((l) => l.style.getPropertyValue('--i'))).toEqual(['0', '1'])
    expect(el.textContent.trim()).toBe('ab and cd')
  })

  it('moves nothing for a motion it does not know, or one played once, but keeps the colour', () => {
    for (const way of ['spin', 'pop']) {
      const el = drawn(`<motion=${way} | still>`)
      expect(el.querySelector('.md-motion')).toBeNull()
      expect(letters(el)).toHaveLength(0)
      expect(el.textContent.trim()).toBe('still')
    }
    const coloured = drawn('<color=red motion=spin | still>').querySelector('.md-inline-color')
    expect(coloured.style.color).toBe('red')
    expect(coloured.classList.contains('md-motion')).toBe(false)
  })

  it('draws a plain colour exactly as before', () => {
    expect(drawn('<color=red | x>').innerHTML.trim()).toBe('<p><span class="md-inline-color" style="color:red">x</span></p>')
  })

  it('shows the mark as written inside code', () => {
    const el = drawn('`<motion=wave | hi>`')
    expect(el.querySelector('code').textContent).toBe('<motion=wave | hi>')
    expect(letters(el)).toHaveLength(0)
  })

  it('is plain text where only text can go', () => {
    expect(nameAsText('<motion=wave color=red | Team>')).toBe('Team')
  })
})
