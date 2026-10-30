import { describe, it, expect, vi, beforeEach } from 'vitest'

// A link that picks a day by a date - `History[key=$(../selected_date)]` - is followed by the page.
// An unnamed div is no step of any address, so the date it reads and the date each day is known by
// may both sit in an unnamed row. Read as names only, neither was found: the key came out empty,
// and an empty key matched the first day whose date it could not read, whatever the bar said.

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }))
import { OverseerRenderer } from '../src/renderer.js'

const field = (name, node_type, value) => ({
  name, node_type, parameters: { value: { String: value } }, children: [], is_hierarchy_transparent: false,
})
const row = (...children) => ({ name: 'div', node_type: 'div', parameters: {}, children, is_hierarchy_transparent: true })
const named = (name, node_type, children, parameters = {}) => ({
  name, node_type, parameters, children, is_hierarchy_transparent: false,
})

const day = (n, date) => named(`Day__${n}`, 'Day', [row(field('date', 'timestamp', date), named('add', 'button', []))])

const LINK = '/t/History[key=$(../selected_date)]'

function documentWith(selected, days) {
  return [named('t', 'tab', [
    row(named('Day', 'div', [row(field('date', 'timestamp', ''), named('add', 'button', []))])),
    named('Selected', 'div', [
      row(field('selected_date', 'timestamp', selected)),
      named('SelectedDay', 'div', [], { link: { String: LINK } }),
    ]),
    named('History', 'list', days, {
      entry: { Template: 'Day' }, key: { String: 'date' }, keyPrecision: { String: 'day' },
    }),
  ])]
}

const dateOf = (node) => {
  const inRow = node.children.find((c) => c.is_hierarchy_transparent)
  const date = (inRow ? inRow.children : node.children).find((c) => c.name === 'date')
  return date.parameters.value.String
}

describe('a link key', () => {
  const renderer = new OverseerRenderer()
  const follow = (doc) => {
    window.app = { currentDocument: doc }
    return renderer.resolveLinkTarget({ String: LINK }, ['t', 'Selected', 'SelectedDay'])
  }

  beforeEach(() => { delete window.app })

  it('is read through an unnamed row beside the link, and finds the day it names in its row', () => {
    const doc = documentWith('2026-10-28', [day(1, '2026-10-27'), day(2, '2026-10-28'), day(3, '2026-10-29')])
    const { targetNode, targetPath } = follow(doc)
    expect(targetNode && dateOf(targetNode)).toBe('2026-10-28')
    expect(targetPath).toEqual(['t', 'History', 'Day__2'])
  })

  it('that no day holds gives a preview dated as the key, its date where the template keeps it', () => {
    const doc = documentWith('2026-10-30', [day(1, '2026-10-27'), day(2, '2026-10-28')])
    const { targetNode, phantomPreviewNode, phantomMeta } = follow(doc)
    expect(targetNode).toBeNull()
    expect(phantomMeta.keyValue).toBe('2026-10-30')
    expect(dateOf(phantomPreviewNode)).toBe('2026-10-30')
  })

  it('that is empty picks no day', () => {
    const doc = documentWith('', [named('Day__1', 'Day', [named('intake', 'list', [])]), day(2, '2026-10-28')])
    const { targetNode } = follow(doc)
    expect(targetNode).toBeNull()
    expect(renderer.entryKeyed(doc[0].children[2], 'date', '')).toBeNull()
  })
})
