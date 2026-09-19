import { describe, it, expect, beforeEach, vi } from 'vitest'

// Chips for tags the field never held itself.
//
// A meal stores a food handle and an amount; what the food *is* - vegan, dairy, alcohol - is
// looked up, the same way its name and its macros are. So the value arrives already worked out,
// and the vocabulary that turns "vegan" into a green chip saying so lives in another document
// entirely, reached through a mount.
//
// The parameters below carry the shape that crosses the wire - `{ String: ... }`, not a bare
// string. Writing bare values here is what let an earlier test pass while the app drew the wrong
// thing: the fixture agreed with the code instead of with the resolver.

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

const str = (name, value) => Object.assign(base(name, 'string'), {
  parameters: { value: { String: value } },
})

const label = (tag, name, colour) => Object.assign(base(`Label__${tag}`, 'div'), {
  children: [str('tag', tag), str('name', name), str('colour', colour)],
})

/** The vocabulary as a preloaded mount delivers it: a mount node holding the list. */
const vocabulary = () => Object.assign(base('VOCAB', 'mount'), {
  children: [
    Object.assign(base('Labels', 'list'), {
      children: [
        label('vegan', 'vegan', '#0e8a16'),
        label('dairy', 'dairy', '#f9a825'),
      ],
    }),
  ],
})

/** A meal whose tags were worked out from the catalogue rather than typed into it. */
const meal = (tags) => Object.assign(base('MealRecord__1', 'div'), {
  parameters: { layout: { String: 'horizontal' }, _effective_layout: { String: 'horizontal' } },
  children: [
    Object.assign(base('labels', 'tags'), {
      parameters: {
        label: { String: '' },
        vocabulary: { String: 'tracker_v2/VOCAB/Labels' },
        mutable: { Boolean: false },
        value: { Formula: 'FOODS/Catalog.filter(|x| x/handle == ../../food)/labels' },
        ...(tags === null ? {} : { _computed_value: { String: tags } }),
      },
    }),
  ],
})

const render = (tags) => {
  const app = new OverseerApp()
  app.currentDocument = [Object.assign(base('tracker_v2', 'tab'), {
    children: [vocabulary(), meal(tags)],
  })]
  app._currentText = 'TEXT'
  app.renderer._filters = new Map()
  app.renderer.renderDocument(app.currentDocument)
  return app
}

const chips = () => Array.from(document.querySelectorAll('.tag-chip'))

describe('tags drawn from a catalogue', () => {
  beforeEach(setupDOM)

  it('draws a chip for each tag the food carries', () => {
    render('vegan, dairy')
    expect(chips().map(c => c.dataset.tag)).toEqual(['vegan', 'dairy'])
  })

  it('colours them from a vocabulary reached through a mount', () => {
    // The vocabulary lives in foods.os and arrives here as a mount. Nothing else would notice it
    // failing to resolve: the chips would simply come out grey and unlisted.
    render('vegan')
    const chip = chips()[0]
    expect(chip.style.backgroundColor).not.toBe('')
    expect(chip.classList.contains('tag-unknown')).toBe(false)
    expect(chip.textContent).toContain('vegan')
  })

  it('offers no way to edit them', () => {
    // They belong to the food, not to this meal. An editable chip would write onto the record,
    // where the next resolve overwrites it from the catalogue - a change that appears to work.
    render('vegan, dairy')
    expect(document.querySelectorAll('.tag-remove').length).toBe(0)
  })

  it('draws nothing for a food nobody has classified', () => {
    // Seventeen of them, deliberately. No chips reads as "nobody has decided", which is true.
    render('')
    expect(chips()).toEqual([])
  })

  it('marks a tag the vocabulary does not list rather than hiding it', () => {
    // The food really does carry it, and dropping it from the display would misreport the field.
    render('vegan, fermented')
    const unknown = chips().find(c => c.dataset.tag === 'fermented')
    expect(unknown).toBeDefined()
    expect(unknown.classList.contains('tag-unknown')).toBe(true)
  })
})
