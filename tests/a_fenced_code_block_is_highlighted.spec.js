import { describe, it, expect, vi } from 'vitest'

// A fenced block in a markdown text is drawn as code, coloured as the language named after its
// fence, or as the one it looks most like when none is named. What it holds stays text.

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(async () => null) }))
import { OverseerRenderer } from '../src/renderer.js'

const draw = (text) => {
  const holder = document.createElement('div')
  holder.innerHTML = new OverseerRenderer().renderMarkdown(text)
  return holder
}
const tokens = (el) => el.querySelectorAll('pre code span[class^="hljs-"]').length

describe('a fenced code block', () => {
  it('is coloured as the language its fence names', () => {
    const el = draw('```rust\nfn main() { let x = 1; }\n```')
    expect(el.querySelector('pre code').className).toBe('hljs language-rust')
    expect(el.querySelector('.hljs-keyword').textContent).toBe('fn')
    expect(el.querySelector('pre code').textContent).toBe('fn main() { let x = 1; }\n')
  })

  it('is coloured by a guess when no language is named', () => {
    const el = draw('```\n//code goes here\nint func()\n{\n   return 0;\n}\n```')
    expect(el.querySelector('pre code').classList.contains('hljs')).toBe(true)
    expect(tokens(el)).toBeGreaterThan(0)
  })

  it('stays plain when it says text, or names a language the page does not know', () => {
    for (const fence of ['text', 'brainfudge']) {
      const el = draw('```' + fence + '\nint x = 0;\n```')
      expect(tokens(el)).toBe(0)
      expect(el.querySelector('pre code').textContent).toBe('int x = 0;\n')
    }
  })

  it('shows the markup it holds as text', () => {
    for (const fence of ['html', '', 'nothing']) {
      const el = draw('```' + fence + '\n<script>alert(1)</script><b>x</b>\n```')
      expect(el.querySelector('script')).toBeNull()
      expect(el.querySelector('b')).toBeNull()
      expect(el.querySelector('pre code').textContent).toBe('<script>alert(1)</script><b>x</b>\n')
    }
  })

  it('keeps the markers it holds as written, where they are written about', () => {
    const r = new OverseerRenderer()
    const holder = document.createElement('div')
    holder.innerHTML = r.renderMarkdown('```js\nconst a = "<icon=clock> <#parser> 42"\n```\n\n`<icon=clock>` and <icon=clock>')
    expect(holder.querySelector('pre code').textContent).toBe('const a = "<icon=clock> <#parser> 42"\n')
    expect(holder.querySelector('pre .icon')).toBeNull()
    expect(holder.querySelector('p code').textContent).toBe('<icon=clock>')
    expect(holder.querySelectorAll('p > .icon').length).toBe(1)
  })
})
