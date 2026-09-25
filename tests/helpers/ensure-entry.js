// A stand-in for the `ensure_overseer_entry` instruction, for specs about the page.
//
// A view onto a key its list does not have shows the template as a preview; editing a field in it
// asks the backend to make the entry real at that key and apply the edit. The page used to do the
// making itself, in its own copy of the document, so a spec could watch it happen with no backend
// at all. It cannot now, and a spec that wants to see what the page does *after* the entry exists
// needs something to answer.
//
// Written to mirror `ActionExecutor::ensure_in_list` rather than to satisfy the specs: find by key
// and do nothing if it is there, otherwise clone the template, set the key, apply the fields, and
// insert at the end or the front. A fake that is kinder than the real thing would hide exactly the
// faults these specs exist to catch.
//
// What the real one does that this cannot is resolve the document afterwards, so template fields
// arrive as the template declared them and formulas keep whatever the clone carried. Specs that
// care about worked-out values want the Rust side instead - `a_preview_becomes_real.rs`.
//
// Nor does it run a press that rides along in `wanted.then`: that is the backend running a
// handler, which is `a_press_on_a_new_day_is_one_step.rs`. A spec here can check what it was
// asked to run, which is what the page decides.

const clone = (o) => JSON.parse(JSON.stringify(o))

/// What a key reads as once `keyPrecision` has had its say. The same rule the renderer applies.
function keyReadsAs(precision, value) {
  if (value === null || value === undefined) return ''
  const said = String(value)
  const p = String(precision || '').toLowerCase()
  if (p === 'day' || p === 'days') {
    const m = said.match(/^(\d{4})[./-](\d{2})[./-](\d{2})(?:.*)?$/)
    if (m) return `${m[1]}-${m[2]}-${m[3]}`
    const d = new Date(said)
    if (!isNaN(d.getTime())) {
      const pad = (n) => String(n).padStart(2, '0')
      return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`
    }
  }
  return said
}

const plainly = (v) => (v && typeof v === 'object')
  ? (v.String ?? v.Timestamp ?? v.Date ?? v.Integer ?? v.Float ?? v.Boolean ?? null)
  : v

const said = (v) => {
  if (typeof v === 'string') return v
  if (v && typeof v === 'object') {
    if (v.Template !== undefined) return String(v.Template)
    if (v.String !== undefined) return String(v.String)
  }
  return null
}

/// The node at a path of names, and the index path that reaches it.
function walkByName(roots, names) {
  let list = roots
  let node = null
  const indices = []
  for (const name of names) {
    const at = (list || []).findIndex((n) => n && n.name === name)
    if (at < 0) return null
    indices.push(at)
    node = list[at]
    list = node.children || []
  }
  return node ? { node, indices } : null
}

function findTemplate(roots, name) {
  for (const node of roots || []) {
    if (node?.name === name) return node
    const found = findTemplate(node?.children, name)
    if (found) return found
  }
  return null
}

/// Answer `ensure_overseer_entry` against `app.currentDocument`, or return null for other
/// commands so a caller can chain its own handling.
///
/// `shape` picks which of the two answers the real backend gives. It describes a change as a
/// delta only while it still holds the document it last worked out for this exact text; it does
/// not when the document was worked out for a viewer rather than plainly, and a guarded field
/// makes that so for anybody who has moved the day - which is everybody who reaches a day the
/// history has not got. So `nodes` is the shape that matters most here, and it is the default:
/// `changes` was the only one a spec exercised, and the page handled only that one.
export function answerEnsureEntry(appOrGetter, cmd, args,
                                  { text = 'AFTER THE WRITE', shape = 'nodes' } = {}) {
  if (cmd !== 'ensure_overseer_entry') return null
  // A getter as well as an app, because a spec often installs its mock before it has one.
  const app = typeof appOrGetter === 'function' ? appOrGetter() : appOrGetter
  if (!app || !app.currentDocument) return Promise.resolve(null)

  const wanted = args?.wanted || {}
  const names = wanted.list_path || wanted.listPath || []
  const found = walkByName(app.currentDocument, names)
  if (!found || (found.node.node_type || '').toLowerCase() !== 'list') {
    return Promise.reject(new Error(`${names.join('/')} is not a list`))
  }

  const list = clone(found.node)
  const keyField = wanted.key_field || wanted.keyField
    || said(list.parameters?.key) || 'id'
  const precision = said(list.parameters?.keyPrecision)
  const key = plainly(wanted.key_value ?? wanted.keyValue)
  const wantedKey = keyReadsAs(precision, key)

  let entry = (list.children || []).find((child) => {
    const field = (child.children || []).find((c) => c?.name === keyField)
    return keyReadsAs(precision, plainly(field?.parameters?._computed_value
      ?? field?.parameters?.value)) === wantedKey
  })

  if (!entry) {
    const template = findTemplate(app.currentDocument, wanted.template)
    if (!template) {
      return Promise.reject(new Error(`no template named ${wanted.template}`))
    }
    entry = clone(template)
    // The clone must not carry the template's provenance, or the serializer replays the
    // template's own text for it.
    const strip = (n) => {
      delete n.source_id; delete n.source_fingerprint; delete n.source_snapshot
      for (const c of n.children || []) strip(c)
    }
    strip(entry)
    entry.parameters = Object.assign({}, entry.parameters, { _from_template: true })
    entry.name = `${template.name}__${(list.children || []).length + 1}`
    let keyChild = (entry.children || []).find((c) => c?.name === keyField)
    if (!keyChild) {
      keyChild = { name: keyField, node_type: 'string', parameters: {}, children: [], is_hierarchy_transparent: false }
      entry.children.unshift(keyChild)
    }
    keyChild.parameters = Object.assign({}, keyChild.parameters,
      { value: wanted.key_value ?? wanted.keyValue })
    if (String(wanted.position || '').startsWith('prepend')
      || String(wanted.position || '') === 'first') {
      list.children.unshift(entry)
    } else {
      list.children.push(entry)
    }
  }

  // The edit that asked for this, named relative to the entry.
  for (const [field, value] of Object.entries(wanted.fields || {})) {
    let at = entry
    const segments = String(field).split('/').filter(Boolean)
    for (let i = 0; i < segments.length; i += 1) {
      at.children = at.children || []
      let next = at.children.find((c) => c?.name === segments[i])
      if (!next) {
        next = {
          name: segments[i],
          node_type: i === segments.length - 1 ? 'string' : 'div',
          parameters: {}, children: [], is_hierarchy_transparent: false,
        }
        at.children.push(next)
      }
      at = next
    }
    at.parameters = Object.assign({}, at.parameters, {
      value,
      _explicit_child_override: { Boolean: true },
      _override_present: { Boolean: true },
    })
  }

  if (shape === 'changes') {
    return Promise.resolve({
      wrote: true,
      text,
      file_text: text,
      changes: [{ kind: 'subtree', path: found.indices, node: list }],
      nodes: null,
    })
  }
  // The whole document, with the list put back where it came from.
  const whole = clone(app.currentDocument)
  let at = { children: whole }
  for (const index of found.indices.slice(0, -1)) at = (at.children || [])[index] || {}
  if (Array.isArray(at.children)) at.children[found.indices[found.indices.length - 1]] = list
  return Promise.resolve({
    wrote: true,
    text,
    file_text: text,
    changes: null,
    nodes: whole,
  })
}

/// Install it on an `invoke` mock, keeping whatever that mock already answers.
export function withEnsureEntry(invoke, appOrGetter, rest = () => Promise.resolve(null), options) {
  invoke.mockImplementation((cmd, args) => {
    const answer = answerEnsureEntry(appOrGetter, cmd, args, options)
    return answer !== null ? answer : rest(cmd, args)
  })
}
