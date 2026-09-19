import { it, expect, vi } from 'vitest'
import { readFileSync } from 'node:fs'

// The filter against a real project document holding 120 open items, resolved by a real server.
//
// This is the size it exists for. A filter that is correct on four rows and slow or wrong on a
// hundred would have missed the point, so the numbers below are measured rather than assumed.

const SP = 'C:/Users/sc/AppData/Local/Temp/claude/e--Source-Repos-Overseer/171c2ed1-4b26-4cee-a303-b66b716dc586/scratchpad'

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }))
import { OverseerApp } from '../src/main.js'

const find = (nodes, name) => {
  for (const node of nodes) {
    if (node.name === name) return node
    const found = find(node.children || [], name)
    if (found) return found
  }
  return null
}

it('filters 120 real items, quickly and without touching the document', () => {
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

  const doc = JSON.parse(readFileSync(`${SP}/project_120.json`, 'utf8'))
  const app = new OverseerApp()
  app.currentDocument = doc
  app._currentText = 'TEXT'
  app.renderer._filters = new Map()
  app.renderer.renderDocument(doc)

  const items = find(doc, 'Items').children
  expect(items.length, 'the fixture is not the size this is testing').toBe(120)

  // Counted as entries, not as elements: a transparent layout div contributes no path
  // segment, so the row and the div inside it share one path and both get marked.
  const rows = () => {
    const seen = new Map()
    for (const element of document.querySelectorAll('[data-path]')) {
      let path
      try { path = JSON.parse(element.dataset.path) } catch { continue }
      if (path.length !== 3 || path[1] !== 'Items') continue
      const key = element.dataset.path
      seen.set(key, (seen.get(key) ?? true) && !element.classList.contains('filtered-out'))
    }
    return seen
  }
  const hidden = () => [...rows().values()].filter(on => !on).length
  const count = () => document.querySelector('.filter-count').textContent
  const box = document.querySelector('.filter-text')
  const type = (what) => {
    box.value = what
    box.dispatchEvent(new Event('input', { bubbles: true }))
  }

  expect(hidden(), 'something was hidden before anything was typed').toBe(0)
  expect(count()).toBe('120')

  // What it costs to type one character over a hundred and twenty rows. The document-side
  // alternative was a parse and resolve per keystroke, measured at 289ms on a smaller document.
  const started = performance.now()
  type('brace')
  const took = performance.now() - started

  const showing = 120 - hidden()
  const expected = items.filter((entry) => {
    // Wherever the layout puts it, which is not among the entry's own children.
    const field = (name) => {
      const seek = (node) => {
        for (const child of node.children || []) {
          if (child && child.name === name) return child
          const found = seek(child)
          if (found) return found
        }
        return null
      }
      const child = seek(entry)
      const value = child && (child.parameters._computed_value ?? child.parameters.value)
      return value && typeof value === 'object' ? String(Object.values(value)[0]) : ''
    }
    return `${field('title')} ${field('commentary')}`.toLowerCase().includes('brace')
  }).length

  expect(showing, 'the filter and the document disagree about what matches').toBe(expected)
  expect(showing).toBeGreaterThan(0)
  expect(showing).toBeLessThan(120)
  expect(count()).toBe(`${showing} of 120`)
  // 338ms before the element lookup was gathered once instead of per row, which is what the
  // document-side filter would have cost. Bounded generously because jsdom is far slower at
  // this than a browser - the point of the number is to catch a return to per-row scanning.
  console.warn(`  one keystroke over 120 rows: ${took.toFixed(0)}ms`)
  expect(took, `a keystroke took ${took.toFixed(0)}ms over 120 rows`).toBeLessThan(120)

  // And clearing it brings everything back rather than leaving rows stranded.
  type('')
  expect(hidden()).toBe(0)
})
