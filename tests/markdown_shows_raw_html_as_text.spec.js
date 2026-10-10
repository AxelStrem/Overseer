import { describe, it, expect, beforeEach, vi } from 'vitest'

// Markdown in a text field is markdown, and raw HTML in it is shown as the text it is.
//
// A project item's comments are markdown written by agents as well as the owner, quoting code and
// pages as they go, and marked passed raw HTML through untouched: an `<img onerror>` in a comment
// would have run in the page, which holds the server's token. The one tag kept is the span the
// colour syntax becomes; a link or an image goes only to the web, to mail, or to this server.

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }))
import { OverseerApp } from '../src/main.js'

function setupDOM() {
  document.body.innerHTML = `
    <div id="app">
      <div id="toolbar"><button id="open-file-btn"></button><button id="new-file-btn"></button></div>
      <button id="welcome-open-btn"></button><button id="welcome-new-btn"></button>
      <button id="error-back-btn"></button>
      <div id="tab-container"></div><div id="content-display"></div>
      <div id="status-bar"><span id="status-message"></span><span id="status-info"></span>
        <span id="file-path"></span></div>
      <div id="welcome-screen" class="screen"></div><div id="editor-screen" class="screen"></div>
      <div id="error-screen" class="screen"></div><div id="error-message"></div>
    </div>`
}

let renderer
const drawn = (text) => {
  const holder = document.createElement('div')
  holder.innerHTML = renderer.renderMarkdown(text)
  return holder
}

describe('markdown with raw HTML in it', () => {
  beforeEach(() => {
    setupDOM()
    renderer = new OverseerApp().renderer
  })

  it('shows an inline tag as text, and runs nothing', () => {
    const out = drawn('Found it: <img src=x onerror="window.ran=1"> in the page')
    expect(out.querySelector('img')).toBeNull()
    expect(out.textContent).toContain('<img src=x onerror="window.ran=1">')
  })

  it('shows a block of HTML as text', () => {
    const out = drawn('<div onclick="window.ran=1">\nhi\n</div>\n\nafter')
    expect(out.querySelector('[onclick]')).toBeNull()
    expect(out.textContent).toContain('<div onclick="window.ran=1">')
    expect(out.textContent).toContain('after')
  })

  it('still draws the markdown around it', () => {
    const out = drawn('**built** - `<b>not bold</b>`\n\n- one\n- two')
    expect(out.querySelector('strong').textContent).toBe('built')
    expect(out.querySelector('code').textContent).toBe('<b>not bold</b>')
    expect(out.querySelectorAll('li')).toHaveLength(2)
  })

  it('keeps a link to the web, to mail, or to this server', () => {
    const hrefs = (text) => Array.from(drawn(text).querySelectorAll('a')).map((a) => a.getAttribute('href'))
    expect(hrefs('[a](https://example.com/x?y=1&z=2) [b](mailto:me@example.com) [c](docs/guide.md) [d](/doc/tasks.os)'))
      .toEqual(['https://example.com/x?y=1&z=2', 'mailto:me@example.com', 'docs/guide.md', '/doc/tasks.os'])
  })

  it('drops a link or an image that would run something, keeping its words', () => {
    for (const text of ['[click](javascript:window.ran=1)', '[click](JavaScript:window.ran=1)',
                        '[click](data:text/html,<script>window.ran=1</script>)', '<javascript:window.ran=1>']) {
      const out = drawn(text)
      const hrefs = Array.from(out.querySelectorAll('a')).map((a) => a.getAttribute('href'))
      expect(hrefs.filter((h) => /^\s*(javascript|data):/i.test(h)), text).toEqual([])
    }
    const out = drawn('![x](javascript:window.ran=1)')
    expect(out.querySelector('img')).toBeNull()
    // An entity in a link's target is written as the text it is, never decoded into a scheme.
    const smuggled = drawn('[click](javascript&colon;window.ran=1)').querySelector('a')
    if (smuggled) expect(smuggled.getAttribute('href')).not.toMatch(/^javascript:/i)
  })

  it('keeps the colour syntax and nothing that only looks like it', () => {
    const kept = drawn('This is <color=#FF0000 | red> text')
    expect(kept.querySelector('span.md-inline-color').getAttribute('style')).toBe('color:#FF0000')
    const forged = drawn('<span class="md-inline-color" style="color:red;background:url(x)">x</span>')
    expect(forged.querySelector('span')).toBeNull()
  })
})
