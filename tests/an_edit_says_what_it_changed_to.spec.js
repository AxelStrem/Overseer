import { describe, it, expect, beforeEach, vi } from 'vitest'

// What gets written to the file is `_currentText` - the text the backend produced on the last
// resolve - not a fresh serialization of the node tree. So an edit only survives a save if the
// backend was told what it changed to: the new value travels as `fieldChanges`, the backend puts
// it into the text it returns, and that text is what is written.
//
// A field that asks for a re-evaluation with only a *path* gets back text resolved from content
// that never had the edit. The screen is right, because the local node was mutated. The file is
// not, and the edit is gone on the next load - which is exactly how it was found: tags assigned
// to a task came back empty after reloading the document.
//
// Every kind of field that can be edited is checked here, because the three that were wrong were
// wrong in the same way and nothing said so.

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

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }))
import { OverseerApp } from '../src/main.js'

const base = (name, node_type) => ({
  name, node_type, parameters: {}, children: [], is_hierarchy_transparent: false,
  source_id: '', source_fingerprint: 1, param_order: [], authored_dash: false,
})

const valued = (name, type, variant, value, extra = {}) => Object.assign(base(name, type), {
  parameters: Object.assign({ value: { [variant]: value }, mutable: { Boolean: true } }, extra),
})

const label = (tag) => Object.assign(base(`Label__${tag}`, 'div'), {
  children: [
    valued('tag', 'string', 'String', tag),
    valued('name', 'string', 'String', tag),
    valued('colour', 'string', 'String', '#888888'),
  ],
})

const docOf = (field) => ([Object.assign(base('t', 'tab'), {
  parameters: { mutable: { Boolean: true } },
  children: [
    Object.assign(base('Labels', 'list'), {
      parameters: { key: { String: 'tag' } },
      children: [label('chores'), label('practice')],
    }),
    Object.assign(base('Row', 'div'), { children: [field] }),
  ],
})])

/** Render one editable field and record every re-evaluation it asks for. */
function withField(field) {
  setupDOM()
  const app = new OverseerApp()
  app.currentDocument = docOf(field)
  app._currentText = 'TEXT'
  app.renderer._filters = new Map()

  const asked = []
  app.reevaluateDocumentSelective = (paths = [], changes = []) => {
    asked.push({ paths, changes })
    return Promise.resolve({ domOnly: false })
  }
  app.markDocumentModified = () => {}

  app.renderer.renderDocument(app.currentDocument)
  return { app, asked }
}

/** What the backend would be told the field became, if anything. */
const told = (asked) => asked
  .flatMap((call) => call.changes || [])
  .map((change) => String(change.newValue))

describe('an edit tells the backend what it changed to', () => {
  it('when a tag is added', () => {
    const field = valued('labels', 'tags', 'String', '', {
      vocabulary: { String: '/t/Labels' },
    })
    const { asked } = withField(field)

    document.querySelector('.tag-add').dispatchEvent(new Event('click', { bubbles: true }))
    const offered = document.querySelector('.tag-option, .tag-choice, .tag-menu button')
    expect(offered, 'no tag was offered to pick').toBeTruthy()
    offered.dispatchEvent(new Event('click', { bubbles: true }))

    expect(asked.length, 'adding a tag asked for nothing').toBeGreaterThan(0)
    expect(
      told(asked),
      'the tag was applied to the node but the backend was never told, so the text that gets ' +
      'saved will not have it'
    ).toContain('chores')
  })

  it('when a tag is removed', () => {
    const field = valued('labels', 'tags', 'String', 'chores, practice', {
      vocabulary: { String: '/t/Labels' },
    })
    const { asked } = withField(field)

    const remove = document.querySelector('.tag-remove, .tag-chip button')
    expect(remove, 'no way to remove a tag').toBeTruthy()
    remove.dispatchEvent(new Event('click', { bubbles: true }))

    expect(told(asked).length, 'removing a tag told the backend nothing').toBeGreaterThan(0)
  })

  it('when a boolean is toggled', () => {
    const { asked } = withField(valued('flag', 'bool', 'Boolean', false))

    const box = document.querySelector('input[type="checkbox"]')
    box.checked = true
    box.dispatchEvent(new Event('change', { bubbles: true }))

    expect(told(asked), 'a toggled boolean is not saved').toContain('true')
  })

  it('when a checkbox is ticked', () => {
    const { asked } = withField(valued('flag', 'checkbox', 'Boolean', false))

    const box = document.querySelector('input[type="checkbox"]')
    box.checked = true
    box.dispatchEvent(new Event('change', { bubbles: true }))

    expect(told(asked), 'a ticked checkbox is not saved').toContain('true')
  })
})
