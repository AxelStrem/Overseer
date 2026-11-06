import { describe, it, expect, beforeEach, vi } from 'vitest'

// Who is looking, on a server that keeps accounts. The server says who is asking and whether the
// open document is theirs to change; the page lists what they may open under whose it is, draws a
// document shared only to be read as read-only, and offers a stranger the way in. A person's
// profile, keys and the links an admin makes are the drawer document's.

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }))
import { invoke } from '@tauri-apps/api/core'
import { Account, groupsOf } from '../src/account.js'
import { OverseerRenderer } from '../src/renderer.js'

function setupDOM() {
  document.body.className = 'in-browser'
  document.body.innerHTML = `
    <a id="drawer-sign-in" hidden>Sign in</a>
    <div id="document-list" hidden></div>`
}

const AP31 = { username: 'ap31', name: 'Dmitrii', admin: true }
const BOB = { username: 'bob', name: 'Bob', admin: false }

const LISTING = {
  documents: ['ap31/tasks.os', 'ap31/food/foods.os', 'alice/shopping.os', 'bob/notes.os', 'bob/recipes.os', 'carol/tasks.os'],
  mine: ['ap31/tasks.os', 'ap31/food/foods.os'],
  shared: { bob: ['bob/notes.os', 'bob/recipes.os'], alice: ['alice/shopping.os'] },
  public: ['ap31/tasks.os', 'bob/recipes.os', 'carol/tasks.os'],
}

const answering = (answer) => invoke.mockImplementation(async (cmd) => (cmd === 'access' ? answer : null))

const fetched = []
const fetching = (answers) => {
  fetched.length = 0
  globalThis.fetch = vi.fn(async (url, options) => {
    fetched.push({ url, body: options && options.body ? JSON.parse(options.body) : null })
    const answer = answers[url]
    return { ok: !!answer, status: answer ? 200 : 403, json: async () => answer || { error: 'only an admin may invite' } }
  })
}

const started = async (answer, { path = '/doc/ap31/tasks.os' } = {}) => {
  window.history.replaceState(null, '', path)
  answering(answer)
  const app = { renderer: new OverseerRenderer() }
  const account = new Account(app)
  account.navigate = vi.fn()
  await account.start()
  return { app, account }
}

describe('a viewer is offered what they may do', () => {
  beforeEach(() => {
    setupDOM()
    window.__OVERSEER_HOST__ = 'browser'
    invoke.mockReset()
    fetching({ '/documents': LISTING })
  })

  it('lists their own without their folder, what is shared by owner, and the rest of what is public', () => {
    const groups = groupsOf(LISTING, 'ap31')
    const shown = groups.map((g) => [g.title, g.parts.map((p) => [p.title || '', p.documents.map((d) => d.shown)])])
    expect(shown).toEqual([
      ['Mine', [['', ['tasks.os', 'food/foods.os']]]],
      ['Shared with me', [['alice', ['alice/shopping.os']], ['bob', ['bob/notes.os', 'bob/recipes.os']]]],
      ['Public', [['', ['carol/tasks.os']]]],
    ])
    expect(groups[0].parts[0].documents[0].name).toBe('ap31/tasks.os')
  })

  it('lists everything as one on a server without accounts', () => {
    expect(groupsOf({ documents: ['tasks.os', 'foods.os'] }, null)).toEqual([
      { title: 'Documents', parts: [{ documents: [{ name: 'tasks.os', shown: 'tasks.os' }, { name: 'foods.os', shown: 'foods.os' }] }] },
    ])
  })

  it('draws the list with no document open', async () => {
    await started({ asker: 'person', person: AP31, can: null }, { path: '/' })
    await vi.waitFor(() => expect(document.getElementById('document-list').hidden).toBe(false))
    const links = Array.from(document.querySelectorAll('#document-list a')).map((a) => [a.textContent, a.getAttribute('href')])
    expect(links[0]).toEqual(['tasks.os', '/doc/ap31/tasks.os'])
    expect(links).toContainEqual(['alice/shopping.os', '/doc/alice/shopping.os'])
    expect(Array.from(document.querySelectorAll('#document-list h3')).map((h) => h.textContent))
      .toEqual(['Mine', 'Shared with me', 'Public'])
  })

  it('draws a document shared only to be read with nothing to change, whatever it says', async () => {
    const field = { name: 'apple', node_type: 'string', parameters: { mutable: { Boolean: true } }, children: [] }
    const guarded = { name: 'day', node_type: 'date', parameters: { mutable: { String: 'guarded' } }, children: [] }
    const { app } = await started({ asker: 'person', person: BOB, can: 'read' })
    expect(app.renderer.getEffectiveMutableMode(field)).toBe('false')
    expect(app.renderer.getEffectiveMutableMode(guarded)).toBe('guarded')
    expect(document.body.classList.contains('document-read-only')).toBe(true)

    const { app: owner } = await started({ asker: 'person', person: AP31, can: 'edit' })
    expect(owner.renderer.getEffectiveMutableMode(field)).toBe('true')
  })

  it('offers a stranger the way in, back to the document', async () => {
    await started({ asker: 'stranger', person: null, can: 'read' })
    expect(document.body.classList.contains('viewer-stranger')).toBe(true)
    expect(document.getElementById('drawer-sign-in').hidden).toBe(false)
    expect(document.getElementById('drawer-sign-in').getAttribute('href')).toBe('/signin?next=%2Fdoc%2Fap31%2Ftasks.os')
  })

  it('leaves the token, a person and the desktop app with no way in to offer', async () => {
    await started({ asker: 'anyone', person: null, can: 'edit' })
    expect(document.body.classList.contains('viewer-stranger')).toBe(false)
    expect(document.getElementById('drawer-sign-in').hidden).toBe(true)
    await started({ asker: 'person', person: BOB, can: 'edit' })
    expect(document.getElementById('drawer-sign-in').hidden).toBe(true)
  })
})
