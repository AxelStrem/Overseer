import { describe, it, expect, vi } from 'vitest'

// What happens - an `on` block, an action - is never drawn, and is known by its type. A field may
// be named like an event or an action, `change` or `move`, and is drawn like any other: hiding by
// name once left an `int change` blank in the page although its value had been worked out.

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }))
import { OverseerRenderer } from '../src/renderer.js'

const node = (name, node_type, children = []) => ({
  name, node_type, parameters: {}, children, is_hierarchy_transparent: false,
})

const parent = node('week', 'div')

describe('a field named like an event or an action', () => {
  const renderer = new OverseerRenderer()

  it('is drawn', () => {
    for (const [name, type] of [['change', 'int'], ['set', 'int'], ['click', 'string'], ['move', 'bool']]) {
      expect(renderer.shouldRenderChild(parent, node(name, type))).toBe(true)
    }
  })

  it('while an event block and an action are not', () => {
    expect(renderer.shouldRenderChild(parent, node('change', 'on'))).toBe(false)
    expect(renderer.shouldRenderChild(parent, node('load', 'on'))).toBe(false)
    expect(renderer.shouldRenderChild(parent, node('', 'set'))).toBe(false)
  })

  it('draws no button of its own on a link', () => {
    const container = document.createElement('div')
    renderer.renderEventControls(node('day', 'link', [node('click', 'int')]), container)
    expect(container.querySelectorAll('button').length).toBe(0)

    renderer.renderEventControls(node('day', 'link', [node('click', 'on')]), container)
    expect(container.querySelectorAll('button').length).toBe(1)
  })
})
