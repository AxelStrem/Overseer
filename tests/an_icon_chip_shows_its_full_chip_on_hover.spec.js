import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'

// `chip="icon"` draws a field's chips as their entries' icons alone, on their colours, so a column
// of people can be as narrow as a chip; the whole chip shows in the hover popup instead.

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

const person = (tag, name, colour, icon) => Object.assign(base(`Person__${tag}`, 'div'), {
  children: [
    valued('tag', 'string', 'String', tag),
    valued('name', 'string', 'String', name),
    valued('colour', 'string', 'String', colour),
    valued('icon', 'string', 'String', icon),
  ],
})

const field = (name, type, held, extra = {}) => Object.assign(base(name, type), {
  parameters: { value: { String: held }, vocabulary: { String: '/project/People' }, ...extra },
})

const docWith = (fields, { mutable = true } = {}) => ([
  Object.assign(base('project', 'tab'), {
    parameters: { mutable: { Boolean: mutable } },
    children: [
      Object.assign(base('People', 'list'), {
        parameters: { key: { String: 'tag' }, hidden: { Boolean: true } },
        children: [person('ap31', 'Dementii', '#ea9a10', 'lambda'),
                   person('bot', 'Claude', '#3673ec', '')],
      }),
      ...fields,
    ],
  }),
])

const iconChip = { chip: { String: 'icon' } }
const chipsOf = (name) => Array.from(document.querySelectorAll(
  `[data-path='${JSON.stringify(['project', name])}'] .tag-chip:not(.tag-option)`))
const popup = () => document.querySelector('.overseer-hover-text')
const hover = (el) => el.dispatchEvent(new MouseEvent('mouseover', { bubbles: true }))
const press = (el) => el.dispatchEvent(new Event('click', { bubbles: true }))

describe('a field whose chips are icons', () => {
  let app

  const render = (doc) => {
    app = new OverseerApp()
    app.currentDocument = doc
    app._currentText = 'TEXT'
    app.renderer.renderDocument(app.currentDocument)
  }

  beforeEach(() => setupDOM())
  afterEach(() => vi.useRealTimers())

  it('draws its entry\'s icon on its colour, without the name', () => {
    render(docWith([field('owner', 'enum', 'ap31', iconChip)]))
    const [chip] = chipsOf('owner')
    expect(chip.classList.contains('chip-icon-only')).toBe(true)
    expect(chip.querySelector('.chip-icon')).not.toBeNull()
    expect(chip.querySelector('.chip-name')).toBeNull()
    expect(chip.textContent).not.toContain('Dementii')
    expect(chip.style.backgroundColor).toBe('rgb(234, 154, 16)')
    expect(chip.title).toBe('')
  })

  it('draws the first letter of the name for an entry without an icon', () => {
    render(docWith([field('owner', 'enum', 'bot', iconChip)]))
    expect(chipsOf('owner')[0].querySelector('.chip-initial').textContent).toBe('C')
  })

  it('shows the whole chip on hover, above what the field says', () => {
    vi.useFakeTimers()
    render(docWith([field('owner', 'enum', 'ap31', { ...iconChip, 'hover-text': { String: 'Whose item it is' } })]))
    hover(chipsOf('owner')[0])
    vi.advanceTimersByTime(400)
    const shown = popup()
    expect(shown.style.display).toBe('block')
    const full = shown.querySelector('.tag-chip')
    expect(full.querySelector('.chip-name').textContent).toBe('Dementii')
    expect(full.querySelector('.chip-icon')).not.toBeNull()
    expect(shown.querySelector('.hover-text-under-chip').textContent).toBe('Whose item it is')
  })

  it('draws a tags field\'s chips the same way', () => {
    vi.useFakeTimers()
    render(docWith([field('people', 'tags', 'ap31, bot', iconChip)]))
    const chips = chipsOf('people')
    expect(chips.map(c => c.classList.contains('chip-icon-only'))).toEqual([true, true])
    hover(chips[1])
    vi.advanceTimersByTime(400)
    expect(popup().querySelector('.chip-name').textContent).toBe('Claude')
  })

  it('still offers full chips in its picker', () => {
    render(docWith([field('owner', 'enum', 'ap31', iconChip)]))
    press(chipsOf('owner')[0])
    expect(Array.from(document.querySelectorAll('.tag-picker .tag-option')).map(o => o.textContent.trim()))
      .toEqual(['Claude'])
  })

  it('keeps the full, marked chip of a value its list does not hold', () => {
    render(docWith([field('owner', 'enum', 'stranger', iconChip)]))
    const [chip] = chipsOf('owner')
    expect(chip.classList.contains('chip-icon-only')).toBe(false)
    expect(chip.textContent.trim()).toBe('stranger')
  })

  it('draws names as before without it', () => {
    render(docWith([field('owner', 'enum', 'ap31')]))
    expect(chipsOf('owner')[0].querySelector('.chip-name').textContent).toBe('Dementii')
  })
})
