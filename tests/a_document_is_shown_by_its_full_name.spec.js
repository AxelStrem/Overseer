import { describe, it, expect, beforeEach, vi } from 'vitest'

// The address bar names a document in full - `/doc/ap31/tasks.os` for ap31's own `tasks.os`, even
// opened at `/doc/tasks.os` - as the server names it, so the link its owner copies is the one whoever
// it is shared with can open. The page never works a username out itself.

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }))
import { invoke } from '@tauri-apps/api/core'
import { Account, docAddress, signInFromHere } from '../src/account.js'
import { OPERATIONS } from '../src/calls.js'

const AP31 = { username: 'ap31', name: 'Dmitrii', admin: true }

const opened = async (path, answer) => {
  window.history.replaceState(null, '', path)
  invoke.mockImplementation(async (cmd) => (cmd === 'access' ? answer : null))
  const account = new Account({})
  await account.start()
  return account
}

describe('a document is shown by its full name', () => {
  beforeEach(() => {
    document.body.innerHTML = '<a id="drawer-sign-in" hidden>Sign in</a><div id="document-list" hidden></div>'
    window.__OVERSEER_HOST__ = 'browser'
    invoke.mockReset()
    globalThis.fetch = vi.fn(async () => ({ ok: false, status: 404, json: async () => null }))
  })

  it('replaces a short address with the full one, keeping the node after #', async () => {
    await opened('/doc/tasks.os#project/Items/[scan]', { asker: 'person', person: AP31, can: 'edit', document: 'ap31/tasks.os' })
    expect(window.location.pathname).toBe('/doc/ap31/tasks.os')
    expect(decodeURIComponent(window.location.hash)).toBe('#project/Items/[scan]')
  })

  it('encodes each part of the name it shows', async () => {
    await opened('/doc/%D0%B7%D0%B0%D0%BC%D0%B5%D1%82%D0%BA%D0%B8.os', { asker: 'person', person: AP31, can: 'edit', document: 'ap31/заметки.os' })
    expect(decodeURIComponent(window.location.pathname)).toBe('/doc/ap31/заметки.os')
    expect(docAddress('ap31/a b.os')).toBe('/doc/ap31/a%20b.os')
  })

  it('leaves the address alone where the server names no document', async () => {
    await opened('/doc/journal.os', { asker: 'anyone', can: 'edit', document: 'journal.os' })
    expect(window.location.pathname).toBe('/doc/journal.os')
    await opened('/doc/tasks.os', { asker: 'person', person: AP31, can: null })
    expect(window.location.pathname).toBe('/doc/tasks.os')
    await opened('/examples/games/minesweeper.os', { asker: 'stranger', can: 'read', document: 'ap31/games/minesweeper.os' })
    expect(window.location.pathname).toBe('/examples/games/minesweeper.os')
  })

  it('sends a stranger signing in back to the node the link pointed at', async () => {
    await opened('/doc/ap31/recipes.os#recipes/soup', { asker: 'stranger', can: 'read', document: 'ap31/recipes.os' })
    expect(signInFromHere()).toBe('/signin?next=' + encodeURIComponent('/doc/ap31/recipes.os#recipes/soup'))
    expect(document.getElementById('drawer-sign-in').getAttribute('href')).toBe(signInFromHere())
  })

  it('takes a short name from the drawer for the open document it names', async () => {
    window.history.replaceState(null, '', '/doc/ap31/tasks.os')
    const asked = []
    invoke.mockImplementation(async (cmd, args, options) => {
      asked.push([cmd, options && options.headers && decodeURIComponent(options.headers['Overseer-Document'])])
      return { asker: 'person', can: 'edit', document: 'ap31/tasks.os' }
    })
    const app = { currentFile: 'ap31/tasks.os', account: { document: 'ap31/tasks.os' } }
    await OPERATIONS.open({ path: 'tasks.os' }, app)
    expect(asked).toEqual([['access', 'tasks.os']])
    expect(window.location.pathname).toBe('/doc/ap31/tasks.os')
  })
})
