import { describe, it, expect, beforeEach, vi } from 'vitest'

// Code is where the colour tag is written about, so inside backticks or a fence it is shown as it
// was written - byte for byte, a colour that is no colour included - and coloured only outside.

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

describe('a colour tag in code', () => {
  beforeEach(() => setupDOM())

  it('is shown as written inside backticks', () => {
    const el = drawn('Write `<color=#e11d48 | text>` for red.')
    expect(el.querySelector('code').textContent).toBe('<color=#e11d48 | text>')
    expect(el.querySelector('.md-inline-color')).toBeNull()
  })

  it('keeps a colour that is no colour', () => {
    const el = drawn('The tag is `<color=... | ...>`, and it ends there.')
    expect(el.querySelector('code').textContent).toBe('<color=... | ...>')
    expect(el.textContent).toContain('and it ends there.')
  })

  it('is shown as written inside a fence', () => {
    const el = drawn('```text\nsay <color=red |  hi>\n```')
    expect(el.querySelector('pre code').textContent.trimEnd()).toBe('say <color=red |  hi>')
    expect(el.querySelector('.md-inline-color')).toBeNull()
  })

  it('still colours outside code beside one inside', () => {
    const el = drawn('<color=red | **warm**> and `<color=blue | cold>`')
    const span = el.querySelector('.md-inline-color')
    expect(span.getAttribute('style')).toBe('color:red')
    expect(span.querySelector('strong').textContent).toBe('warm')
    expect(el.querySelector('code').textContent).toBe('<color=blue | cold>')
  })

  it('is shown as written in a chip name', () => {
    expect(nameAsText('`<color=red | x>` <color=red | y>')).toBe('<color=red | x> y')
  })
})
