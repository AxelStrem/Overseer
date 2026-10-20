import { describe, it, expect, beforeEach, vi } from 'vitest'

// Who is looking, on a server that keeps accounts. The server says who is asking and whether the
// open document is theirs to change; the page lists what they may open under whose it is, draws a
// document shared only to be read as read-only, keeps from a stranger what would be refused them,
// and gives a person their profile, and an admin the links that let others in.

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }))
import { invoke } from '@tauri-apps/api/core'
import { Account, groupsOf } from '../src/account.js'
import { OverseerRenderer } from '../src/renderer.js'

function setupDOM() {
  document.body.className = 'in-browser'
  document.body.innerHTML = `
    <section id="drawer-account" hidden>
      <span id="account-who"></span>
      <button id="account-sharing"></button>
      <input id="account-name">
      <button id="account-save-name"></button><div id="account-name-said"></div>
      <input id="account-current"><input id="account-password">
      <button id="account-save-password"></button><div id="account-password-said"></div>
      <details id="account-admin" hidden>
        <button id="account-invite"></button>
        <input id="account-reset-username">
        <button id="account-reset"></button>
        <div id="account-invite-said"></div>
        <div id="account-link-row" hidden><input id="account-link"><button id="account-copy"></button></div>
      </details>
      <button id="account-sign-out"></button>
    </section>
    <a id="drawer-all-documents" href="/">All documents</a>
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

  it('offers a stranger the way in, and no profile', async () => {
    await started({ asker: 'stranger', person: null, can: 'read' })
    expect(document.body.classList.contains('viewer-stranger')).toBe(true)
    expect(document.getElementById('drawer-account').hidden).toBe(true)
    expect(document.getElementById('drawer-sign-in').hidden).toBe(false)
    expect(document.getElementById('drawer-sign-in').getAttribute('href')).toBe('/signin?next=%2Fdoc%2Fap31%2Ftasks.os')
    expect(document.getElementById('drawer-all-documents').hidden).toBe(true)
  })

  it('leaves the token and the desktop app as they were', async () => {
    await started({ asker: 'anyone', person: null, can: 'edit' })
    expect(document.body.classList.contains('viewer-stranger')).toBe(false)
    expect(document.getElementById('drawer-account').hidden).toBe(true)
    expect(document.getElementById('drawer-sign-in').hidden).toBe(true)
  })

  it('shows a person their name, and saves a new one', async () => {
    fetching({ '/account/me': { ...BOB, name: 'Robert' } })
    await started({ asker: 'person', person: BOB, can: 'edit' })
    expect(document.getElementById('drawer-account').hidden).toBe(false)
    expect(document.getElementById('account-who').textContent).toBe('Bob')
    expect(document.getElementById('account-admin').hidden).toBe(true)
    document.getElementById('account-name').value = 'Robert'
    document.getElementById('account-save-name').click()
    await vi.waitFor(() => expect(document.getElementById('account-who').textContent).toBe('Robert'))
    expect(fetched).toContainEqual({ url: '/account/me', body: { name: 'Robert' } })
  })

  it('opens their sharing document, made when there is none', async () => {
    fetching({ '/account/sharing': { document: 'bob/sharing.os' } })
    const { account } = await started({ asker: 'person', person: BOB, can: 'edit' })
    document.getElementById('account-sharing').click()
    await vi.waitFor(() => expect(account.navigate).toHaveBeenCalledWith('/doc/bob/sharing.os'))
  })

  it('gives an admin an invite link to copy, and a refusal is said as it was', async () => {
    fetching({ '/account/invite': { link: '/join?invite=abc' } })
    await started({ asker: 'person', person: AP31, can: 'edit' })
    expect(document.getElementById('account-admin').hidden).toBe(false)
    document.getElementById('account-invite').click()
    await vi.waitFor(() => expect(document.getElementById('account-link-row').hidden).toBe(false))
    expect(document.getElementById('account-link').value).toBe(window.location.origin + '/join?invite=abc')

    fetching({})
    document.getElementById('account-reset-username').value = 'bob'
    document.getElementById('account-reset').click()
    await vi.waitFor(() => expect(document.getElementById('account-invite-said').textContent).toBe('only an admin may invite'))
    expect(fetched).toContainEqual({ url: '/account/invite', body: { reset: 'bob' } })
  })
})
