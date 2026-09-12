import { describe, it, expect, beforeEach, vi } from 'vitest'
import { readFileSync } from 'node:fs'

// A filter over a list: text to match, and tags to narrow by.
//
// Client-side on purpose. In the document it would mean re-resolving on every keystroke - 289ms
// measured on a document smaller than this is meant for - and the filter would be *stored*:
// written to the file, committed by the backup, synced to whoever else is reading. A filter is a
// view, like which tab is open. What has to be true of it is that it writes nothing, that it
// matches the fields rather than what is on screen, and that it survives a repaint of the list.

function setupDOM() {
  document.body.innerHTML = `
    <div id="app">
      <div id="toolbar"><button id="open-file-btn"></button><button id="new-file-btn"></button>
        <button id="save-file-btn"></button><button id="reload-file-btn"></button></div>
      <button id="welcome-open-btn"></button><button id="welcome-new-btn"></button>
      <button id="error-back-btn"></button>
      <div id="tab-container"></div><div id="content-display"></div>
      <div id="status-bar"><span id="status-message"></span><span id="status-info"></span>
        <span id="file-path"></span></div>
      <div id="welcome-screen" class="screen"></div><div id="editor-screen" class="screen"></div>
      <div id="error-screen" class="screen"></div><div id="error-message"></div>
    </div>`
}

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }))
import { OverseerApp } from '../src/main.js'

const base = (name, node_type) => ({
  name, node_type, parameters: {}, children: [], is_hierarchy_transparent: false,
  source_id: '', source_fingerprint: 1, param_order: [], authored_dash: false,
})

const valued = (name, type, variant, value) => Object.assign(base(name, type), {
  parameters: { value: { [variant]: value } },
})

const label = (tag, colour) => Object.assign(base(`Label__${tag}`, 'div'), {
  children: [valued('tag', 'string', 'String', tag),
             valued('name', 'string', 'String', tag),
             valued('colour', 'string', 'String', colour)],
})

const item = (n, title, labels, commentary = '', done = 0) => Object.assign(base(`Item__${n}`, 'div'), {
  children: [
    valued('title', 'string', 'String', title),
    valued('labels', 'tags', 'String', labels),
    valued('commentary', 'string', 'String', commentary),
    valued('done', 'int', 'Integer', done),
  ],
})

const ITEMS = [
  item(1, 'Parser drops a brace', 'bug, ui', 'only on nested lists', 60),
  item(2, 'Write the guide', 'docs', '', 0),
  item(3, 'Tighten the rows', 'ui', '', 100),
  item(4, 'Brace matching in the editor', 'bug', '', 0),
]

const docOf = (items) => ([Object.assign(base('project', 'tab'), {
  parameters: { mutable: { Boolean: true } },
  children: [
    Object.assign(base('Labels', 'list'), {
      parameters: { key: { String: 'tag' } },
      children: [label('bug', '#d73a4a'), label('ui', '#1d76db'), label('docs', '#7057ff')],
    }),
    Object.assign(base('filter', 'filter'), {
      parameters: {
        target: { String: '/project/Items' },
        text: { String: 'title, commentary' },
        tags: { String: 'labels' },
        status: { String: 'done' },
        vocabulary: { String: '/project/Labels' },
      },
    }),
    Object.assign(base('Items', 'list'), { children: items }),
  ],
})])

/** The titles still on screen. */
const visible = () => Array.from(document.querySelectorAll('[data-path]'))
  .filter((element) => {
    let path
    try { path = JSON.parse(element.dataset.path) } catch { return false }
    if (path.length !== 3 || path[1] !== 'Items') return false
    return !element.classList.contains('filtered-out')
  })
  .map((element) => {
    const path = JSON.parse(element.dataset.path)
    const title = document.querySelector(`[data-path='${JSON.stringify([...path, 'title'])}']`)
    return (title?.textContent || '').trim()
  })

const type = (what) => {
  const box = document.querySelector('.filter-text')
  box.value = what
  box.dispatchEvent(new Event('input', { bubbles: true }))
}

const clickTag = (tag) => {
  Array.from(document.querySelectorAll('.filter-tag'))
    .find((chip) => chip.dataset.tag === tag)
    .dispatchEvent(new Event('click', { bubbles: true }))
}

describe('a filter over a list', () => {
  let app

  beforeEach(() => {
    setupDOM()
    app = new OverseerApp()
    app.currentDocument = docOf(ITEMS)
    app._currentText = 'TEXT'
    app.renderer._filters = new Map()
    app.renderer.renderDocument(app.currentDocument)
  })

  it('shows everything until something is typed', () => {
    expect(visible().length).toBe(4)
    expect(document.querySelector('.filter-count').textContent).toBe('4')
  })

  it('matches a substring of the title, whatever the case', () => {
    type('brace')
    expect(visible()).toEqual(['Parser drops a brace', 'Brace matching in the editor'])
    expect(document.querySelector('.filter-count').textContent).toBe('2 of 4')
  })

  it('matches the other fields it was told about', () => {
    type('nested')
    expect(visible()).toEqual(['Parser drops a brace'])
  })

  it('narrows by a tag', () => {
    clickTag('ui')
    expect(visible()).toEqual(['Parser drops a brace', 'Tighten the rows'])
  })

  it('two tags means both, not either', () => {
    clickTag('ui')
    clickTag('bug')
    expect(visible()).toEqual(['Parser drops a brace'])
  })

  it('combines the text and the tags', () => {
    clickTag('bug')
    type('editor')
    expect(visible()).toEqual(['Brace matching in the editor'])
  })

  it('lets a tag go again', () => {
    clickTag('docs')
    expect(visible()).toEqual(['Write the guide'])
    clickTag('docs')
    expect(visible().length).toBe(4)
  })

  it('writes nothing to the document', () => {
    const before = JSON.stringify(app.currentDocument)
    type('brace')
    clickTag('bug')
    expect(JSON.stringify(app.currentDocument), 'the filter changed the document').toBe(before)
  })

  it('survives the list being repainted under it', () => {
    // The hazard it shares with the tab bug: a repaint replaces the entries with fresh elements
    // that know nothing about any filter over them.
    type('brace')
    expect(visible().length).toBe(2)

    app.renderer.rerenderSubtree(app.currentDocument, ['project', 'Items'])

    expect(visible(), 'the filtered list refilled when it was repainted')
      .toEqual(['Parser drops a brace', 'Brace matching in the editor'])
  })

  it('survives a full re-render', () => {
    type('guide')
    app.renderer.renderDocument(app.currentDocument)
    expect(visible()).toEqual(['Write the guide'])
  })

  it('says so rather than rendering an inert box when no list is named', () => {
    const doc = docOf(ITEMS)
    doc[0].children[1].parameters.target = { String: '' }
    app.renderer._filters = new Map()
    app.renderer.renderDocument(doc)
    expect(document.querySelector('.filter-broken')).toBeTruthy()
  })
})

// -- narrowing by status -----------------------------------------------------------------------
//
// Not a stored field: "in progress" is what a percentage between the two ends means, so the
// three boxes are worked out from the same number the row shows.

describe('the status boxes', () => {
  let app

  const tick = (label) => {
    const box = Array.from(document.querySelectorAll('.filter-status-option'))
      .find(o => o.textContent.trim() === label)
      .querySelector('input')
    box.checked = !box.checked
    box.dispatchEvent(new Event('change', { bubbles: true }))
  }

  beforeEach(() => {
    setupDOM()
    app = new OverseerApp()
    app.currentDocument = docOf(ITEMS)
    app._currentText = 'TEXT'
    app.renderer._filters = new Map()
    app.renderer.renderDocument(app.currentDocument)
  })

  it('offers three of them', () => {
    expect(Array.from(document.querySelectorAll('.filter-status-option')).map(o => o.textContent.trim()))
      .toEqual(['not started', 'in progress', 'finished'])
  })

  it('shows everything until one is ticked', () => {
    expect(visible().length).toBe(4)
  })

  it('finds what has not been begun', () => {
    tick('not started')
    expect(visible()).toEqual(['Write the guide', 'Brace matching in the editor'])
  })

  it('finds what is part way through', () => {
    tick('in progress')
    expect(visible()).toEqual(['Parser drops a brace'])
  })

  it('finds what is finished', () => {
    tick('finished')
    expect(visible()).toEqual(['Tighten the rows'])
  })

  it('two boxes means either, unlike two tags', () => {
    // Tags narrow - a thing can hold both. A task has one status, so ticking two must widen or
    // the pair would always be empty.
    tick('not started')
    tick('finished')
    expect(visible()).toEqual(['Write the guide', 'Tighten the rows', 'Brace matching in the editor'])
  })

  it('combines with the text and the tags', () => {
    tick('not started')
    clickTag('bug')
    expect(visible()).toEqual(['Brace matching in the editor'])
  })

  it('counts what is showing', () => {
    tick('finished')
    expect(document.querySelector('.filter-count').textContent).toBe('1 of 4')
  })

  it('lets a box go again', () => {
    tick('finished')
    expect(visible().length).toBe(1)
    tick('finished')
    expect(visible().length).toBe(4)
  })

  it('survives a repaint, like the rest of the filter', () => {
    tick('in progress')
    app.renderer.rerenderSubtree(app.currentDocument, ['project', 'Items'])
    expect(visible()).toEqual(['Parser drops a brace'])
  })
})

// -- and actually hidden, not merely marked ----------------------------------------------------
//
// Every test above checks that the class is applied, and every one of them passed while the
// browser showed all ten rows and the count read "4 of 10". The layout rules set
// `display: flex !important` on `.overseer-div.layout-horizontal` - specificity (0,2,0) -
// against `.filtered-out` at (0,1,0), so `flex` won and nothing in the DOM said so.
//
// The obvious guard is to load the stylesheet and read back `getComputedStyle(row).display`.
// That does not work: jsdom resolves this cascade the wrong way and answers `none` for the
// broken stylesheet as readily as for the fixed one, so such a test passes either way - which
// is the same hole in a new shape.
//
// So the invariant is checked where it lives, in the stylesheet: whatever hides a filtered row
// has to outrank every other rule that sets `display` with `!important`.

describe('the rule that hides a filtered row', () => {
  // Comments stripped first: this file explains the very rule being checked, and the
  // explanation contains selectors that would otherwise be read as rules.
  const styles = readFileSync('src/styles.css', 'utf8')
    .replace(/\/\*[\s\S]*?\*\//g, '')

  /** Every selector that sets `display` with `!important`, and how many classes it carries. */
  const importantDisplayRules = () => {
    const found = []
    const pattern = /([^{}]+)\{([^}]*)\}/g
    let match
    while ((match = pattern.exec(styles)) !== null) {
      const [, selectors, body] = match
      if (!/display\s*:[^;]*!important/.test(body)) continue
      for (const selector of selectors.split(',')) {
        const trimmed = selector.trim()
        if (!trimmed || trimmed.startsWith('@')) continue
        found.push({
          selector: trimmed,
          // Classes and attribute selectors, which is all this stylesheet uses to compete.
          weight: (trimmed.match(/\.[a-zA-Z_-]/g) || []).length
                + (trimmed.match(/\[/g) || []).length,
        })
      }
    }
    return found
  }

  it('exists at all', () => {
    const hiding = importantDisplayRules().filter(r => r.selector.includes('filtered-out'))
    expect(hiding.length, 'nothing hides a filtered row').toBeGreaterThan(0)
  })

  it('outranks every other rule that forces a display', () => {
    const rules = importantDisplayRules()
    const hiding = rules.filter(r => r.selector.includes('filtered-out'))
    const others = rules.filter(r => !r.selector.includes('filtered-out'))
    const mine = Math.min(...hiding.map(r => r.weight))
    const worst = others.reduce((held, r) => (r.weight > held.weight ? r : held), { weight: 0, selector: '(none)' })

    expect(
      mine,
      `\`${hiding[0].selector}\` carries ${mine} classes against ` +
      `\`${worst.selector}\` with ${worst.weight}; the more specific one wins even though ` +
      `both are !important, so a filtered row stays on screen`
    ).toBeGreaterThan(worst.weight)
  })
})
