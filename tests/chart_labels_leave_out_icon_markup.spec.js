import { describe, it, expect, beforeEach, vi } from 'vitest'

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

/** Chart.js needs a canvas context jsdom does not provide, so the config is captured instead. */
const drawn = []
vi.mock('chart.js', () => {
  class Chart {
    constructor(canvas, config) {
      drawn.push(config)
      this.config = config
    }
    destroy() {}
    update() {}
    static register() {}
  }
  const stub = {}
  return {
    Chart,
    CategoryScale: stub, LinearScale: stub, PointElement: stub, LineElement: stub,
    LineController: stub, ArcElement: stub, PieController: stub, DoughnutController: stub,
    BarElement: stub, BarController: stub, Title: stub, Tooltip: stub, Legend: stub,
  }
})
import { OverseerApp } from '../src/main.js'

// A chart is drawn on a canvas, which holds only text. A label with an icon in it - a tag's
// `<icon=bug> wrong` - gives its words to the axis and the legend, never the markup.

const base = (name, node_type) => ({
  name, node_type, parameters: {}, children: [], is_hierarchy_transparent: false,
  source_id: '', source_fingerprint: 1, param_order: [], authored_dash: false,
})

const render = (kind, plots) => {
  drawn.length = 0
  const app = new OverseerApp()
  app.currentDocument = [Object.assign(base('t', 'tab'), {
    children: [
      Object.assign(base('c', 'chart'), {
        parameters: { kind: { String: kind }, width: { CssSize: { Pixels: 240 } } },
        children: plots,
      }),
    ],
  })]
  app._currentText = 'TEXT'
  app.renderer.renderDocument(app.currentDocument)
  return drawn[drawn.length - 1]
}

const perTag = () => Object.assign(base('each', 'plot'), {
  parameters: {
    source: { Formula: '/project/Labels' },
    label: { Formula: '|t| t/label' },
    _computed_bars: { String: JSON.stringify([
      { label: '<icon=bug> wrong', amount: 3, limit: 4 },
      { label: '<icon=file-text color=#ff0000> documents', amount: 2, limit: 4 },
      { label: '<icon=server>', amount: 1, limit: 4 },
    ]) },
  },
})

describe('chart labels leave out icon markup', () => {
  beforeEach(setupDOM)

  it('gives a bar chart over a list the words of each label', () => {
    expect(render('bar', [perTag()]).data.labels).toEqual(['wrong', 'documents', 'each'])
  })

  it('gives a pie over a list the words of each label', () => {
    expect(render('pie', [perTag()]).data.labels).toEqual(['wrong', 'documents', 'each'])
  })

  it('gives a plot of its own the words of its label', () => {
    const own = Object.assign(base('salt', 'plot'), {
      parameters: { label: { String: '<icon=flame> Salt' }, _computed_amount: { Float: 4 } },
    })
    expect(render('pie', [own]).data.labels).toEqual(['Salt'])
  })

  it('gives a line series the words of its label', () => {
    const series = Object.assign(base('weight', 'plot'), {
      parameters: {
        label: { String: '<icon=scale> Weight' },
        _computed_series: { String: JSON.stringify([[1, 70], [2, 71]]) },
      },
    })
    const config = render('line', [series])
    expect(config.data.datasets.map(d => d.label)).toContain('Weight')
    expect(config.data.datasets.some(d => String(d.label).includes('<icon='))).toBe(false)
  })
})
