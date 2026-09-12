import { describe, it, expect, beforeEach, vi } from 'vitest'

// A `tags` field holds a comma-separated set - already a real collection to the evaluator,
// which can filter and count it. It was rendered as a plain string, so six labels read as forty
// characters of prose. These pin the chips, where the colours come from, and that editing one
// writes the set back the way it was stored.

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

/** One entry of the tag list: what it is called and what colour it takes. */
const label = (tag, name, colour) => Object.assign(base(`Label__${tag}`, 'div'), {
  children: [
    valued('tag', 'string', 'String', tag),
    valued('name', 'string', 'String', name),
    valued('colour', 'string', 'String', colour),
  ],
})

const docWith = (held, { vocabulary = '/project/Labels', mutable = true } = {}) => ([
  Object.assign(base('project', 'tab'), {
    parameters: { mutable: { Boolean: mutable } },
    children: [
      Object.assign(base('Labels', 'list'), {
        parameters: { key: { String: 'tag' } },
        children: [label('bug', 'bug', '#d73a4a'),
                   label('ui', 'interface', '#0e8a16'),
                   label('later', 'later', '#fbca04')],
      }),
      Object.assign(valued('labels', 'tags', 'String', held), {
        parameters: { value: { String: held },
                      ...(vocabulary ? { vocabulary: { String: vocabulary } } : {}) },
      }),
    ],
  }),
])

const chips = () => Array.from(document.querySelectorAll('.tag-chip:not(.tag-option)'))
const shown = () => chips().map(c => c.textContent.replace(/×/g, '').trim())

describe('a tags field', () => {
  let app

  const render = (held, options) => {
    app = new OverseerApp()
    app.currentDocument = docWith(held, options)
    app._currentText = 'TEXT'
    app.renderer.renderDocument(app.currentDocument)
  }

  beforeEach(() => setupDOM())

  it('draws one chip per tag, not one line of prose', () => {
    render('bug, ui')
    expect(shown()).toEqual(['bug', 'interface'])
  })

  it('takes its colours from the document, not from the renderer', () => {
    render('bug, ui')
    expect(chips()[0].style.backgroundColor).toBe('rgb(215, 58, 74)')
    expect(chips()[1].style.backgroundColor).toBe('rgb(14, 138, 22)')
  })

  it('shows a tag the list does not mention rather than dropping it', () => {
    // It is in the document. Hiding it would misreport what the field holds - and it is usually
    // a typo, or a tag since removed from the list, which is worth seeing.
    render('bug, nonsense')
    expect(shown()).toEqual(['bug', 'nonsense'])
    expect(chips()[1].classList.contains('tag-unknown')).toBe(true)
  })

  it('is still usable with no vocabulary at all', () => {
    render('one, two', { vocabulary: null })
    expect(shown()).toEqual(['one', 'two'])
  })

  it('writes the set back as the line it came from when a tag is removed', () => {
    render('bug, ui, later')
    chips()[1].querySelector('.tag-remove').dispatchEvent(new Event('click', { bubbles: true }))

    const field = app.currentDocument[0].children[1]
    expect(field.parameters.value).toEqual({ String: 'bug, later' })
    expect(shown(), 'the page still shows the removed tag').toEqual(['bug', 'later'])
  })

  it('offers only the tags that are not on it yet', () => {
    render('bug')
    document.querySelector('.tag-add').dispatchEvent(new Event('click', { bubbles: true }))

    const offered = Array.from(document.querySelectorAll('.tag-picker .tag-option'))
      .map(o => o.textContent.trim())
    expect(offered).toEqual(['interface', 'later'])
  })

  it('adds the one that is picked', () => {
    render('bug')
    document.querySelector('.tag-add').dispatchEvent(new Event('click', { bubbles: true }))
    Array.from(document.querySelectorAll('.tag-picker .tag-option'))
      .find(o => o.textContent.trim() === 'later')
      .dispatchEvent(new Event('click', { bubbles: true }))

    expect(app.currentDocument[0].children[1].parameters.value).toEqual({ String: 'bug, later' })
    expect(shown()).toEqual(['bug', 'later'])
  })

  it('offers nothing to change when the document is not mutable', () => {
    render('bug, ui', { mutable: false })
    expect(shown()).toEqual(['bug', 'interface'])
    expect(document.querySelector('.tag-add'), 'it offered an add on a read-only field').toBeNull()
    expect(document.querySelector('.tag-remove'), 'it offered a remove').toBeNull()
  })
})
