import { describe, it, expect, beforeEach, vi } from 'vitest'

// A div that declares `on click` is pressed wherever it is touched - so ticking a shopping item
// off on a phone is a tap anywhere on the row, not on a small button at the end of it.
//
// Anywhere nothing inside answers for itself, that is. A control that can be changed keeps its
// tap, and nothing reaches the entry: a double tap on an amount has to edit it, and the first tap
// of the two must not have bought the thing on the way. Anything that cannot be changed from here
// lets the tap through - a worked-out name, a field declared fixed, a checkbox that only shows
// something, a group or a button with nothing to do - to the nearest thing that has an action.

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

const base = (name, node_type, parameters = {}, children = []) => ({
  name, node_type, parameters, children, is_hierarchy_transparent: false,
  source_id: '', source_fingerprint: 1, param_order: [], authored_dash: false,
})

const onClick = () => base('click', 'on', {}, [base('toggle', 'toggle', { path: { String: '../x' } })])

/// A shopping row, as the resolver hands it over, with one of everything a row can hold.
const theDocument = () => [base('s', 'tab', { mutable: { Boolean: true } }, [
  base('List', 'list', {}, [
    base('Item', 'div', { layout: { String: 'horizontal' } }, [
      base('name', 'string', { value: { Formula: 'Types.first()/name' }, _computed_value: { String: 'milk' } }),
      base('amount', 'float', { value: { Float: 2 } }),
      base('note', 'string', { value: { String: 'the blue one' }, mutable: { Boolean: false } }),
      base('shown', 'bool', { value: { Boolean: true }, mutable: { Boolean: false } }),
      base('ok', 'bool', { value: { Boolean: false } }),
      base('bought', 'button', { label: { String: 'bought' } }, [onClick()]),
      base('idle', 'button', { label: { String: 'idle' } }),
      base('group', 'div', {}, [
        base('said', 'string', { value: { String: 'fixed' }, mutable: { Boolean: false } }),
      ]),
      base('inner', 'div', {}, [
        base('also', 'string', { value: { Formula: '1' }, _computed_value: { String: '1' } }),
        onClick(),
      ]),
      onClick(),
    ]),
    base('Plain', 'div', {}, [
      base('title', 'string', { value: { Formula: '1' }, _computed_value: { String: '1' } }),
    ]),
  ]),
])]

let app
let pressed

const render = () => {
  app = new OverseerApp()
  app.currentDocument = theDocument()
  app._currentText = 'TEXT'
  app.renderer._filters = new Map()
  pressed = []
  // A press that runs something. The real one makes no trip for a node without the handler, so
  // a button with nothing to do calling it is not a press of anything.
  vi.spyOn(app.renderer, 'emitEvent').mockImplementation(async (node, _el, eventName) => {
    if (eventName === 'click' && app.renderer.declaresEvent(node, 'click')) pressed.push(node.name)
  })
  app.renderer.renderDocument(app.currentDocument)
}

const at = (...names) => document.querySelector(`[data-path='${JSON.stringify(['s', 'List', ...names])}']`)

const tap = (element) => {
  const event = new window.MouseEvent('click', { bubbles: true, cancelable: true })
  element.dispatchEvent(event)
  return event
}

describe('a whole entry as the thing you press', () => {
  beforeEach(() => {
    setupDOM()
    render()
  })

  it('is drawn as something to press', () => {
    const entry = at('Item')
    expect(entry.classList.contains('overseer-pressable')).toBe(true)
    expect(entry.getAttribute('role')).toBe('button')
    expect(entry.tabIndex, 'it cannot be reached from the keyboard').toBe(0)
  })

  it('is pressed by a tap on the entry itself', () => {
    tap(at('Item'))
    expect(pressed).toEqual(['Item'])
  })

  it('is pressed by a tap on a worked-out value', () => {
    tap(at('Item', 'name').querySelector('.field-value'))
    expect(pressed).toEqual(['Item'])
  })

  it('is pressed by a tap on a field declared fixed', () => {
    tap(at('Item', 'note').querySelector('.field-value'))
    expect(pressed).toEqual(['Item'])
  })

  it('is pressed by a tap on a group with no action of its own', () => {
    tap(at('Item', 'group'))
    tap(at('Item', 'group', 'said').querySelector('.field-value'))
    expect(pressed).toEqual(['Item', 'Item'])
  })

  it('is not pressed by a tap on a field that can be changed', () => {
    tap(at('Item', 'amount').querySelector('.field-value'))
    expect(pressed, 'the first tap of a double tap would have pressed it').toEqual([])
  })

  it('leaves a field that can be changed its double tap', () => {
    const value = at('Item', 'amount').querySelector('.field-value')
    value.dispatchEvent(new window.MouseEvent('dblclick', { bubbles: true }))
    expect(at('Item', 'amount').querySelector('.field-editor'), 'the amount could not be edited').not.toBeNull()
  })

  it('does not open the formula of a worked-out value on a double tap', () => {
    // Its taps are the entry's, so the second of two cannot be the value's.
    const value = at('Item', 'name').querySelector('.field-value')
    value.dispatchEvent(new window.MouseEvent('dblclick', { bubbles: true }))
    expect(at('Item', 'name').querySelector('.field-editor')).toBeNull()
  })

  it('passes a tap through a checkbox that only shows something, which stays as it was', () => {
    const box = at('Item', 'shown').querySelector('input[type=checkbox]')
    box.click()
    expect(pressed).toEqual(['Item'])
    expect(box.checked, 'the indicator flipped under the finger').toBe(true)
  })

  it('leaves a checkbox that can be changed its tap', () => {
    const box = at('Item', 'ok').querySelector('input[type=checkbox]')
    box.click()
    expect(pressed).toEqual([])
    expect(box.checked, 'it did not tick').toBe(true)
  })

  it('leaves a button inside the entry its own press', () => {
    tap(at('Item', 'bought'))
    expect(pressed).toEqual(['bought'])
  })

  it('passes a tap through a button that has nothing to do', () => {
    tap(at('Item', 'idle'))
    expect(pressed).toEqual(['Item'])
  })

  it('gives a tap on an entry inside another to the inner one alone', () => {
    tap(at('Item', 'inner', 'also').querySelector('.field-value'))
    expect(pressed).toEqual(['inner'])
  })

  it('is pressed from the keyboard', () => {
    const entry = at('Item')
    entry.dispatchEvent(new window.KeyboardEvent('keydown', { key: 'Enter', bubbles: true }))
    entry.dispatchEvent(new window.KeyboardEvent('keydown', { key: ' ', bubbles: true }))
    expect(pressed).toEqual(['Item', 'Item'])
  })

  it('leaves a div with no action as it was', () => {
    const plain = at('Plain')
    expect(plain.classList.contains('overseer-pressable')).toBe(false)
    expect(plain.hasAttribute('role')).toBe(false)
    tap(plain)
    expect(pressed).toEqual([])
  })
})
