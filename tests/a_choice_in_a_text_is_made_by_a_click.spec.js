import { describe, it, expect, vi } from 'vitest'

// A selector in a markdown text, `- ( ) A` / `- (x) B`, and a vote, `- (+2) A` / `- (+) B`, draw a
// control on each option, and a click on one rewrites the text's marks as an edit would.

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
import { withChoiceMade, withVoteAdded, withTaskTicked } from '../src/renderer.js'

const base = (name, node_type, parameters = {}, children = []) => ({
  name, node_type, parameters, children, is_hierarchy_transparent: false,
  source_id: '', source_fingerprint: 1, param_order: [], authored_dash: false,
})

let written
const draw = (body, { mutable = true, formula = false } = {}) => {
  setupDOM()
  const app = new OverseerApp()
  window.app = app
  written = []
  app.markDocumentModified = () => {}
  app.reevaluateDocumentSelective = (paths, changes) => { written.push(...changes) }
  const params = { value: formula ? { Formula: '"' + body + '"' } : { String: body } }
  if (formula) params._computed_value = { String: body }
  if (mutable) params.mutable = { Boolean: true }
  app.currentDocument = [base('t', 'tab', {}, [base('body', 'text', params)])]
  app.currentFile = 'notes/t.os'
  app.renderer.renderDocument(app.currentDocument)
  return app
}
const settle = () => new Promise((r) => setTimeout(r, 20))
const options = () => Array.from(document.querySelectorAll('.text-content .ov-choice'))
const votes = () => Array.from(document.querySelectorAll('.text-content .ov-vote'))
const click = (el) => el.dispatchEvent(new window.MouseEvent('click', { bubbles: true, cancelable: true }))

describe('a choice in a text is made by a click', () => {
  it('chooses one option of a list and clears only that list', () => {
    const text = 'Where:\n\n- ( ) pizza\n- (x) noodles\n  - (x) spicy\n  - ( ) mild\n- plain line\n\nWhen:\n\n* (X) noon\n* ( ) evening'
    expect(withChoiceMade(text, 0)).toBe(text.replace('- ( ) pizza', '- (x) pizza').replace('- (x) noodles', '- ( ) noodles'))
    expect(withChoiceMade(text, 3)).toBe(text.replace('  - (x) spicy', '  - ( ) spicy').replace('  - ( ) mild', '  - (x) mild'))
    expect(withChoiceMade(text, 5)).toBe(text.replace('* (X) noon', '* ( ) noon').replace('* ( ) evening', '* (x) evening'))
    expect(withChoiceMade(text, 1)).toBe(text)
    expect(withChoiceMade(text, 6)).toBeNull()
  })

  it('finds options in quotes, numbered and loose lists and any line ending, and none in code or tasks', () => {
    expect(withChoiceMade('> 1. ( ) a\n> 2. (x) b', 0)).toBe('> 1. (x) a\n> 2. ( ) b')
    expect(withChoiceMade('- ( ) a\n\n  more of a\n\n- (x) b', 0)).toBe('- (x) a\n\n  more of a\n\n- ( ) b')
    expect(withChoiceMade('intro\r\n\r\n- ( ) a\r\n- (x) b\r\n', 0)).toBe('intro\r\n\r\n- (x) a\r\n- ( ) b\r\n')
    expect(withChoiceMade('```\n- ( ) code\n```\n\n- [ ] (x) task\n- ( ) real', 0)).toBe('```\n- ( ) code\n```\n\n- [ ] (x) task\n- (x) real')
    expect(withChoiceMade('- (y) not one\n- (x)no space', 0)).toBeNull()
  })

  it('adds one vote to one option, a bare one counting none', () => {
    const text = '- (+2) hiking\n- (+) cinema\n- (+9) sleep\n\n- (x) not a vote'
    expect(withVoteAdded(text, 0)).toBe(text.replace('(+2)', '(+3)'))
    expect(withVoteAdded(text, 1)).toBe(text.replace('(+) cinema', '(+1) cinema'))
    expect(withVoteAdded(text, 2)).toBe(text.replace('(+9)', '(+10)'))
    expect(withVoteAdded(text, 3)).toBeNull()
  })

  it('leaves tasks counted as they were', () => {
    expect(withTaskTicked('- ( ) a\n- [ ] b\n- (+) c\n- [ ] d', 1, true)).toBe('- ( ) a\n- [ ] b\n- (+) c\n- [x] d')
  })

  it('draws selectors and votes with their marks as controls, numbered in order', () => {
    draw('- ( ) pizza\n- (x) noodles\n  - ( ) spicy\n\nNext:\n\n- (+2) hiking\n- (+) cinema\n- plain')
    expect(options().map((o) => [o.checked, o.dataset.choiceIndex, o.disabled])).toEqual([[false, '0', false], [true, '1', false], [false, '2', false]])
    expect(document.querySelector('li.ov-choice-item.chosen > .ov-choice-text').textContent).toBe('noodles')
    expect(votes().map((v) => [v.querySelector('.ov-vote-count').textContent, v.dataset.voteIndex])).toEqual([['2', '0'], ['0', '1']])
    expect(document.querySelector('li.ov-vote-item .ov-vote-text').textContent).toBe('hiking')
    expect(document.querySelector('.text-content').textContent).not.toContain('(x)')
    expect(document.querySelector('.text-content').textContent).toContain('plain')
  })

  it('a click on an option writes the choice, and one on a vote writes one more', async () => {
    const app = draw('- ( ) pizza\n- (x) noodles\n\n- (+) hiking')
    click(options()[0])
    await settle()
    expect(written).toEqual([{ path: 't/body', oldValue: '- ( ) pizza\n- (x) noodles\n\n- (+) hiking', newValue: '- (x) pizza\n- ( ) noodles\n\n- (+) hiking' }])
    expect(options().map((o) => o.checked)).toEqual([true, false])

    click(votes()[0].querySelector('.ov-vote-count'))
    await settle()
    click(votes()[0])
    await settle()
    expect(written.map((w) => w.newValue).slice(1)).toEqual(['- (x) pizza\n- ( ) noodles\n\n- (+1) hiking', '- (x) pizza\n- ( ) noodles\n\n- (+2) hiking'])
    expect(app.currentDocument[0].children[0].parameters.value).toEqual({ String: '- (x) pizza\n- ( ) noodles\n\n- (+2) hiking' })
    expect(document.querySelector('textarea')).toBeNull()

    click(options()[0])
    await settle()
    expect(written.length).toBe(3)
  })

  it('a double click on a vote does not open the editor', () => {
    draw('- (+) hiking')
    votes()[0].dispatchEvent(new window.MouseEvent('dblclick', { bubbles: true, cancelable: true }))
    expect(document.querySelector('textarea')).toBeNull()
  })

  it('keeps the controls shut and writes nothing in a text that cannot be changed or is worked out', async () => {
    draw('- ( ) a\n- (+) b', { mutable: false })
    expect([options()[0].disabled, votes()[0].disabled]).toEqual([true, true])
    click(options()[0])
    draw('- ( ) a\n- (+) b', { formula: true })
    expect([options()[0].disabled, votes()[0].disabled]).toEqual([true, true])
    click(votes()[0])
    await settle()
    expect(written).toEqual([])
  })
})
