import { describe, it, expect, beforeEach, vi } from 'vitest'

// How a line plot goes from point to point, said by `curve` on the chart or on a plot. jsdom draws
// nothing, so what is checked is the configuration Chart.js is handed.

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
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }))
import { OverseerApp } from '../src/main.js'


const base = (name, node_type) => ({
  name, node_type, parameters: {}, children: [], is_hierarchy_transparent: false,
  source_id: '', source_fingerprint: 1, param_order: [], authored_dash: false,
})

/** A line plot as the resolver hands it over: its points already worked out. */
const plot = (name, curve) =>
  Object.assign(base(name, 'plot'), {
    parameters: {
      label: { String: name },
      _computed_series: { String: JSON.stringify([[1, 2], [2, 5], [3, 3]]) },
      ...(curve ? { curve: { String: curve } } : {}),
    },
  })

const render = (plots, curve) => {
  drawn.length = 0
  const app = new OverseerApp()
  app.currentDocument = [Object.assign(base('t', 'tab'), {
    children: [
      Object.assign(base('trend', 'chart'), {
        parameters: curve ? { curve: { String: curve } } : {},
        children: plots,
      }),
    ],
  })]
  app._currentText = 'TEXT'
  app.renderer._filters = new Map()
  app.renderer.renderDocument(app.currentDocument)
  return drawn[drawn.length - 1].data.datasets
}

describe('a line plot can be drawn smooth', () => {
  beforeEach(setupDOM)

  it('draws straight lines when nothing says otherwise', () => {
    const [line] = render([plot('a')])
    expect(line.stepped).toBe(false)
    expect(line.cubicInterpolationMode).toBeUndefined()
  })

  it('curves every plot of a chart that says smooth, without overshooting its points', () => {
    const lines = render([plot('a'), plot('b')], 'smooth')
    for (const line of lines) {
      expect(line.tension).toBeGreaterThan(0.1)
      expect(line.cubicInterpolationMode).toBe('monotone')
    }
  })

  it('lets a plot say its own curve over the chart', () => {
    const [smooth, step] = render([plot('a'), plot('b', 'step')], 'smooth')
    expect(smooth.cubicInterpolationMode).toBe('monotone')
    expect(step.stepped).toBe(true)
  })
})
