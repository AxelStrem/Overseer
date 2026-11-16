import { describe, it, expect, vi } from 'vitest'

// A mount brought in on open (`lazy=false`) and there has nothing to load and nothing to report,
// so it draws what it holds and nothing of its own - a project's definition in its tracker's tab.
// A lazy mount, or one that could not be had, still says where it reads from, how it stands, and
// offers to load it.

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

const base = (name, node_type, parameters = {}, children = []) => ({
  name, node_type, parameters, children, is_hierarchy_transparent: false,
  source_id: '', source_fingerprint: 1, param_order: [], authored_dash: false,
})

const held = () => [base('definition', 'div', {}, [
  base('description', 'text', { value: { String: 'What P is.' } }),
])]

const draw = (mountParameters, children) => {
  setupDOM()
  const app = new OverseerApp()
  app.currentDocument = [base('t', 'tab', {}, [base('definition', 'mount', mountParameters, children)])]
  app.currentFile = 't.os'
  app.renderer.renderDocument(app.currentDocument)
  return document.querySelector('.overseer-mount')
}

describe('a mount brought in on open draws only what it holds', () => {
  it('draws what it holds with no title, status or load button', () => {
    const mount = draw({
      source: { String: 'p.project.os/definition' },
      lazy: { Boolean: false },
      _mount_status: { String: 'loaded' },
    }, held())
    expect(mount.querySelector('.mount-title')).toBeNull()
    expect(mount.querySelector('.mount-status')).toBeNull()
    expect(mount.querySelector(':scope > button')).toBeNull()
    expect(document.body.textContent).toContain('What P is.')
  })

  it('still offers to load a lazy mount', () => {
    const mount = draw({
      source: { String: 'p.project.os/definition' },
      _mount_status: { String: 'unloaded' },
    }, [])
    expect(mount.querySelector('.mount-title').textContent).toBe('p.project.os/definition')
    expect(mount.querySelector('.mount-status').textContent).toBe('Status: unloaded')
    expect(mount.querySelector('button')).not.toBeNull()
  })

  it('still says why a mount brought in on open could not be had', () => {
    const mount = draw({
      source: { String: 'p.project.os/definition' },
      lazy: { Boolean: false },
      _mount_status: { String: 'error' },
      _mount_error: { String: 'no such file' },
    }, [])
    expect(mount.querySelector('.mount-status').textContent).toBe('Status: error')
    expect(mount.querySelector('.mount-error').textContent).toBe('no such file')
  })
})
