import { describe, it, expect, beforeEach, vi } from 'vitest'
import { readFileSync } from 'node:fs'

// A kind of element can have the theme's general keys of its own:
// `context theme { - button { - accent = ... } }`. The page writes it as `--ov-button-accent`, and
// each general key a second time as `--ov-theme-<key>`; the stylesheet gives each kind's element
// the general keys again from its own, or else from those. jsdom does not work out `var()`, so the
// colours themselves are seen in the real page; this holds the parts together.

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

const GENERAL = ['background', 'surface', 'text', 'muted', 'line', 'accent', 'success', 'warning', 'danger', 'font', 'mono', 'radius']
const KINDS = {
  button: '.overseer-button',
  checkbox: '.checkbox-field input[type="checkbox"]',
  chip: '.tag-chip',
  tab: '.tab-bar',
  table: '.overseer-list.view-table',
  field: '.overseer-field',
  chart: '.overseer-chart',
  separator: '.mark-separator',
  filter: '.overseer-filter',
}

const key = (name, value) => ({
  name, node_type: '-', children: [], is_hierarchy_transparent: false,
  parameters: { value: { String: value } },
})

const kind = (name, children) => ({ name, node_type: '-', children, is_hierarchy_transparent: false, parameters: {} })

const theme = (...children) => ({
  name: 'theme', node_type: 'context', is_hierarchy_transparent: false, children,
  parameters: { _context_copy: { Boolean: true } },
})

const div = (name, children) => ({
  name, node_type: 'div', parameters: {}, is_hierarchy_transparent: false, children,
})

const button = (name) => ({
  name, node_type: 'button', children: [], is_hierarchy_transparent: false,
  parameters: { label: { String: name } },
})

const buttonAt = (path) => document.querySelector(`button[data-path='${JSON.stringify(path)}']`)

/** The nearest element above `element` that says `name`, and what it says. */
function said(element, name) {
  for (let at = element.parentElement; at; at = at.parentElement) {
    const value = at.style && at.style.getPropertyValue(name)
    if (value) return value.trim()
  }
  return null
}

describe('a kind of element in the theme', () => {
  beforeEach(() => {
    setupDOM()
    document.documentElement.removeAttribute('style')
  })

  it('has its own keys written beside the container\'s general ones', () => {
    const app = new OverseerApp()
    app.currentDocument = [
      { name: 't', node_type: 'tab', parameters: {}, is_hierarchy_transparent: true, children: [
        div('side', [
          theme(key('accent', '#2563eb'), kind('button', [key('accent', '#7c3aed'), key('hover', 'lift')])),
          button('inside'),
        ]),
      ] },
    ]
    app.renderer.renderDocument(app.currentDocument)

    const inside = buttonAt(['t', 'side', 'inside'])
    expect(said(inside, '--ov-accent')).toBe('#2563eb')
    expect(said(inside, '--ov-theme-accent')).toBe('#2563eb')
    expect(said(inside, '--ov-button-accent')).toBe('#7c3aed')
    // A kind's own keys are not general ones.
    expect(said(inside, '--ov-theme-button-accent')).toBe(null)
    expect(said(inside, '--ov-theme-hover')).toBe(null)
  })

  it('is given every general key again, from its own or else the container\'s', () => {
    const styles = readFileSync('src/styles.css', 'utf8').replace(/\r\n/g, '\n')
    for (const [name, selector] of Object.entries(KINDS)) {
      const start = styles.indexOf(`--ov-background: var(--ov-${name}-background`)
      expect(start, `no block for ${name}`).toBeGreaterThan(-1)
      const open = styles.lastIndexOf('{', start)
      expect(styles.slice(styles.lastIndexOf('}', open) + 1, open), `${name} is not on ${selector}`).toContain(selector)
      const block = styles.slice(open, styles.indexOf('}', start))
      for (const general of GENERAL) {
        expect(block, `${name} has no ${general}`)
          .toContain(`--ov-${general}: var(--ov-${name}-${general}, var(--ov-theme-${general})) !important;`)
      }
    }
  })

  it('falls back, before a document is worked out, to the stylesheet\'s own keys', () => {
    const styles = readFileSync('src/styles.css', 'utf8')
    for (const general of GENERAL) {
      expect(styles, `no --ov-theme-${general} on the root`).toContain(`--ov-theme-${general}: var(--ov-${general});`)
    }
  })
})
