import { describe, it, expect, vi, beforeAll } from 'vitest'

// `image plate = "images/plate.png"` and `![plate](images/plate.png)` draw a raster image the
// document keeps beside it, asked for by the open document and its path; an image from elsewhere
// is drawn as it was, sent no referrer.

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

const asked = []
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(async (cmd, args) => {
    if (cmd !== 'load_overseer_image') return null
    asked.push(args)
    if (args.image === 'images/missing.png') throw new Error('no image')
    return { mime: 'image/png', data: 'iVBORw0KGgo=' }
  }),
}))
import { OverseerApp } from '../src/main.js'

beforeAll(() => {
  let n = 0
  URL.createObjectURL = () => `blob:image-${++n}`
})

const base = (name, node_type, parameters = {}, children = []) => ({
  name, node_type, parameters, children, is_hierarchy_transparent: false,
  source_id: '', source_fingerprint: 1, param_order: [], authored_dash: false,
})

const draw = (children, file = 'kitchen/t.os') => {
  setupDOM()
  const app = new OverseerApp()
  window.app = app
  app.currentDocument = [base('t', 'tab', {}, children)]
  app.currentFile = file
  app.renderer._filters = new Map()
  app.renderer.renderDocument(app.currentDocument)
  return app
}

const at = (name) => document.querySelector(`[data-path='${JSON.stringify(['t', name])}']`)
const settle = () => new Promise((r) => setTimeout(r, 0))
const image = (name, path, parameters = {}) => base(name, 'image', { value: { String: path }, ...parameters })
const text = (name, body) => base(name, 'text', { value: { String: body } })
const askedFor = (path) => asked.filter((a) => a.image === path).length

describe('a document image is drawn from its path', () => {
  it('an image node asks the open document for it, and draws it named by its name', async () => {
    draw([image('plate', 'images/plate.png')])
    const img = at('plate').querySelector('img')
    expect(img.dataset.imageFile).toBe('images/plate.png')
    expect(img.alt).toBe('plate')
    await settle()
    expect(asked).toContainEqual({ path: 'kitchen/t.os', image: 'images/plate.png' })
    expect(img.getAttribute('src')).toMatch(/^blob:image-\d+$/)
  })

  it('a label heads the image and is its alt text', () => {
    draw([image('plate', 'images/plate.png', { label: { String: 'Lunch' } })])
    expect(at('plate').querySelector('label').textContent).toBe('Lunch')
    expect(at('plate').querySelector('img').alt).toBe('Lunch')
  })

  it('is asked for once, and drawn at once the next time', async () => {
    draw([image('a', 'images/once.png'), image('b', 'images/once.png')])
    await settle()
    expect(askedFor('images/once.png')).toBe(1)
    draw([image('a', 'images/once.png')])
    expect(at('a').querySelector('img').getAttribute('src')).toMatch(/^blob:image-\d+$/)
    expect(askedFor('images/once.png')).toBe(1)
  })

  it('a path out of the folder, or not to an image, is never asked for', async () => {
    const refused = ['../up.png', '/up.png', 'C:/up.png', 'images/../../up.png', './images/x.png', 'images/x.svg', 'pantry.os', '']
    draw(refused.map((p, i) => image(`r${i}`, p)))
    await settle()
    for (const p of refused) expect(askedFor(p)).toBe(0)
    expect(at('r0').querySelector('img').hasAttribute('src')).toBe(false)
  })

  it('a missing image stays an empty box', async () => {
    draw([image('gone', 'images/missing.png')])
    await settle()
    await settle()
    const img = at('gone').querySelector('img')
    expect(img.hasAttribute('src')).toBe(false)
    expect(img.classList.contains('image-missing')).toBe(true)
  })

  it('in markdown, a relative path is an image beside the document', async () => {
    draw([text('note', 'Look: ![the plate](images/md.png)')])
    const img = at('note').querySelector('.text-content img')
    expect(img.dataset.imageFile).toBe('images/md.png')
    expect(img.alt).toBe('the plate')
    await settle()
    expect(asked).toContainEqual({ path: 'kitchen/t.os', image: 'images/md.png' })
    expect(img.getAttribute('src')).toMatch(/^blob:image-\d+$/)
  })

  it('in markdown, an image from elsewhere is sent no referrer, and a refused path is its words', async () => {
    draw([text('note', '![far](https://example.com/x.png) ![up](../up.png) ![js](javascript:alert(1))')])
    const imgs = at('note').querySelectorAll('.text-content img')
    expect(imgs.length).toBe(1)
    expect(imgs[0].getAttribute('src')).toBe('https://example.com/x.png')
    expect(imgs[0].getAttribute('referrerpolicy')).toBe('no-referrer')
    expect(at('note').querySelector('.text-content').textContent).toContain('up')
    await settle()
    expect(askedFor('../up.png')).toBe(0)
  })

  it('an image under a folded div is not asked for until it is drawn', async () => {
    draw([base('box', 'div', { foldable: { Boolean: true }, folded: { Boolean: true }, label: { String: 'Box' } },
      [image('hidden', 'images/folded.png')])])
    await settle()
    expect(askedFor('images/folded.png')).toBe(0)
  })
})
