import { describe, it, expect, beforeEach, vi } from 'vitest'

// A bar chart where each bar is a share of its own limit.
//
// The figures it draws are not comparable as they stand: salt is in grams and caffeine in
// milligrams, so on one axis the salt bar is a thousandth the height of the caffeine one and the
// chart says nothing at all. Against their own limits they are comparable, and the limit line is
// the reading - a bar past it is past it, whatever the units were.
//
// jsdom draws nothing, so what these check is the configuration Chart.js is handed: the heights,
// the line, the axis, and the colour that marks an overrun. That is where the decisions are.

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

/** A plot as the resolver hands it over: the formulas answered into computed shadows. */
const plot = (name, label, amount, limit, { colour = '#4A90E2', suffix = '' } = {}) =>
  Object.assign(base(name, 'plot'), {
    parameters: {
      label: { String: label },
      color: { String: colour },
      suffix: { String: suffix },
      amount: { Formula: 'totals/whatever' },
      _computed_amount: { Float: amount },
      ...(limit === null ? {} : { limit: { Formula: 'limits/whatever' }, _computed_limit: { Float: limit } }),
    },
  })

const render = (plots) => {
  drawn.length = 0
  const app = new OverseerApp()
  app.currentDocument = [Object.assign(base('t', 'tab'), {
    children: [
      Object.assign(base('allowances', 'chart'), {
        parameters: { kind: { String: 'bar' }, width: { CssSize: { Pixels: 240 } } },
        children: plots,
      }),
    ],
  })]
  app._currentText = 'TEXT'
  app.renderer._filters = new Map()
  app.renderer.renderDocument(app.currentDocument)
  return drawn[drawn.length - 1]
}

const bars = (config) => config.data.datasets.find(d => d.type !== 'line')
const line = (config) => config.data.datasets.find(d => d.type === 'line')

describe('bars measured against a limit', () => {
  beforeEach(setupDOM)

  it('draws each bar as a share of its own limit', () => {
    // Salt in grams and caffeine in milligrams, three orders of magnitude apart as figures and
    // side by side as shares. This is the whole reason the chart normalises.
    const config = render([
      plot('salt', 'Salt', 4, 5, { suffix: ' g' }),
      plot('caffeine', 'Caffeine', 200, 400, { suffix: ' mg' }),
    ])
    expect(bars(config).data).toEqual([0.8, 0.5])
  })

  it('puts the limit at the same height for every bar', () => {
    // What was asked for: one line meaning every limit at once.
    const config = render([
      plot('salt', 'Salt', 4, 5),
      plot('sugar', 'Sugar', 75, 50),
      plot('caffeine', 'Caffeine', 100, 400),
    ])
    expect(line(config).data).toEqual([1, 1, 1])
  })

  it('marks a bar that is over its limit', () => {
    // Not left to the reader to compare a bar against a line by eye.
    const config = render([
      plot('salt', 'Salt', 4, 5, { colour: '#e2a24a' }),
      plot('sugar', 'Sugar', 75, 50, { colour: '#c77dd6' }),
    ])
    const colours = bars(config).backgroundColor
    expect(colours[0]).not.toBe(colours[1])
    expect(colours[1].toLowerCase()).toContain('e63e11')
  })

  it('keeps the limit line in the upper half when everything is under', () => {
    // A fixed ceiling would draw five stubs against a line near the top on a good day.
    const config = render([plot('salt', 'Salt', 1, 5), plot('sugar', 'Sugar', 5, 50)])
    expect(config.options.scales.y.max).toBeLessThanOrEqual(1.5)
    expect(config.options.scales.y.max).toBeGreaterThan(1)
  })

  it('stops growing the axis at three times over', () => {
    // Real days reach four times the sugar limit. An axis that fits that leaves the other bars
    // a few pixels high, so it stops - the bar clips, stays red, and the tooltip has the figure.
    const config = render([
      plot('sugar', 'Sugar', 205, 50),
      plot('caffeine', 'Caffeine', 192, 400),
    ])
    expect(config.options.scales.y.max).toBe(3)
    expect(bars(config).data[0]).toBeCloseTo(4.1, 5)
  })

  it('leaves out a plot with no limit rather than drawing it against the others', () => {
    // It has nothing to be a share of, and putting it on this axis would be a category error.
    const config = render([
      plot('salt', 'Salt', 4, 5),
      plot('protein', 'Protein', 90, null),
    ])
    expect(bars(config).data).toEqual([0.8])
    expect(config.data.labels).toEqual(['Salt'])
  })

  it('leaves out a plot whose limit is zero rather than dividing by it', () => {
    const config = render([plot('salt', 'Salt', 4, 5), plot('broken', 'Broken', 1, 0)])
    expect(config.data.labels).toEqual(['Salt'])
  })

  it('says the real figures when a bar is pointed at, not the share alone', () => {
    // "84%" is the reading; "4.2 g of 5 g" is the fact behind it, and the units only exist here.
    const config = render([plot('salt', 'Salt', 4.2, 5, { suffix: ' g' })])
    const text = config.options.plugins.tooltip.callbacks.label({
      label: 'Salt', dataIndex: 0, parsed: { y: 0.84 }, datasetIndex: 0,
    })
    expect(text).toBe('Salt: 4.2 g of 5 g (84%)')
  })

  it('does not offer a tooltip for the limit line', () => {
    // It is the same value on every bar and says nothing when hovered.
    const config = render([plot('salt', 'Salt', 4, 5)])
    const keep = config.options.plugins.tooltip.filter
    expect(keep({ datasetIndex: 0 })).toBe(true)
    expect(keep({ datasetIndex: 1 })).toBe(false)
  })

  it('is a bar chart', () => {
    expect(render([plot('salt', 'Salt', 4, 5)]).type).toBe('bar')
  })
})
