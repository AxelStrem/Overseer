// A calendar as the resolver hands it to the page, for the specs of its drawings: a cell for
// every day from `from` to `to`, in date order, each named by its date and in view.

export function setupCalendarDOM() {
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

const base = (name, node_type) => ({
  name, node_type, parameters: {}, children: [], is_hierarchy_transparent: false,
  source_id: '', source_fingerprint: 1, param_order: [], authored_dash: false,
})

const iso = (ms) => new Date(ms).toISOString().slice(0, 10)

export function calendarDocument({ from, to, layout, header, values = {} }) {
  const parameters = { entry: { String: 'int' }, from: { String: from }, to: { String: to } }
  if (layout !== undefined) parameters.layout = { String: layout }
  if (header !== undefined) parameters.header = { Boolean: header }
  const children = []
  for (let ms = Date.parse(from); ms <= Date.parse(to); ms += 86400000) {
    const name = iso(ms)
    children.push(Object.assign(base(name, 'int'), {
      parameters: { value: { Integer: values[name] || 0 }, _in_view: { Boolean: true } },
    }))
  }
  return [Object.assign(base('t', 'tab'), {
    children: [Object.assign(base('Days', 'calendar'), { parameters, children })],
  })]
}

export const calendarElement = () => document.querySelector('.overseer-calendar')

/// What the calendar draws, in order: `#text` a heading, `-` a blank, a date a day.
export const drawnInOrder = () => Array.from(calendarElement().children).map(el => {
  if (el.classList.contains('overseer-grid-heading')) return `#${el.textContent}`
  if (el.classList.contains('overseer-calendar-blank')) return '-'
  return JSON.parse(el.dataset.path).slice(-1)[0]
})
