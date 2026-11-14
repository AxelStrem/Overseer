import { describe, it, expect, beforeEach, vi } from 'vitest'

// A plot that says `curve="trend"` is drawn as its points and a line fitted to them, which goes
// through none of them in particular. jsdom draws nothing, so what is checked is the
// configuration Chart.js is handed.

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

const plot = (name, series, parameters = {}) =>
  Object.assign(base(name, 'plot'), {
    parameters: {
      label: { String: name },
      _computed_series: { String: JSON.stringify(series) },
      ...parameters,
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

// A steady climb with one day far above it.
const climb = [[0, 0], [1, 1], [2, 2], [3, 3], [4, 40], [5, 5], [6, 6], [7, 7], [8, 8]]
const trend = { curve: { String: 'trend' } }
const at = (line, x) => line.data.reduce((best, p) => Math.abs(p.x - x) < Math.abs(best.x - x) ? p : best).y

describe('a line plot can follow a trend', () => {
  beforeEach(setupDOM)

  it('draws its points as dots and a smooth line that does not chase the odd one', () => {
    const [dots, line] = render([plot('a', climb)], trend).data.datasets
    expect(dots.showLine).toBe(false)
    expect(dots.data).toHaveLength(climb.length)
    expect(line.pointRadius).toBe(0)
    expect(line.borderColor).toBe(dots.borderColor)
    expect(at(line, 4)).toBeLessThan(25)
    expect(at(line, 4)).toBeGreaterThan(4)
    expect(line.data[0].x).toBe(0)
    expect(line.data[line.data.length - 1].x).toBe(8)
  })

  it('follows a straight run of points exactly', () => {
    const [, line] = render([plot('a', [[0, 1], [1, 3], [2, 5], [3, 7], [4, 9]])], trend).data.datasets
    for (const p of line.data) expect(p.y).toBeCloseTo(1 + 2 * p.x)
  })

  it('keeps closer to its points the less smoothing it is given', () => {
    const smooth = render([plot('a', climb)], { ...trend, smoothing: { Float: 1 } }).data.datasets[1]
    const close = render([plot('a', climb)], { ...trend, smoothing: { Float: 0 } }).data.datasets[1]
    expect(at(close, 4)).toBeGreaterThan(at(smooth, 4))
  })

  it('lists the plot once, and says nothing for the line in a tooltip', () => {
    const { data, options } = render([plot('a', climb), plot('b', climb, { curve: { String: 'straight' } })], trend)
    expect(data.datasets).toHaveLength(3)
    const listed = data.datasets
      .map((_, datasetIndex) => ({ datasetIndex }))
      .filter(item => options.plugins.legend.labels.filter(item, data))
    expect(listed.map(item => item.datasetIndex)).toEqual([0, 2])
    expect(options.plugins.tooltip.filter({ dataset: data.datasets[0] })).toBe(true)
    expect(options.plugins.tooltip.filter({ dataset: data.datasets[1] })).toBe(false)
  })

  it('hides the line with its dots when the plot is pressed in the legend', () => {
    const { data, options } = render([plot('a', climb), plot('b', climb)], trend)
    const shown = data.datasets.map(() => true)
    const chart = {
      data,
      isDatasetVisible: (i) => shown[i],
      setDatasetVisibility: (i, to) => { shown[i] = to },
      update: () => {},
    }
    options.plugins.legend.onClick(null, { datasetIndex: 2 }, { chart })
    expect(shown).toEqual([true, true, false, false])
  })

  it('draws a plot of fewer than three points straight', () => {
    const datasets = render([plot('a', [[0, 1], [1, 2]])], trend).data.datasets
    expect(datasets).toHaveLength(1)
    expect(datasets[0].showLine).toBeUndefined()
  })

  it('is fitted to the even steps of a chart that spaces its points evenly', () => {
    const far = [[0, 1], [1, 3], [1000, 5], [1001, 7], [5000, 9]]
    const [, line] = render([plot('a', far)], { ...trend, 'x-scale': { String: 'even' } }).data.datasets
    for (const p of line.data) expect(p.y).toBeCloseTo(1 + 2 * p.x)
  })
})
