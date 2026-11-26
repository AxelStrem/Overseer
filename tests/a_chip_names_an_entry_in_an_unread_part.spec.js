import { describe, it, expect, beforeEach, vi } from 'vitest'

// A list with parts not read holds only its newest entries, and the server puts what a chip is
// drawn from for the rest on the list. A `<#key>` naming one of those draws its name, colour and
// title as any other would. The entry is not in the page, so a press on its chip asks the server
// to read the list back as far as that key, then shows the entry the answer holds.

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

const valued = (name, type, value, more = {}) => Object.assign(base(name, type), {
  parameters: { value: { String: value }, ...more },
})

const finished = (handle, title) => Object.assign(base(`[${handle}]`, 'div'), {
  children: [valued('handle', 'string', handle), valued('title', 'string', title)],
})

const UNREAD = [
  { key: 'old', title: 'An old one', colour: '#1d4ed8' },
  { key: 'new', title: 'Not what the page holds' },
  { key: 'bare' },
]

const doc = (note, read = []) => ([
  Object.assign(base('project', 'tab'), {
    parameters: { mutable: { Boolean: true } },
    children: [
      Object.assign(base('vocabulary', 'context'), {
        parameters: { value: { String: '/project/History' }, _context_copy: { Boolean: true } },
      }),
      valued('note', 'text', note, { markdown: { Boolean: true } }),
      Object.assign(base('History', 'list'), {
        parameters: {
          key: { String: 'handle' },
          ...(read.length ? {} : {
            _parts_left_unread: { Integer: 1 },
            _parts_vocabulary: { String: JSON.stringify(UNREAD) },
          }),
        },
        children: [...read, finished('new', 'Drawn this week')],
      }),
    ],
  }),
])

const noteChips = () => Array.from(document.querySelectorAll('.text-field .tag-chip'))

describe('a chip naming an entry in a part not read', () => {
  const render = (note) => {
    const app = new OverseerApp()
    app.currentDocument = doc(note)
    app._currentText = 'TEXT'
    app.renderer.renderDocument(app.currentDocument)
    return app
  }

  beforeEach(() => setupDOM())

  it('draws the title and colour the server read from the part', () => {
    render('after <#old>')
    const [chip] = noteChips()
    expect(chip.textContent).toBe('old')
    expect(chip.title).toBe('An old one')
    expect(chip.style.backgroundColor).toBe('rgb(29, 78, 216)')
    expect(chip.classList.contains('tag-unknown')).toBe(false)
  })

  it('reads the list back as far as the entry when pressed, and shows it', async () => {
    const app = render('after <#old>')
    const chip = noteChips()[0]
    expect(chip.classList.contains('chip-reveals')).toBe(true)
    // The server's answer: the list with the old part read.
    app.showMoreOf = vi.fn(async () => {
      app.currentDocument = doc('after <#old>', [finished('old', 'An old one')])
      app.renderer.renderDocument(app.currentDocument)
      return true
    })
    const revealed = vi.spyOn(app.renderer, 'revealEntry')
    chip.click()
    await vi.waitFor(() => expect(revealed).toHaveBeenCalled())
    expect(app.showMoreOf).toHaveBeenCalledWith(['project', 'History'], 'old')
    expect(revealed).toHaveBeenCalledWith(['project', 'History', '[old]'])
    expect(revealed.mock.results[0].value).toBe(null)
  })

  it('says so when the answer still does not hold the entry', async () => {
    const app = render('after <#old>')
    app.showMoreOf = vi.fn(async () => true)
    app.setStatus = vi.fn()
    noteChips()[0].click()
    await vi.waitFor(() => expect(app.setStatus).toHaveBeenCalled())
    expect(app.setStatus.mock.calls[0][0]).toBe('old: not found in the list')
  })

  it('reveals an entry the page draws without asking for more', () => {
    const app = render('and <#new>')
    app.showMoreOf = vi.fn()
    const revealed = vi.spyOn(app.renderer, 'revealEntry')
    noteChips()[0].click()
    expect(app.showMoreOf).not.toHaveBeenCalled()
    expect(revealed).toHaveBeenCalledWith(['project', 'History', '[new]'])
  })

  it('is drawn from the entry the page holds where there is one', () => {
    render('and <#new>')
    const [chip] = noteChips()
    expect(chip.title).toBe('Drawn this week')
    expect(chip.classList.contains('chip-reveals')).toBe(true)
  })

  it('is its key alone for an entry that gives nothing more', () => {
    render('and <#bare>')
    const [chip] = noteChips()
    expect(chip.textContent).toBe('bare')
    expect(chip.classList.contains('tag-unknown')).toBe(false)
  })
})
