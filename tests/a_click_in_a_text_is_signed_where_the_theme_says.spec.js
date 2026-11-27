import { describe, it, expect, vi } from 'vitest'

// Where the theme's `markdown/sign` is `me` and someone is signed in, a click on a task's box, a
// selector's option or a vote's writes who clicked at the end of its line, `<@handle>`; and the
// person chips such a line ends with are drawn as their icons alone.

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
import { withChoiceMade, withVoteAdded, withVoteToggled, withTaskTicked } from '../src/renderer.js'

const base = (name, node_type, parameters = {}, children = []) => ({
  name, node_type, parameters, children, is_hierarchy_transparent: false,
  source_id: '', source_fingerprint: 1, param_order: [], authored_dash: false,
})
const valued = (name, type, value) => base(name, type, { value: { String: value } })
const person = (tag, name) => base(`Person__${tag}`, 'div', {}, [valued('tag', 'string', tag), valued('name', 'string', name)])
const people = () => base('People', 'list', { key: { String: 'tag' }, hidden: { Boolean: true } },
  [person('ap31', 'Alice'), person('bob', 'Bob')])
const vocabulary = () => base('vocabulary', 'context', { value: { String: '@/t/People' }, _context_copy: { Boolean: true } })

let written
const draw = (body, { sign = true, viewer = 'ap31' } = {}) => {
  setupDOM()
  const app = new OverseerApp()
  window.app = app
  written = []
  app.markDocumentModified = () => {}
  app.reevaluateDocumentSelective = (paths, changes) => { written.push(...changes) }
  app.renderer.viewer = viewer
  const text = base('body', 'text', { value: { String: body }, markdown: { Boolean: true } })
  app.currentDocument = [base('t', 'tab', { mutable: { Boolean: true } }, [vocabulary(), people(), text])]
  app.currentFile = 'notes/t.os'
  if (sign) document.getElementById('content-display').style.setProperty('--ov-markdown-sign', 'me')
  app.renderer.renderDocument(app.currentDocument)
  return app
}
const settle = () => new Promise((r) => setTimeout(r, 20))
const click = (el) => el.dispatchEvent(new window.MouseEvent('click', { bubbles: true, cancelable: true }))
const controls = (kind) => Array.from(document.querySelectorAll(`.text-content .ov-${kind}`))

describe('a click in a text is signed where the theme says', () => {
  it('signs a ticked task once, and takes every signature off an unticked one', () => {
    expect(withTaskTicked('- [ ] milk\n- [ ] bread  ', 1, true, 'ap31')).toBe('- [ ] milk\n- [x] bread <@ap31>')
    expect(withTaskTicked('- [ ] milk <@ap31>', 0, true, 'ap31')).toBe('- [x] milk <@ap31>')
    expect(withTaskTicked('- [ ] milk <@bob>', 0, true, 'ap31')).toBe('- [x] milk <@bob> <@ap31>')
    expect(withTaskTicked('- [x] milk <@bob> <@ap31>', 0, false, 'ap31')).toBe('- [ ] milk')
    expect(withTaskTicked('> 1. [ ] tea', 0, true, 'ap31')).toBe('> 1. [x] tea <@ap31>')
  })

  it('moves the chooser\'s chip with the choice, leaving none on the other options of the list', () => {
    const text = '- (x) A <@bob>\n- ( ) B\n\nOther:\n\n- (x) C <@bob>'
    expect(withChoiceMade(text, 1, 'ap31')).toBe('- ( ) A\n- (x) B <@ap31>\n\nOther:\n\n- (x) C <@bob>')
    expect(withChoiceMade(text, 0, 'ap31')).toBe('- (x) A <@ap31>\n- ( ) B\n\nOther:\n\n- (x) C <@bob>')
  })

  it('gives a vote with a chip and takes it back with the next click by the same person', () => {
    expect(withVoteToggled('- (+) A', 0, 'ap31')).toBe('- (+1) A <@ap31>')
    expect(withVoteToggled('- (+1) A <@bob>', 0, 'ap31')).toBe('- (+2) A <@bob> <@ap31>')
    expect(withVoteToggled('- (+2) A <@ap31> <@bob>', 0, 'ap31')).toBe('- (+1) A <@bob>')
    expect(withVoteToggled('- (+1) A <@ap31>', 0, 'ap31')).toBe('- (+) A')
    expect(withVoteToggled('- (+) A <@ap31>', 0, 'ap31')).toBe('- (+) A')
  })

  it('without a signer, clicks write what they wrote before', () => {
    expect(withTaskTicked('- [x] milk <@bob>', 0, false)).toBe('- [ ] milk <@bob>')
    expect(withChoiceMade('- (x) A <@bob>\n- ( ) B', 1)).toBe('- ( ) A <@bob>\n- (x) B')
    expect(withVoteAdded('- (+1) A <@bob>', 0)).toBe('- (+2) A <@bob>')
    expect(withVoteToggled('- (+1) A <@bob>', 0, null)).toBe('- (+2) A <@bob>')
  })

  it('a click where the theme signs and someone is signed in writes their chip', async () => {
    draw('- [ ] milk\n\n- ( ) A\n- (x) B <@bob>\n\n- (+1) C <@bob>')
    click(controls('task')[0])
    await settle()
    click(controls('choice')[0])
    await settle()
    click(controls('vote')[0])
    await settle()
    click(controls('vote')[0])
    await settle()
    expect(written.map((w) => w.newValue)).toEqual([
      '- [x] milk <@ap31>\n\n- ( ) A\n- (x) B <@bob>\n\n- (+1) C <@bob>',
      '- [x] milk <@ap31>\n\n- (x) A <@ap31>\n- ( ) B\n\n- (+1) C <@bob>',
      '- [x] milk <@ap31>\n\n- (x) A <@ap31>\n- ( ) B\n\n- (+2) C <@bob> <@ap31>',
      '- [x] milk <@ap31>\n\n- (x) A <@ap31>\n- ( ) B\n\n- (+1) C <@bob>',
    ])
  })

  it('a click without the theme word, or with nobody signed in, is not signed', async () => {
    draw('- [ ] milk', { sign: false })
    click(controls('task')[0])
    await settle()
    expect(written.map((w) => w.newValue)).toEqual(['- [x] milk'])

    draw('- (+) A', { viewer: '' })
    click(controls('vote')[0])
    await settle()
    expect(written.map((w) => w.newValue)).toEqual(['- (+1) A'])
  })

  it('draws the people a line ends with as their icons alone, and a chip in its words in full', () => {
    draw('- [x] ask <@bob> first <@ap31>\n- (x) A <@bob> <@ap31>\n- (+2) C <@nobody>\n\nSee <@bob>', { sign: false })
    const iconOnly = (chip) => chip.classList.contains('chip-icon-only')
    const chipsIn = (selector) => Array.from(document.querySelectorAll(`${selector} .inline-chip`))
      .map((c) => [c.dataset.tag, iconOnly(c)])
    expect(chipsIn('.ov-task-item')).toEqual([['bob', false], ['ap31', true]])
    expect(chipsIn('.ov-choice-item')).toEqual([['bob', true], ['ap31', true]])
    // Nobody the people list knows keeps its word, as a field's chip does.
    expect(chipsIn('.ov-vote-item')).toEqual([['nobody', false]])
    expect(Array.from(document.querySelectorAll('.text-content p .inline-chip')).map(iconOnly)).toEqual([false])
    const signed = document.querySelector('.ov-task-item .chip-icon-only')
    expect(signed.textContent).toBe('A')
    expect(signed.dataset.fullChip).toBe('ap31')
  })
})
