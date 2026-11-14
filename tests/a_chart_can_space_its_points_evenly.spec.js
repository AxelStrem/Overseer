import { describe, it, expect, beforeEach, vi } from 'vitest'

// A chart that says `x-scale="even"` gives every x one equal step, whatever lies between them.
// jsdom draws nothing, so what is checked is the configuration Chart.js is handed.

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

const DAY = 86400000
const START = Date.UTC(2026, 0, 1)

const plot = (name, series) =>
  Object.assign(base(name, 'plot'), {
    parameters: {
      label: { String: name },
      _computed_series: { String: JSON.stringify(series) },
    },
  })

const render = (plots, parameters = {}) => {
  drawn.length = 0
  const app = new OverseerApp()
  app.currentDocument = [Object.assign(base('t', 'tab'), {
    children: [Object.assign(base('trend', 'chart'), { parameters, children: plots })],
  })]
  app._currentText = 'TEXT'
  app.renderer.renderDocument(app.currentDocument)
  return drawn[drawn.length - 1]
}

// Two days, then nothing for a year, then two more; the second plot's days fall between.
const first = plot('a', [[START, 1], [START + DAY, 2], [START + 365 * DAY, 3], [START + 366 * DAY, 4]])
const second = plot('b', [[START + 2 * DAY, 5], [START + 367 * DAY, 6]])
const even = { 'x-scale': { String: 'even' } }

describe('a chart can space its points evenly', () => {
  beforeEach(setupDOM)

  it('draws each point at its real x when nothing says otherwise', () => {
    const { data } = render([first, second])
    expect(data.datasets[0].data.map(p => p.x)).toEqual([START, START + DAY, START + 365 * DAY, START + 366 * DAY])
  })

  it('gives every x of its plots one equal step, in order', () => {
    const { data } = render([first, second], even)
    expect(data.datasets[0].data.map(p => p.x)).toEqual([0, 1, 3, 4])
    expect(data.datasets[1].data.map(p => p.x)).toEqual([2, 5])
    expect(data.datasets[0].data.map(p => p.y)).toEqual([1, 2, 3, 4])
  })

  it('still says the real x on its ticks and in its tooltips', () => {
    const { data, options } = render([first, second], even)
    const ticks = options.scales.x.ticks
    expect(ticks.precision).toBe(0)
    expect(ticks.callback(3)).toBe(new Date(START + 365 * DAY).toLocaleDateString())
    const point = data.datasets[0].data[2]
    const title = options.plugins.tooltip.callbacks.title([{ raw: point, parsed: { x: point.x } }])
    expect(title).toBe(new Date(START + 365 * DAY).toLocaleString())
  })

  it('puts a fixed domain where its values stand among the points', () => {
    const { options } = render([first, second], {
      ...even,
      'domain-x-min': { Float: START + DAY },
      'domain-x-max': { Float: START + 365.5 * DAY },
    })
    expect(options.scales.x.min).toBe(1)
    expect(options.scales.x.max).toBeCloseTo(3.5)
  })
})
