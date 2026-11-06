import { describe, it, expect, beforeEach, vi } from 'vitest'

// Whoever is looking has a drawer of their own, which the host finds - their `drawer.os`, or the
// base where they have none - and the page draws it in the pane.

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(async () => null) }))
import { invoke } from '@tauri-apps/api/core'
import { OverseerApp } from '../src/main.js'

describe('a person opens their own drawer', () => {
  beforeEach(() => {
    document.body.innerHTML = `
      <div id="app">
        <nav id="drawer"><div id="drawer-document" hidden></div></nav>
        <div id="tab-container"></div><div id="content-display"></div>
        <div id="status-bar"><span id="status-message"></span><span id="status-info"></span>
          <span id="file-path"></span></div>
      </div>`
    invoke.mockReset()
  })

  it('opens what the host names as the drawer', async () => {
    invoke.mockImplementation(async (cmd) => (cmd === 'drawer_location' ? 'drawer.os' : null))
    const app = new OverseerApp()
    const opened = []
    app.openDrawerDocument = async (path) => {
      opened.push(path)
      return null
    }
    await app.openOwnDrawer()
    expect(invoke).toHaveBeenCalledWith('drawer_location')
    expect(opened).toEqual(['drawer.os'])
  })

  it('opens nothing where the host names none', async () => {
    invoke.mockImplementation(async () => '')
    const app = new OverseerApp()
    const opened = []
    app.openDrawerDocument = async (path) => {
      opened.push(path)
      return null
    }
    expect(await app.openOwnDrawer()).toBe(null)
    expect(opened).toEqual([])
  })
})
