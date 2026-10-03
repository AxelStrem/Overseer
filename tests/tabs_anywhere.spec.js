import { describe, it, expect, beforeEach, vi } from 'vitest'

// Tabs anywhere in a document, not only at the top.
//
// The tabs one parent holds are one tab system - a bar of their labels and a page each, one
// showing at a time - standing where the first of them stands. Grouped by parent, not by standing
// next to each other: two systems at one level are two divs. The parent's layout places the bar,
// above the pages or beside them. Which tab shows is the viewer's, kept for the session.

function setupDOM() {
  document.body.innerHTML = `
    <div id="app">
      <div id="toolbar"><button id="open-file-btn"></button><button id="new-file-btn"></button></div>
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

const node = (name, node_type, parameters = {}, children = []) => ({
  name, node_type, parameters, children, is_hierarchy_transparent: node_type === 'tab',
  source_id: '', source_fingerprint: 1, param_order: [], authored_dash: false,
})
const text = (name, value) => node(name, 'string', { value: { String: value } })
const tab = (name, label, children = [], extra = {}) => node(name, 'tab', { label: { String: label }, ...extra }, children)
const div = (name, layout, children) => node(name, 'div', { layout: { String: layout }, _effective_layout: { String: layout } }, children)

const theDocument = () => [tab('doc', 'Doc', [
  text('intro', 'before the tabs'),
  div('plan', 'vertical', [
    text('note', 'a note'),
    tab('open', 'Open', [text('first', 'first thing')], { 'hover-text': { String: 'What is still to do' } }),
    tab('done', 'Finished', [
      text('third', 'third thing'),
      div('detail', 'vertical', [
        tab('week', 'This week', [text('count', '3')]),
        tab('month', 'This month', [text('count', '12')]),
      ]),
    ]),
  ]),
  div('side', 'horizontal', [
    tab('one', 'One', [text('what', 'page one')]),
    tab('two', 'Two', [text('what', 'page two')]),
  ]),
  div('both', 'vertical', [
    div('left', 'vertical', [tab('a', 'A'), tab('b', 'B')]),
    div('right', 'vertical', [tab('c', 'C'), tab('d', 'D')]),
  ]),
])]

let app
const draw = () => app.renderer.renderDocument(app.currentDocument)
const at = (...path) => document.querySelector(`[data-path='${JSON.stringify(path)}']`)
const systemIn = (...path) => at(...path).querySelector(':scope > .overseer-tabs')
const headers = (system) => Array.from(system.querySelector(':scope > .tab-bar').children)
const showing = (system) => Array.from(system.querySelector(':scope > .tab-pages').children)
  .filter((p) => p.style.display !== 'none').map((p) => p.dataset.tabId)
const press = (label) => Array.from(document.querySelectorAll('.tab-button'))
  .find((b) => b.textContent === label).dispatchEvent(new Event('click', { bubbles: true }))

describe('tabs anywhere', () => {
  beforeEach(() => {
    setupDOM()
    try { sessionStorage.clear() } catch (_) { /* nothing kept */ }
    app = new OverseerApp()
    app.currentFile = 'tabs.os'
    app.currentDocument = theDocument()
    draw()
  })

  it('makes one system of the tabs a parent holds, where the first of them stands', () => {
    const plan = at('doc', 'plan')
    const children = Array.from(plan.children).map((c) => c.classList.contains('overseer-tabs') ? 'tabs' : c.dataset.path)
    expect(children).toEqual([JSON.stringify(['doc', 'plan', 'note']), 'tabs'])
    expect(headers(systemIn('doc', 'plan')).map((h) => h.textContent)).toEqual(['Open', 'Finished'])
    expect(showing(systemIn('doc', 'plan'))).toEqual(['open'])
  })

  it('keeps the tabs of two parents apart, and a tab inside a tab is a system of its own', () => {
    expect(headers(systemIn('doc', 'both', 'left')).map((h) => h.textContent)).toEqual(['A', 'B'])
    expect(headers(systemIn('doc', 'both', 'right')).map((h) => h.textContent)).toEqual(['C', 'D'])
    expect(headers(systemIn('doc', 'plan', 'done', 'detail')).map((h) => h.textContent)).toEqual(['This week', 'This month'])
    expect(document.querySelectorAll('.overseer-tabs').length).toBe(5)
  })

  it('puts the bar beside the pages when the parent lays its children out in a row', () => {
    expect(systemIn('doc', 'side').classList.contains('tabs-beside')).toBe(true)
    expect(systemIn('doc', 'plan').classList.contains('tabs-beside')).toBe(false)
  })

  it('shows the tab pressed, and only in its own system', () => {
    press('Finished')
    press('D')
    expect(showing(systemIn('doc', 'plan'))).toEqual(['done'])
    expect(showing(systemIn('doc', 'both', 'right'))).toEqual(['d'])
    expect(showing(systemIn('doc', 'both', 'left')), 'pressing in one system moved another').toEqual(['a'])
    expect(headers(systemIn('doc', 'plan')).map((h) => h.classList.contains('active'))).toEqual([false, true])
  })

  it('remembers what was chosen through a full redraw, and the document still shows', () => {
    press('Finished')
    press('This month')
    draw()
    draw()
    expect(showing(systemIn('doc', 'plan'))).toEqual(['done'])
    expect(showing(systemIn('doc', 'plan', 'done', 'detail'))).toEqual(['month'])
    const top = Array.from(document.querySelectorAll('#content-display > .tab-content'))
    expect(top.map((p) => p.style.display), 'the document drew blank after being drawn twice').toEqual([''])
  })

  it('says on hover what the tab says, and not on the fields inside it', () => {
    const open = headers(systemIn('doc', 'plan'))[0]
    expect(open.dataset.hoverText).toBe('What is still to do')
    expect(headers(systemIn('doc', 'plan'))[1].dataset.hoverText).toBeUndefined()
  })

  it('draws one tab again in place, showing or not as it was', () => {
    press('Finished')
    // A press inside a tab changes the tab's own parameters, so it is drawn again on its own.
    const done = app.currentDocument[0].children[1].children[2]
    done.parameters = { ...done.parameters, label: { String: 'Done' }, _explicit_overrides: { String: 'x' } }
    app.renderer.rerenderSubtree(app.currentDocument, ['doc', 'plan', 'done'])
    const system = systemIn('doc', 'plan')
    expect(headers(system).map((h) => h.textContent), 'a header was added or lost').toEqual(['Open', 'Done'])
    expect(showing(system)).toEqual(['done'])
    expect(document.querySelectorAll('.overseer-tabs').length, 'a second system grew').toBe(5)
    expect(systemIn('doc', 'plan', 'done', 'detail'), 'what the tab holds was lost').not.toBeNull()
  })

  it('draws a list whose entries are made from a tab as one system, an entry a page', () => {
    const day = (name, date, sort) => node(name, 'Day', {
      _original_type: { String: 'tab' }, label: { String: date }, _ui_sort_key: { Integer: sort },
    }, [text('date', date)])
    app.currentDocument = [tab('doc', 'Doc', [
      node('Days', 'list', { key: { String: 'date' } }, [day('Day__1', '2026-10-01', 2), day('Day__2', '2026-10-02', 1)]),
    ])]
    draw()
    const system = systemIn('doc', 'Days')
    expect(system, 'the entries were drawn as rows').not.toBeNull()
    expect(headers(system).map((h) => h.textContent), 'not in the order the list sorts them').toEqual(['2026-10-02', '2026-10-01'])
    expect(showing(system)).toEqual(['Day__2'])
  })
})
