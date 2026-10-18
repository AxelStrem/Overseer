import { describe, it, expect, beforeEach, vi } from 'vitest'
import { readFileSync } from 'node:fs'

// The theme's `mono` key is the font the values of string and number fields, code and the
// markdown editor are drawn in, as `font` is the font of everything else. jsdom does not work out
// `var()`, so the font itself is seen in the real page; this holds the parts together.

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

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(async () => null) }))
import { OverseerApp } from '../src/main.js'

const styles = readFileSync('src/styles.css', 'utf8').replace(/\r\n/g, '\n')

/** The declarations of every rule whose selector is exactly `selector`, together. */
function rule(selector) {
  const rules = []
  for (let at = styles.indexOf(`\n${selector} {`); at > -1; at = styles.indexOf(`\n${selector} {`, at + 1)) {
    rules.push(styles.slice(at, styles.indexOf('}', at)))
  }
  expect(rules.length, `no rule for ${selector}`).toBeGreaterThan(0)
  return rules.join('\n')
}

const key = (name, value) => ({
  name, node_type: '-', children: [], is_hierarchy_transparent: false,
  parameters: { value: { String: value } },
})

const theme = (...children) => ({
  name: 'theme', node_type: 'context', is_hierarchy_transparent: false, children,
  parameters: { _context_copy: { Boolean: true } },
})

describe('the theme\'s mono font', () => {
  beforeEach(() => {
    setupDOM()
    document.documentElement.removeAttribute('style')
  })

  it('is what string and number values, code, errors and the markdown editor are drawn in', () => {
    for (const selector of [
      '.string-field .field-value',
      '.number-field .field-value',
      '.error-details',
      '.markdown-editor',
      ':is(.markdown-enabled, .markdown-preview) code',
    ]) {
      expect(rule(selector), selector).toContain('font-family: var(--ov-mono);')
    }
  })

  it('is monospace where no theme says otherwise', () => {
    expect(rule(':root')).toMatch(/--ov-mono: [^;]*monospace;/)
    const shipped = readFileSync('library/themes/default.os', 'utf8')
    expect(shipped).toMatch(/- mono = "[^"]*monospace"/)
  })

  it('is written on the container whose theme says it', () => {
    const app = new OverseerApp()
    app.currentDocument = [
      { name: 't', node_type: 'tab', parameters: {}, is_hierarchy_transparent: true, children: [
        { name: 'side', node_type: 'div', parameters: {}, is_hierarchy_transparent: false, children: [
          theme(key('mono', 'Georgia')),
          { name: 'title', node_type: 'string', children: [], is_hierarchy_transparent: false,
            parameters: { value: { String: 'Dune' } } },
        ] },
      ] },
    ]
    app.renderer.renderDocument(app.currentDocument)

    const side = document.querySelector(`[data-path='${JSON.stringify(['t', 'side'])}']`)
    expect(side.style.getPropertyValue('--ov-mono')).toBe('Georgia')
    expect(side.style.getPropertyValue('--ov-theme-mono')).toBe('Georgia')
  })
})
