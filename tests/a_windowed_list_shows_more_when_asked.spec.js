import { describe, it, expect, beforeEach, vi } from 'vitest'

// A list showing part of itself offers the reader one window more, under the note saying how many
// it left out. Asking writes nothing: the server keeps it against the reader's session and answers
// with the whole document worked out with the wider window, which the page draws again.

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
import { invoke } from '@tauri-apps/api/core'
import { OverseerApp } from '../src/main.js'

const base = (name, node_type) => ({
  name, node_type, parameters: {}, children: [], is_hierarchy_transparent: false,
  source_id: '', source_fingerprint: 1, param_order: [], authored_dash: false,
})

const inView = (n) => Object.assign(base(`Day__${n}`, 'div'), {
  parameters: { layout: { String: 'horizontal' }, _effective_layout: { String: 'horizontal' } },
  children: [
    Object.assign(base('date', 'string'), {
      parameters: { value: { String: `2026-09-${String(n).padStart(2, '0')}` }, label: { String: '' } },
    }),
  ],
})

const outOfView = (n) => Object.assign(base(`Day__${n}`, '-'), {
  parameters: { _out_of_view: { Boolean: true } },
  children: [
    Object.assign(base('date', 'string'), {
      parameters: { value: { String: `2026-08-${String(n).padStart(2, '0')}` } },
    }),
  ],
})

const docOf = (children, left) => ([Object.assign(base('t', 'tab'), {
  children: [
    Object.assign(base('Days', 'list'), {
      parameters: {
        _effective_layout: { String: 'vertical' },
        window: { Integer: 3 },
        ...(left ? { _left_out_of_view: { Integer: left } } : {}),
      },
      children,
    }),
  ],
})])

const render = (doc) => {
  const app = new OverseerApp()
  app.currentFile = 'p.os'
  app.currentDocument = doc
  app._currentText = 'TEXT'
  app.renderer.renderDocument(app.currentDocument)
  return app
}

const ask = () => document.querySelector('.overseer-entries-left-out .overseer-show-more')
const dates = () => Array.from(document.querySelectorAll('.field-value'))
  .map(e => e.textContent)
  .filter(v => typeof v === 'string' && v.startsWith('20'))

describe('a windowed list shows more when asked', () => {
  beforeEach(() => {
    setupDOM()
    invoke.mockReset()
  })

  it('offers one window more, or what is left when that is less', () => {
    render(docOf([inView(1), inView(2), inView(3), outOfView(4), outOfView(5), outOfView(6), outOfView(7)], 4))
    expect(ask().textContent).toBe('show 3 more')
    render(docOf([inView(1), inView(2), inView(3), outOfView(4)], 1))
    expect(ask().textContent).toBe('show 1 more')
  })

  it('asks the server for this list by its path, and draws what comes back', async () => {
    const app = render(docOf([inView(1), inView(2), inView(3), outOfView(4)], 1))
    invoke.mockImplementation(async (cmd) => cmd === 'show_more_of_overseer_list'
      ? { text: 'TEXT', version: 'v', changes: null, nodes: docOf([inView(1), inView(2), inView(3), inView(4)]) }
      : null)
    await app.showMoreOf(['t', 'Days'])
    const asked = invoke.mock.calls.filter(([cmd]) => cmd === 'show_more_of_overseer_list')
    expect(asked).toEqual([['show_more_of_overseer_list', { path: 'p.os', listPath: ['t', 'Days'] }]])
    expect(dates()).toEqual(['2026-09-01', '2026-09-02', '2026-09-03', '2026-09-04'])
    expect(ask()).toBeNull()
  })

  it('draws no answer over what the page took meanwhile, and catches up instead', async () => {
    const app = render(docOf([inView(1), inView(2), inView(3), outOfView(4)], 1))
    let answer
    invoke.mockImplementation((cmd) => {
      if (cmd === 'show_more_of_overseer_list') return new Promise((resolve) => { answer = resolve })
      if (cmd === 'catch_up_overseer_document') return Promise.resolve(null)
      return Promise.resolve(null)
    })
    const asking = app.showMoreOf(['t', 'Days'], '2026-08-04')
    // An edit's answer lands while the list is being read back.
    app._currentText = 'TEXT AFTER AN EDIT'
    answer({ text: 'TEXT', version: 'v', changes: null, nodes: docOf([inView(1), inView(2), inView(3), inView(4)]) })
    expect(await asking).toBe(false)
    expect(dates()).toEqual(['2026-09-01', '2026-09-02', '2026-09-03'])
    const caught = invoke.mock.calls.filter(([cmd]) => cmd === 'catch_up_overseer_document')
    expect(caught.length).toBe(1)
    expect(caught[0][1].path).toBe('p.os')
  })

  it('reads back once for a chip pressed twice, and says so on the chip meanwhile', async () => {
    const app = render(docOf([inView(1), inView(2), inView(3)]))
    const within = document.createElement('div')
    document.body.appendChild(within)
    const chip = document.createElement('span')
    chip.className = 'tag-chip chip-reveals'
    chip.dataset.reach = JSON.stringify({ list: ['t', 'History'], key: 'w42' })
    within.appendChild(chip)
    app.renderer.watchForChipPresses(within)
    vi.spyOn(app.renderer, 'canReadBackTo').mockReturnValue(true)
    let done
    const readBackTo = vi.spyOn(app.renderer, 'readBackTo').mockImplementation(() => new Promise((resolve) => { done = resolve }))
    chip.click()
    chip.click()
    expect(readBackTo).toHaveBeenCalledTimes(1)
    expect(chip.classList.contains('chip-reading-back')).toBe(true)
    expect(document.getElementById('status-message').textContent).toBe('w42: reading back to it')
    done(null)
    await Promise.resolve()
    await Promise.resolve()
    expect(chip.classList.contains('chip-reading-back')).toBe(false)
  })

  it('presses through to the list it is under', () => {
    const app = render(docOf([inView(1), inView(2), inView(3), outOfView(4)], 1))
    const showMoreOf = vi.spyOn(app, 'showMoreOf').mockResolvedValue(undefined)
    ask().click()
    expect(showMoreOf).toHaveBeenCalledWith(['t', 'Days'])
  })
})
