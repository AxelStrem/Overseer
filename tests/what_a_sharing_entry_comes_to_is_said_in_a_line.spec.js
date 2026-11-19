import { describe, it, expect, vi } from 'vitest'

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }))

import { describeReach } from '../src/calls.js'

// `sharing_reach`: what `GET /account/sharing` answers, said as one markdown line an entry for the
// guarded field a sharing document shows it in.

describe('what a sharing entry comes to', () => {
  it('names the documents each entry covers, within the sharer\'s folder, and who it is with', () => {
    const said = describeReach({
      shares: [
        { document: 'projects/', with: 'team', can: 'edit', documents: ['ap31/projects/a.os', 'ap31/projects/b.os'], people: ['bob', 'erin'] },
        { document: 'gone.os', with: 'zoe', can: 'read', documents: [], people: ['zoe'] },
      ],
    })
    expect(said).toBe('- **projects/** - edit - projects/a.os, projects/b.os, with bob, erin\n- **gone.os** - read - no document, with zoe')
  })

  it('says nothing is shared when there are no entries', () => {
    expect(describeReach({ shares: [] })).toBe('Nothing is shared.')
  })

  it('keeps markdown in a name from being read as markup', () => {
    const said = describeReach({ shares: [{ document: '*x*.os', with: 'a_b', can: 'read', documents: ['ap31/*x*.os'], people: ['a_b'] }] })
    expect(said).toBe('- **\\*x\\*.os** - read - \\*x\\*.os, with a\\_b')
  })
})
