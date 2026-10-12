import { describe, it, expect, vi } from 'vitest'

// `<icon=name>` inside any text a document gives the page draws that icon of the library in the
// text's place. The text is stored as written; only drawing changes.

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
import { textWithoutIcons } from '../src/renderer.js'

const base = (name, node_type, parameters = {}, children = []) => ({
  name, node_type, parameters, children, is_hierarchy_transparent: false,
  source_id: '', source_fingerprint: 1, param_order: [], authored_dash: false,
})

const draw = (children, tabParams = {}) => {
  setupDOM()
  const app = new OverseerApp()
  app.currentDocument = [base('t', 'tab', tabParams, children)]
  app.currentFile = 't.os'
  app.renderer._filters = new Map()
  app.renderer.renderDocument(app.currentDocument)
  return app
}

const at = (name) => document.querySelector(`[data-path='${JSON.stringify(['t', name])}']`)
const icons = (el) => Array.from(el.querySelectorAll('span.icon')).map((i) => i.dataset.icon)

describe('a text draws its icons', () => {
  it('in a label and a value, with the text around them', () => {
    draw([base('due', 'string', { label: { String: '<icon=clock> Due' }, value: { String: 'soon <icon=alarm-clock>!' } })])
    const label = at('due').querySelector('label')
    expect(icons(label)).toEqual(['clock'])
    expect(label.textContent).toBe(' Due')
    expect(label.dataset.text).toBe('<icon=clock> Due')
    const value = at('due').querySelector('.field-value')
    expect(icons(value)).toEqual(['alarm-clock'])
    expect(value.textContent).toBe('soon !')
  })

  it('leaves every other tag as the text it is', () => {
    draw([base('odd', 'string', { label: { String: '<b>bold</b> <icon=../x>' } })])
    const label = at('odd').querySelector('label')
    expect(label.querySelector('b')).toBeNull()
    expect(label.textContent).toBe('<b>bold</b> ')
    const slot = label.querySelector('span.icon')
    expect(slot.dataset.icon).toBeUndefined()
    expect(slot.style.getPropertyValue('--icon')).toBe('')
  })

  it('draws text without an icon exactly as before', () => {
    draw([base('plain', 'string', { label: { String: 'Plain' }, value: { String: 'a < b' } })])
    const label = at('plain').querySelector('label')
    expect(label.textContent).toBe('Plain')
    expect(label.dataset.text).toBeUndefined()
    expect(at('plain').querySelector('.field-value').textContent).toBe('a < b')
  })

  it('in a tab label', () => {
    const app = draw([])
    const el = document.createElement('div')
    app.renderer.labelTabHeader(el, base('t', 'tab', { label: { String: '<icon=list-todo> Tasks' } }))
    expect(icons(el)).toEqual(['list-todo'])
    expect(el.textContent).toBe(' Tasks')
  })

  it('in a chip, which says its tag on hover when it is only an icon', () => {
    const app = draw([])
    const chip = app.renderer.tagChip('veg', { name: '<icon=carrot>' }, null)
    expect(icons(chip)).toEqual(['carrot'])
    expect(chip.dataset.tag).toBe('veg')
    expect(chip.title).toBe('veg')
    const named = app.renderer.tagChip('veg', { name: '<icon=carrot> vegetable', hover: 'Vegetables' }, null)
    expect(named.textContent).toBe(' vegetable')
    expect(named.title).toBe('Vegetables')
  })

  it('in a button label, which alone makes an icon button named by its node', () => {
    draw([base('add', 'button', { label: { String: '<icon=plus>' } }),
          base('save', 'button', { label: { String: '<icon=save> Save' } })])
    const add = at('add').matches('button') ? at('add') : at('add').querySelector('button')
    expect(add.classList.contains('icon-button')).toBe(true)
    expect(icons(add)).toEqual(['plus'])
    expect(add.title).toBe('add')
    const save = at('save').matches('button') ? at('save') : at('save').querySelector('button')
    expect(save.classList.contains('icon-button')).toBe(false)
    expect(icons(save.querySelector('.overseer-button-label'))).toEqual(['save'])
  })

  it('in markdown, inside a colour too', () => {
    const app = draw([])
    const box = document.createElement('div')
    box.innerHTML = app.renderer.renderMarkdown('**late** <color=red | <icon=clock> again>')
    const red = box.querySelector('.md-inline-color')
    expect(red.style.color).toBe('red')
    expect(icons(red)).toEqual(['clock'])
    expect(red.querySelector('span.icon').style.getPropertyValue('--icon')).toBe('url("/icons/clock.svg")')
    expect(red.textContent.trim()).toBe('again')
    expect(box.querySelector('strong').textContent).toBe('late')
  })

  it('in markdown, an unsafe name draws an empty slot and no HTML gets through', () => {
    const app = draw([])
    const box = document.createElement('div')
    box.innerHTML = app.renderer.renderMarkdown('<icon="x" onerror="y"> <img src=x>')
    expect(box.querySelector('img')).toBeNull()
    expect(box.querySelector('[onerror]')).toBeNull()
    const slot = box.querySelector('span.icon')
    expect(slot.dataset.icon).toBeUndefined()
  })

  it('keeps the text as written for whatever reads it back', () => {
    const app = draw([base('due', 'string', { value: { String: '<icon=clock> soon' } })])
    const value = at('due').querySelector('.field-value')
    expect(app.renderer.textOf(value)).toBe('<icon=clock> soon')
    app.renderer.setTextWithIcons(value, 'later')
    expect(app.renderer.textOf(value)).toBe('later')
  })

  it('is taken out where only text can go', () => {
    expect(textWithoutIcons('<icon=clock>  Due  <icon=x>')).toBe('Due')
    expect(textWithoutIcons('plain  text')).toBe('plain  text')
  })
})
