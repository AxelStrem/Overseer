import { describe, it, expect, beforeEach, vi } from 'vitest'

// An enum holds one of its values, and most - a stage - always should. One that says
// `clearable=true` - whom an item is for - also offers a first choice that empties it again.

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

const valued = (name, type, variant, value) => Object.assign(base(name, type), {
  parameters: { value: { [variant]: value } },
})

const person = (tag) => Object.assign(base(`Person__${tag}`, 'div'), {
  children: [valued('tag', 'string', 'String', tag), valued('name', 'string', 'String', tag)],
})

const docWith = (held, parameters = {}) => ([
  Object.assign(base('project', 'tab'), {
    parameters: { mutable: { Boolean: true } },
    children: [
      Object.assign(base('People', 'list'), {
        parameters: { key: { String: 'tag' }, hidden: { Boolean: true } },
        children: [person('ann'), person('bot')],
      }),
      Object.assign(base('assignee', 'enum'), {
        parameters: { value: { String: held }, vocabulary: { String: '/project/People' }, ...parameters },
      }),
    ],
  }),
])

const chip = () => document.querySelector('.enum-field .tag-chip:not(.tag-option)')
const offered = () => Array.from(document.querySelectorAll('.tag-picker .tag-option'))
const names = () => offered().map(o => o.textContent.trim())
const press = (el) => el.dispatchEvent(new Event('click', { bubbles: true }))

describe('a clearable enum', () => {
  let app

  const render = (doc) => {
    app = new OverseerApp()
    app.currentDocument = doc
    app._currentText = 'TEXT'
    app.renderer.renderDocument(app.currentDocument)
  }

  beforeEach(() => setupDOM())

  it('offers an empty choice first, read as its placeholder', () => {
    render(docWith('ann', { clearable: { Boolean: true }, placeholder: { String: 'nobody' } }))
    press(chip())
    expect(names()).toEqual(['nobody', 'bot'])
    expect(offered()[0].classList.contains('tag-placeholder')).toBe(true)
  })

  it('reads its empty choice as a dash when the placeholder says nothing', () => {
    render(docWith('ann', { clearable: { Boolean: true }, placeholder: { String: '' } }))
    press(chip())
    expect(names()).toEqual(['—', 'bot'])
  })

  it('writes an empty value when the empty choice is picked', async () => {
    render(docWith('ann', { clearable: { Boolean: true } }))
    const writes = []
    app.reevaluateDocumentSelective = vi.fn(async (_paths, changes) => { writes.push(changes) })
    press(chip())
    press(offered()[0])
    await vi.waitFor(() => expect(writes).toHaveLength(1))

    expect(writes[0]).toEqual([{ path: 'project/assignee', oldValue: 'ann', newValue: '' }])
    expect(app.currentDocument[0].children[1].parameters.value).toEqual({ String: '' })
    expect(chip().classList.contains('tag-placeholder')).toBe(true)
  })

  it('offers no empty choice while it is empty already', () => {
    render(docWith('', { clearable: { Boolean: true } }))
    press(chip())
    expect(names()).toEqual(['ann', 'bot'])
  })

  it('can still be emptied when it holds the only value there is', () => {
    const doc = docWith('ann', { clearable: { Boolean: true } })
    doc[0].children[0].children = [person('ann')]
    render(doc)
    press(chip())
    expect(names()).toEqual(['—'])
  })

  it('draws its empty choice as the first segment, pressed while it holds nothing', () => {
    render(docWith('', { clearable: { Boolean: true }, view: { String: 'segments' } }))
    const segments = Array.from(document.querySelectorAll('.enum-segment'))
    expect(segments.map(s => s.textContent.trim())).toEqual(['—', 'ann', 'bot'])
    expect(segments[0].getAttribute('aria-pressed')).toBe('true')
  })
})

describe('an enum that is not clearable', () => {
  beforeEach(() => setupDOM())

  it('offers only its other values', () => {
    const app = new OverseerApp()
    app.currentDocument = docWith('ann')
    app._currentText = 'TEXT'
    app.renderer.renderDocument(app.currentDocument)
    press(chip())
    expect(names()).toEqual(['bot'])
    expect(document.querySelectorAll('.enum-segment-empty')).toHaveLength(0)
  })
})
