import { describe, it, expect, beforeEach, vi } from 'vitest'
import { readFileSync } from 'node:fs'

// A project's milestones sit beside its tags (pills) and its stage (a box), so they take a third
// shape: `slant`, a box leaning right. These pin that the shape is asked for and reaches the
// field's chips and the box that narrows by it, and that the stylesheet leans the chip and keeps
// what it holds upright.

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

const milestone = (tag, colour) => Object.assign(base(`Label__${tag}`, 'div'), {
  children: [
    valued('tag', 'string', 'String', tag),
    valued('name', 'string', 'String', tag),
    valued('colour', 'string', 'String', colour),
  ],
})

const field = (name, value, extra) => Object.assign(valued(name, 'tags', 'String', value), {
  parameters: { value: { String: value }, ...extra },
})

const item = (n, labels, milestones) => Object.assign(base(`Item__${n}`, 'div'), {
  children: [
    valued('title', 'string', 'String', `item ${n}`),
    field('labels', labels, { vocabulary: { String: '/project/Labels' } }),
    field('milestones', milestones, {
      vocabulary: { String: '/project/Milestones' },
      shape: { String: 'slant' },
    }),
  ],
})

const docOf = () => ([Object.assign(base('project', 'tab'), {
  parameters: { mutable: { Boolean: true } },
  children: [
    Object.assign(base('Labels', 'list'), {
      parameters: { key: { String: 'tag' }, hidden: { Boolean: true } },
      children: [milestone('bug', '#d73a4a')],
    }),
    Object.assign(base('Milestones', 'list'), {
      parameters: { key: { String: 'tag' }, hidden: { Boolean: true } },
      children: [milestone('alpha', '#0e8a16'), milestone('beta', '#1d76db')],
    }),
    Object.assign(base('labels', 'textbox'), {
      parameters: {
        value: { String: '' },
        narrows: { String: '/project/Items' },
        by: { String: 'labels' },
        vocabulary: { String: '/project/Labels' },
      },
    }),
    Object.assign(base('milestones', 'textbox'), {
      parameters: {
        value: { String: '' },
        narrows: { String: '/project/Items' },
        by: { String: 'milestones' },
        vocabulary: { String: '/project/Milestones' },
        shape: { String: 'slant' },
      },
    }),
    Object.assign(base('Items', 'list'), {
      children: [item(1, 'bug', 'alpha'), item(2, '', 'beta')],
    }),
  ],
})])

describe('a milestone chip', () => {
  beforeEach(() => {
    setupDOM()
    const app = new OverseerApp()
    app.currentDocument = docOf()
    app._currentText = 'TEXT'
    app.renderer.renderDocument(app.currentDocument)
  })

  it('leans when its field asks for it, beside tags that stay pills', () => {
    const rows = Array.from(document.querySelectorAll('.tags-field:not(.textbox-field) .tag-chips'))
    expect(rows.filter((r) => r.classList.contains('chip-slant'))).toHaveLength(2)
    expect(rows.filter((r) => r.classList.contains('chip-pill'))).toHaveLength(2)
    const leaning = rows.find((r) => r.classList.contains('chip-slant'))
    expect(leaning.querySelector('.tag-chip').dataset.tag).toBe('alpha')
  })

  it('leans in the box that narrows by it too, and in what that box offers', () => {
    const boxOf = (name) => document.querySelector(`[data-path='${JSON.stringify(['project', name])}']`)
    const row = boxOf('milestones').querySelector('.tag-chips')
    expect(row.classList.contains('chip-slant')).toBe(true)
    expect(boxOf('labels').querySelector('.tag-chips').classList.contains('chip-pill')).toBe(true)
    boxOf('milestones').querySelector('.tag-add').click()
    expect(Array.from(document.querySelectorAll('.tag-option')).map((c) => c.dataset.tag)).toEqual(['alpha', 'beta'])
  })
})

describe('the slant in the stylesheet', () => {
  const css = readFileSync('src/styles.css', 'utf8')

  it('skews the chip one way and what it holds back the other', () => {
    const chip = css.match(/\.chip-slant \.tag-chip,\s*\.chip-slant \.tag-add \{[^}]*skewX\((-?\d+)deg\)/)
    const held = css.match(/\.chip-slant \.tag-chip > \* \{[^}]*skewX\((-?\d+)deg\)/)
    expect(chip).not.toBeNull()
    expect(held).not.toBeNull()
    expect(Number(chip[1])).toBeLessThan(0)
    expect(Number(held[1])).toBe(-Number(chip[1]))
  })
})
