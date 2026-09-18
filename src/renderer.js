import { PROFILE, profiled } from './profile.js'
// Debug UI is disabled unless explicitly enabled via ?debug=1 (localStorage flag ignored)
const DEBUG_MODE = (() => {
    try {
        const qs = typeof window !== 'undefined' && window.location && typeof window.location.search === 'string' ? window.location.search : ''
        return !!(qs && qs.includes('debug=1'))
    } catch { return false }
})();

// Import marked for markdown rendering
import { marked } from 'marked';
import { invoke } from '@tauri-apps/api/core'

// Import Chart.js for chart visualization
import {
    Chart,
    CategoryScale,
    LinearScale,
    PointElement,
    LineElement,
    LineController,
    ArcElement,
    PieController,
    DoughnutController,
    BarElement,
    BarController,
    Title,
    Tooltip,
    Legend
} from 'chart.js';

// Register Chart.js components
Chart.register(
    CategoryScale,
    LinearScale,
    PointElement,
    LineElement,
    LineController,
    // A chart that says `kind="pie"` draws slices instead of a line. Chart.js only knows the
    // controllers that are registered, and an unregistered one fails at draw time rather than
    // at configuration - so these belong here even though most charts are lines.
    ArcElement,
    PieController,
    DoughnutController,
    // A chart that says `kind="bar"` draws its figures against a limit each - see the branch
    // in `createChartElement`. Same reason these are listed: unregistered fails at draw time.
    BarElement,
    BarController,
    Title,
    Tooltip,
    Legend
);

export class OverseerRenderer {
    /** What a field is given when the document asks for nothing. */
    static FIELD_PADDING = '4px 6px'
    static VALUE_MIN_HEIGHT = '20px'

    constructor() {
        this.contentDisplay = document.getElementById('content-display')
        this.tabContainer = document.getElementById('tab-container')
        // Track live intervals so we can clear them on each full re-render
        this._liveIntervals = new Set()
        // Track newly materialized targets so updates apply to the exact node, not a loosely-resolved path
        this._materializedTargets = new Map()
    // Default top-level mutability: disabled by default (can be enabled per-node via mutable=true or inherited)
    this._defaultTopLevelMutable = false
    }

    // Compute effective mutability mode at a specific path within a provided document tree.
    // Returns one of: 'true' | 'false' | 'guarded'. Falls back to top-level default when unspecified.
    _computeEffectiveMutableAtPath(doc, pathArr) {
        const parseMode = (v) => {
            if (v === null || v === undefined) return 'inherited'
            if (typeof v === 'boolean') return v ? 'true' : 'false'
            const s = String(v).toLowerCase().trim()
            if (s === 'true') return 'true'
            if (s === 'false') return 'false'
            if (s === 'guarded') return 'guarded'
            if (s === 'inherit' || s === 'inherited') return 'inherited'
            return 'inherited'
        }
        const readMutableParam = (n) => {
            try {
                const comp = this.getParameterValue(n, '_computed_mutable')
                if (comp !== null && comp !== undefined) return comp
            } catch(_) {}
            try { return this.getParameterValue(n, 'mutable') } catch(_) { return null }
        }
        try {
            if (!doc || !Array.isArray(pathArr) || pathArr.length === 0) {
                return this._defaultTopLevelMutable ? 'true' : 'false'
            }
            // Build ancestor chain from deepest to root
            const ancestors = []
            for (let i = pathArr.length; i >= 1; i--) {
                const sub = pathArr.slice(0, i)
                const n = this.findNodeByPath(doc, sub)
                if (n) ancestors.push(n)
            }
            for (const anc of ancestors) {
                const raw = readMutableParam(anc)
                const mode = parseMode(raw)
                if (mode !== 'inherited') return mode
            }
            return this._defaultTopLevelMutable ? 'true' : 'false'
        } catch(_) {
            return this._defaultTopLevelMutable ? 'true' : 'false'
        }
    }

    // Tag value changes that came from backend actions for nodes that are mutable=guarded so they won’t persist on save.
    // This function walks the new document and compares against the old document by canonical path (name + ordinal).
    _tagGuardedChangesAfterBackendUpdate(oldDoc, newDoc) {
        try {
            if (!oldDoc || !newDoc) return
            const deepClone = (obj) => {
                try { if (typeof structuredClone === 'function') return structuredClone(obj) } catch(_) {}
                try { return JSON.parse(JSON.stringify(obj)) } catch(_) { return obj }
            }
            const valuesEqual = (a, b) => {
                try { return window?.app?.valuesEqual ? window.app.valuesEqual(a, b) : JSON.stringify(a) === JSON.stringify(b) } catch(_) { return false }
            }
            const walk = (node, parent, pathArr) => {
                if (!node || typeof node !== 'object') return
                const p = node.parameters || {}
                const oldNode = this.findNodeByPath(oldDoc, pathArr)
                const oldVal = oldNode && oldNode.parameters ? oldNode.parameters.value : undefined
                const newVal = p ? p.value : undefined
                // Determine effective mutability at this path using the NEW doc
                const mode = this._computeEffectiveMutableAtPath(newDoc, pathArr)
                if (mode === 'guarded') {
                    const changed = !valuesEqual(oldVal, newVal)
                    if (!oldNode) {
                        // Newly created node (e.g., new override) under guarded scope
                        if (!node.parameters) node.parameters = {}
                        node.parameters._guarded_edit = { Boolean: true }
                        node.parameters._guarded_was_new_override = { Boolean: true }
                    } else if (changed) {
                        if (!node.parameters) node.parameters = {}
                        node.parameters._guarded_edit = { Boolean: true }
                        // The value to restore on save is the one the document was authored
                        // with, not the one from just before the latest change. Capturing
                        // `oldVal` unconditionally makes each change overwrite the last
                        // capture, so a second edit records the first edit's value as the
                        // "original" - navigate two days and the document saves the day in
                        // between. Once captured, it must not be recaptured.
                        const alreadyCaptured = oldNode.parameters
                            ? oldNode.parameters._guarded_original_value
                            : undefined
                        if (alreadyCaptured !== undefined) {
                            try { node.parameters._guarded_original_value = deepClone(alreadyCaptured) } catch(_) { node.parameters._guarded_original_value = alreadyCaptured }
                        } else if (node.parameters._guarded_original_value === undefined) {
                            if (oldVal === undefined) {
                                node.parameters._guarded_was_new_override = { Boolean: true }
                            } else {
                                try { node.parameters._guarded_original_value = deepClone(oldVal) } catch(_) { node.parameters._guarded_original_value = oldVal }
                            }
                        }
                        // Carry a new-override marker forward too, for the same reason.
                        try {
                            const wasNew = oldNode.parameters && oldNode.parameters._guarded_was_new_override
                            if (wasNew !== undefined && node.parameters._guarded_was_new_override === undefined) {
                                node.parameters._guarded_was_new_override = deepClone(wasNew)
                            }
                        } catch(_) { /* non-fatal */ }
                    }
                }
                // Recurse children with canonical ordinal-aware path segments
                if (Array.isArray(node.children)) {
                    for (let i = 0; i < node.children.length; i++) {
                        const ch = node.children[i]
                        if (!ch || typeof ch !== 'object') continue
                        const base = ch.name || ch.node_type || ch.type || 'child'
                        const ord = node.children.slice(0, i).filter(c => c && (c.name || c.node_type || c.type) === base).length
                        const seg = ord > 0 ? `${base}#${ord}` : base
                        walk(ch, node, pathArr.concat([seg]))
                    }
                }
            }
            // Root(s)
            const rootsNew = Array.isArray(newDoc) ? newDoc : [newDoc]
            for (let i = 0; i < rootsNew.length; i++) {
                const r = rootsNew[i]
                if (!r) continue
                walk(r, null, [r.name || r.node_type || r.type || 'root'])
            }
        } catch(_) { /* best-effort tagging */ }
    }

    // Resolve effective mutability mode for a node: 'true' | 'false' | 'guarded'
    // Parameter forms supported: boolean true/false, string 'true'|'false'|'inherited'|'guarded'
    // Inheritance walks up parent chain using node.__overseer_path or nearest DOM data-path.
    _parseMutableMode(v) {
        if (v === null || v === undefined) return 'inherited'
        if (typeof v === 'boolean') return v ? 'true' : 'false'
        const s = String(v).toLowerCase().trim()
        if (s === 'true') return 'true'
        if (s === 'false') return 'false'
        if (s === 'guarded') return 'guarded'
        if (s === 'inherit' || s === 'inherited') return 'inherited'
        return 'inherited'
    }

    _readMutableParam(n) {
        try {
            // Prefer computed if available, else raw
            const comp = this.getParameterValue(n, '_computed_mutable')
            if (comp !== null && comp !== undefined) return comp
        } catch(_) { /* ignore */ }
        try { return this.getParameterValue(n, 'mutable') } catch(_) { return null }
    }

    /**
     * Closest explicit `mutable` declared on a DOM ancestor that carries a data-path.
     *
     * A link proxy renders its target's subtree inside itself, so the field's canonical
     * path bypasses the proxy entirely — a `mutable` on the proxy would otherwise never
     * apply to the fields it displays, even though that container is exactly what the
     * user sees the field inside. Returns null when nothing explicit is found.
     */
    _mutableFromDisplayChain(elementHint, doc) {
        try {
            if (!elementHint || !doc) return null
            let el = elementHint.parentElement
            let hops = 0
            while (el && hops < 64) {
                hops++
                const raw = el.dataset ? el.dataset.path : null
                if (raw) {
                    let p = null
                    try { p = JSON.parse(raw) } catch(_) { p = null }
                    if (Array.isArray(p) && p.length > 0) {
                        const n = this.findNodeByPath(doc, p)
                        if (n) {
                            const mode = this._parseMutableMode(this._readMutableParam(n))
                            if (mode !== 'inherited') return mode
                        }
                    }
                }
                el = el.parentElement
            }
        } catch(_) { /* ignore */ }
        return null
    }

    getEffectiveMutableMode(node, elementHint = null) {
        const parseMode = (v) => this._parseMutableMode(v)
        const readMutableParam = (n) => this._readMutableParam(n)
        // 1) If the node itself declares an explicit mode, honor it immediately (works for phantom previews as well)
        try {
            const selfRaw = readMutableParam(node)
            const selfMode = parseMode(selfRaw)
            if (selfMode !== 'inherited') return selfMode
        } catch(_) { /* ignore */ }
        // Resolve the path to walk ancestors
        let pathArr = []
        try {
            if (node && Array.isArray(node.__overseer_path)) pathArr = node.__overseer_path.slice()
        } catch(_) {}
        if ((!pathArr || pathArr.length === 0) && elementHint && elementHint.dataset && elementHint.dataset.path) {
            try { pathArr = JSON.parse(elementHint.dataset.path) } catch(_) { pathArr = [] }
        }
        // Walk from node up to root looking for explicit mode
        try {
            const doc = window?.app?.currentDocument
            if (!Array.isArray(pathArr) || pathArr.length === 0 || !doc) {
                // Without a resolvable path the displayed chain is the only signal available.
                const displayMode = this._mutableFromDisplayChain(elementHint, doc)
                if (displayMode) return displayMode
                // Fallback to default
                return this._defaultTopLevelMutable ? 'true' : 'false'
            }
            // Build ancestor chain of nodes (from deepest to root)
            const ancestors = []
            for (let i = pathArr.length; i >= 1; i--) {
                const sub = pathArr.slice(0, i)
                const n = this.findNodeByPath(doc, sub)
                if (n) ancestors.push(n)
            }
            // First explicit non-inherited wins (closest ancestor first)
            for (const anc of ancestors) {
                const raw = readMutableParam(anc)
                const mode = parseMode(raw)
                if (mode !== 'inherited') return mode
            }
            // Nothing explicit along the canonical chain. Fall back to the chain the field
            // is actually displayed in, which for a link proxy includes the container the
            // canonical path bypasses.
            const displayMode = this._mutableFromDisplayChain(elementHint, doc)
            if (displayMode) return displayMode
            // No explicit setting found: default at top-level
            return this._defaultTopLevelMutable ? 'true' : 'false'
        } catch(_) {
            return this._defaultTopLevelMutable ? 'true' : 'false'
        }
    }

    // Build a non-persistent preview item based on list's entry template, with key preset.
    _buildPhantomItemPreview(documentArray, listNode, templateName, keyField, keyValue) {
        try {
            // Find template definition by name at document root
            const roots = Array.isArray(documentArray) ? documentArray : []
            let tmpl = null
            if (templateName) {
                // Deep-search for a node with matching name anywhere in the document (DFS)
                const findByName = (nodes, name) => {
                    if (!Array.isArray(nodes)) return null
                    for (const n of nodes) {
                        if (!n) continue
                        if (n.name === name) return n
                        const found = findByName(n.children || [], name)
                        if (found) return found
                    }
                    return null
                }
                tmpl = findByName(roots, templateName) || null
            }
            // If no template found, create a minimal generic container
            const preview = tmpl ? JSON.parse(JSON.stringify(tmpl)) : { name: templateName || 'Item', node_type: 'div', parameters: {}, children: [] }
            // Ensure key field child exists and has the keyValue
            if (!preview.children) preview.children = []
            let keyChild = preview.children.find(c => c && c.name === keyField)
            if (!keyChild) {
                keyChild = { name: keyField, node_type: 'string', parameters: {}, children: [] }
                preview.children.unshift(keyChild)
            }
            if (!keyChild.parameters) keyChild.parameters = {}
            // Coerce key value to match key field type when possible
            const ty = String(keyChild.node_type || keyChild.type || '').toLowerCase()
            const toDate = (s) => {
                const str = String(s || '')
                // Accept YYYY-MM-DD, YYYY/MM/DD, YYYY.MM.DD or RFC3339
                const m = str.match(/^(\d{4})[./-](\d{2})[./-](\d{2})(?:.*)?$/)
                if (m) return `${m[1]}-${m[2]}-${m[3]}`
                const d = new Date(str); if (!isNaN(d.getTime())) {
                    const pad = (n) => String(n).padStart(2, '0')
                    return `${d.getFullYear()}-${pad(d.getMonth()+1)}-${pad(d.getDate())}`
                }
                return String(str)
            }
            const toTs = (s) => {
                const str = String(s || '')
                // If already RFC3339-like keep it, else assume day precision start of day UTC
                if (/^\d{4}-\d{2}-\d{2}T/.test(str)) return str
                const d = toDate(str)
                return `${d}T00:00:00Z`
            }
            const toNum = (s, f=false) => {
                const n = f ? parseFloat(String(s)) : parseInt(String(s), 10)
                return isNaN(n) ? null : n
            }
            let coerced
            switch (ty) {
                case 'date':
                    // Store as String normalized to YYYY-MM-DD so tests and existing data match
                    coerced = { String: toDate(keyValue) }
                    break
                case 'timestamp':
                    // Store as String day-precision (YYYY-MM-DD) for consistency with existing documents
                    coerced = { String: toDate(keyValue) }
                    break
                case 'int':
                case 'integer': {
                    const n = toNum(keyValue, false)
                    coerced = n === null ? { String: String(keyValue) } : { Integer: n }
                    break
                }
                case 'float': {
                    const n = toNum(keyValue, true)
                    coerced = n === null ? { String: String(keyValue) } : { Float: n }
                    break
                }
                case 'bool':
                case 'boolean': {
                    const s = String(keyValue).toLowerCase()
                    if (s === 'true' || s === 'false') coerced = { Boolean: s === 'true' }
                    else coerced = { String: String(keyValue) }
                    break
                }
                default:
                    coerced = (typeof keyValue === 'number') ? { Integer: keyValue } : { String: String(keyValue) }
            }
            // Assign coerced value and also override any stale computed value coming from the template
            // so the preview displays the selected key immediately (even if template had $today()).
            keyChild.parameters.value = coerced
            try { keyChild.parameters._computed_value = coerced } catch(_) {}
            // Mark as explicit so when materialized later, the key is persisted
            try { keyChild.parameters._override_present = { Boolean: true } } catch(_) {}
            // Reflect that this is a template-derived instance for styling/layout if needed
            preview.parameters = Object.assign({}, preview.parameters || {}, { _from_template: true })

            // Clean any stale computed shadows copied from the template to avoid misleading values in preview
            const scrubComputed = (node) => {
                if (!node || typeof node !== 'object') return
                if (node.parameters && typeof node.parameters === 'object') {
                    try { if ('_computed_value' in node.parameters) delete node.parameters._computed_value } catch(_) {}
                    try { if ('_computed_fallback' in node.parameters) delete node.parameters._computed_fallback } catch(_) {}
                }
                const ch = Array.isArray(node.children) ? node.children : []
                for (const c of ch) scrubComputed(c)
            }
            scrubComputed(preview)

            // Best-effort compute a preview fallback for common patterns like weight-from-prior-history.
            // Only for phantom preview, using local document array and the provided listNode context.
            try {
                const normDay = (s) => {
                    const str = String(s || '')
                    const m = str.match(/^(\d{4})[./-](\d{2})[./-](\d{2})/)
                    if (m) return `${m[1]}-${m[2]}-${m[3]}`
                    const d = new Date(str); if (!isNaN(d.getTime())) {
                        const pad = (n) => String(n).padStart(2, '0')
                        return `${d.getFullYear()}-${pad(d.getMonth()+1)}-${pad(d.getDate())}`
                    }
                    return str
                }
                const readFieldNode = (parent, name) => (parent && Array.isArray(parent.children)) ? parent.children.find(c => c && c.name === name) : null
                const readStringOrNumber = (valObj) => {
                    if (valObj === null || valObj === undefined) return null
                    if (typeof valObj === 'number') return valObj
                    if (typeof valObj === 'string') return valObj
                    if (typeof valObj === 'object') {
                        if ('String' in valObj) return valObj.String
                        if ('Float' in valObj) return valObj.Float
                        if ('Integer' in valObj) return valObj.Integer
                        if ('Timestamp' in valObj) return valObj.Timestamp
                        if ('Date' in valObj) return valObj.Date
                    }
                    return null
                }
                const readEffectiveParam = (node, key) => {
                    if (!node || !node.parameters) return null
                    const shadow = node.parameters[`_computed_${key}`]
                    if (shadow !== undefined) return shadow
                    const raw = node.parameters[key]
                    return raw
                }
                // Only attempt for a child named 'weight' (float) when value is missing/empty or explicitly Null
                const weightChild = readFieldNode(preview, 'weight')
                const weightVal = weightChild?.parameters?.value
                const weightMissing = (weightVal === undefined) || weightVal === null ||
                    (typeof weightVal === 'object' && weightVal !== null && ('Null' in weightVal)) ||
                    (typeof weightVal === 'string' && weightVal.trim().toLowerCase() === 'null')
                if (weightChild && weightMissing && listNode && Array.isArray(listNode.children)) {
                    const targetDay = normDay(readStringOrNumber(coerced) || keyValue)
                    const candidates = []
                    for (const item of listNode.children) {
                        const dNode = readFieldNode(item, keyField)
                        const dVal = readStringOrNumber(readEffectiveParam(dNode, 'value'))
                        const day = normDay(dVal)
                        if (!day || !/\d{4}-\d{2}-\d{2}/.test(day)) continue
                        if (day < targetDay) candidates.push({ day, item })
                    }
                    candidates.sort((a, b) => a.day < b.day ? 1 : (a.day > b.day ? -1 : 0))
                    let prev = candidates.length ? candidates[0].item : null
                    let prevWeight = null
                    if (prev) {
                        const wNode = readFieldNode(prev, 'weight')
                        const wVal = readEffectiveParam(wNode, 'value')
                        const n = readStringOrNumber(wVal)
                        prevWeight = typeof n === 'string' ? parseFloat(n) : n
                    }
                    if (typeof prevWeight !== 'number' || isNaN(prevWeight)) prevWeight = 80.0
                    // Surface as computed fallback so UI shows it when value is null
                    if (!weightChild.parameters) weightChild.parameters = {}
                    // Surface both computed fallback and computed value to ensure display shows the number in preview
                    weightChild.parameters._computed_fallback = { Float: prevWeight }
                    weightChild.parameters._computed_value = { Float: prevWeight }
                }

                // Compute simple totals for empty intake lists: total_calories = 0 when intake has no items
                try {
                    const intakeNode = readFieldNode(preview, 'intake')
                    const intakeEmpty = !intakeNode || !Array.isArray(intakeNode.children) || intakeNode.children.length === 0
                    if (intakeEmpty) {
                        const totalNode = readFieldNode(preview, 'total_calories')
                        if (totalNode) {
                            if (!totalNode.parameters) totalNode.parameters = {}
                            if (totalNode.parameters._computed_value === undefined) {
                                totalNode.parameters._computed_value = { Integer: 0 }
                            }
                        }
                    }
                } catch(_) { /* non-fatal */ }
            } catch(_) { /* non-fatal */ }
            return preview
        } catch(_) { return { name: templateName || 'Item', node_type: 'div', parameters: {}, children: [] } }
    }

    // Materialize missing list item by calling ensure_in_list via event executor, update the link param, and return a concrete field path for the edited element.
    // Options:
    //  - position: 'append' | 'prepend' (default: 'append')
    async _materializePhantomAndComputePath(meta, options = {}) {
        try {
            if (!window.app || !window.app.currentDocument) return null
            // meta.listPath is path array to the list node
            const listPathArr = Array.isArray(meta.listPath) ? meta.listPath : []
            const listPathStr = listPathArr.join('/')
            const ownerEl = document.querySelector(`[data-path='${JSON.stringify(listPathArr)}']`)
            const ownerNode = ownerEl ? null : null // unused; actions are path-based
            // Build a synthetic owner path: use the list container path for event context
            const eventContextPath = listPathArr
            // Execute ensure_in_list by invoking backend through a tiny synthetic action: we piggyback on execute_overseer_event with an injected action is complex,
            // instead, reuse the existing command interface by creating a minimal action block would require serialization changes.
            // Simpler: directly mutate currentDocument here to append, using list entry template name.
            const doc = window.app.currentDocument
            // Locate list node in document by path
            const listNode = this.findNodeByPath(doc, listPathArr)
            if (!listNode || (listNode.node_type || '').toLowerCase() !== 'list') return null
            // Determine effective key field
            let keyField = meta.keyField
            if (!keyField) {
                const k = listNode.parameters?.key || listNode.parameters?._computed_key
                keyField = (typeof k === 'string') ? k : (k && k.String !== undefined ? String(k.String) : 'id')
            }
            // Find template
            let tmplName = meta.templateName
            if (!tmplName) {
                const entry = listNode.parameters?.entry
                if (entry) {
                    if (typeof entry === 'string') tmplName = entry
                    else if (typeof entry === 'object' && entry.Template !== undefined) tmplName = String(entry.Template)
                    else if (typeof entry === 'object' && entry.String !== undefined) tmplName = String(entry.String)
                }
            }
            if (tmplName && tmplName.startsWith('<') && tmplName.endsWith('>')) tmplName = tmplName.slice(1, -1)
            // Clone template (deep-search by name anywhere in the document)
            const roots = doc
            const findByNameDeep = (nodes, name) => {
                if (!Array.isArray(nodes)) return null
                for (const n of nodes) {
                    if (!n) continue
                    if (n.name === name) return n
                    const found = findByNameDeep(n.children || [], name)
                    if (found) return found
                }
                return null
            }
            const tmpl = tmplName ? findByNameDeep(roots, tmplName) : null
            let newItem = tmpl ? JSON.parse(JSON.stringify(tmpl)) : { name: tmplName || 'Item', node_type: 'div', parameters: {}, children: [] }
            // The clone must not inherit the template's provenance. `source_id` points at the
            // template's own source text, and the serializer replays that text verbatim for
            // any node still carrying one - so a materialized entry would be written out as a
            // full copy of the template, comments and all, instead of the handful of fields
            // that actually differ. Without the ids it is treated as a fresh template
            // instance, and only genuine overrides are written.
            const stripTemplateProvenance = (n) => {
                if (!n || typeof n !== 'object') return
                delete n.source_id
                delete n.source_fingerprint
                delete n.source_snapshot
                if (Array.isArray(n.children)) n.children.forEach(stripTemplateProvenance)
            }
            stripTemplateProvenance(newItem)
            // Assign a stable UID to the new item (used for DOM mapping independent of name/position)
            try {
                const uid = `uid_${Date.now().toString(36)}_${Math.random().toString(36).slice(2)}`
                if (!newItem.parameters) newItem.parameters = {}
                // Store in parameters to persist through rerenders (not serialized if we prefix underscore)
                newItem.parameters._uid = { String: uid }
                newItem.__uid = uid
                if (!this._uidToNode) this._uidToNode = new Map()
                this._uidToNode.set(uid, newItem)
                if (DEBUG_MODE) console.debug('[Overseer] uid assign (materialize)', uid, newItem.name)
            } catch(_) { /* best-effort */ }
            // Ensure required schema fields exist on the new item
            if (newItem.is_hierarchy_transparent === undefined) newItem.is_hierarchy_transparent = (tmpl && typeof tmpl.is_hierarchy_transparent === 'boolean') ? tmpl.is_hierarchy_transparent : false
            // Mark as originating from a template to help selective UI rerenders detect templated instances
            try { newItem.parameters = Object.assign({}, newItem.parameters || {}, { _from_template: true }) } catch (_) {}
            // Assign a unique instance name similar to backend logic (T__N), avoiding collisions by scanning siblings
            const baseNameRaw = (tmplName || newItem.name || 'Item')
            const baseName = String(baseNameRaw).replace(/__\d+$/, '')
            const usedSuffixes = new Set()
            for (const sib of (Array.isArray(listNode.children) ? listNode.children : [])) {
                const nm = sib && sib.name ? String(sib.name) : ''
                if (nm === baseName) { usedSuffixes.add(1); continue }
                const m = nm.startsWith(baseName + '__') ? nm.slice(baseName.length + 2).match(/^(\d+)$/) : null
                if (m) {
                    const n = parseInt(m[1], 10)
                    if (!isNaN(n)) usedSuffixes.add(n)
                }
            }
            let nextN = 1
            if (usedSuffixes.size > 0) {
                let max = 0
                for (const n of usedSuffixes) if (n > max) max = n
                nextN = max + 1
            }
            newItem.name = `${baseName}__${nextN}`
            
            // Set key field value
            if (!newItem.children) newItem.children = []
            let keyChild = newItem.children.find(c => c && c.name === keyField)
            if (!keyChild) { keyChild = { name: keyField, node_type: 'string', parameters: {}, children: [] }; newItem.children.unshift(keyChild) }
            if (keyChild.is_hierarchy_transparent === undefined) keyChild.is_hierarchy_transparent = false
            if (!keyChild.parameters) keyChild.parameters = {}
            const kv = meta.keyValue
            // Match key value type to the field's node type where possible
            const keyTy = String(keyChild.node_type || keyChild.type || '').toLowerCase()
            const toDate2 = (s) => {
                const str = String(s || '')
                const m = str.match(/^(\d{4})[./-](\d{2})[./-](\d{2})(?:.*)?$/)
                if (m) return `${m[1]}-${m[2]}-${m[3]}`
                const d = new Date(str); if (!isNaN(d.getTime())) {
                    const pad = (n) => String(n).padStart(2, '0')
                    return `${d.getFullYear()}-${pad(d.getMonth()+1)}-${pad(d.getDate())}`
                }
                return String(str)
            }
            const toTs2 = (s) => {
                const str = String(s || '')
                if (/^\d{4}-\d{2}-\d{2}T/.test(str)) return str
                const d = toDate2(str)
                return `${d}T00:00:00Z`
            }
            const numParse = (s, f=false) => {
                const n = f ? parseFloat(String(s)) : parseInt(String(s), 10)
                return isNaN(n) ? null : n
            }
            let kvTyped
            switch (keyTy) {
                case 'date': kvTyped = { String: toDate2(kv) }; break
                case 'timestamp': kvTyped = { String: toDate2(kv) }; break
                case 'int':
                case 'integer': { const n = numParse(kv, false); kvTyped = (n===null)?{ String: String(kv) }:{ Integer: n }; break }
                case 'float': { const n = numParse(kv, true); kvTyped = (n===null)?{ String: String(kv) }:{ Float: n }; break }
                case 'bool':
                case 'boolean': { const s = String(kv).toLowerCase(); kvTyped = (s==='true'||s==='false')?{ Boolean: s==='true' }:{ String: String(kv) }; break }
                default: kvTyped = (typeof kv === 'number') ? { Integer: kv } : { String: String(kv) }
            }
            // Assign the key and mirror it in _computed_value to avoid template-computed fallbacks overriding display
            keyChild.parameters.value = kvTyped
            try { keyChild.parameters._computed_value = kvTyped } catch(_) {}
            // Ensure the key field persists on serialization as an explicit override
            try { keyChild.parameters._override_present = { Boolean: true } } catch(_) {}
            // Write it in the same dash form authored entries use.
            try { keyChild.authored_dash = true } catch(_) {}
            // Mark the entry as one built by cloning a template, so the serializer records
            // only what distinguishes it. The clone has to carry the template's full
            // structure - the click that materialized it addresses a button inside the
            // entry, and that path has to resolve - but none of that structure belongs on
            // disk, where the template supplies it.
            try { newItem.parameters._materialized_from_template = { Boolean: true } } catch(_) {}
            // Insert into list honoring requested position
            const pos = (options && typeof options.position === 'string') ? options.position.toLowerCase() : 'append'
            if (pos === 'prepend') {
                // Prior to inserting at the front, freeze existing sibling weight values so reevaluation does not shift them.
                try {
                    let frozenCount = 0
                    for (const sib of (Array.isArray(listNode.children) ? listNode.children : [])) {
                        if (!sib || !Array.isArray(sib.children)) continue
                        const weightChild = sib.children.find(c => c && c.name === 'weight')
                        if (!weightChild) continue
                        if (!weightChild.parameters) weightChild.parameters = {}
                        const hasExplicit = weightChild.parameters.value !== undefined
                        if (hasExplicit) continue // already explicit, skip
                        // Prefer an existing computed value; fall back to fallback; else skip
                        const cv = weightChild.parameters._computed_value || weightChild.parameters._computed_fallback || null
                        if (!cv || typeof cv !== 'object') continue
                        // Mirror value structure exactly (Float/Integer/String/etc.)
                        try {
                            weightChild.parameters.value = JSON.parse(JSON.stringify(cv))
                            // Mark override so serializer persists it
                            weightChild.parameters._override_present = { Boolean: true }
                            frozenCount++
                        } catch(_) { /* ignore */ }
                    }
                    if (frozenCount > 0) {
                        
                    } else {
                        
                    }
                } catch(_) { /* best-effort */ }
                listNode.children.unshift(newItem)
            } else {
                listNode.children.push(newItem)
            }
            // Mark explicit override so it persists
            listNode.parameters = Object.assign({}, listNode.parameters || {}, { _explicit_overrides: { String: (listNode.parameters?._explicit_overrides?.String || '') } })
            // Do not mutate the original link; keep it dynamic so it can follow future date changes.
            // Return a real field path if the edit targeted a child in tailSegments; otherwise the item path
            // Use the actual item name (with instance suffix) for correct path resolution
            const siblings = listNode.children
            // Use the exact instance name (which already contains __N) to avoid ambiguity; still include
            // an ordinal when multiple siblings coincidentally share the same exact name (extremely rare given unique suffix selection above).
            const idxNew = siblings.indexOf(newItem)
            const itemExactName = newItem.name // e.g., WeightRecord__19
            const itemOrd = siblings.slice(0, idxNew).filter(c => c && c.name === itemExactName).length
            const itemSeg = itemOrd > 0 ? `${itemExactName}#${itemOrd}` : itemExactName
            let realPathArr = listPathArr.concat([itemSeg])
            // Traverse tail segments directly on the newly created item, creating missing fields on demand,
            // and construct canonical path segments using the actual picked names and true ordinal among siblings.
            const parseSeg = (seg) => {
                const i = typeof seg === 'string' ? seg.lastIndexOf('#') : -1
                return i > 0 ? { base: seg.slice(0, i), ord: parseInt(seg.slice(i + 1), 10) || 0 } : { base: String(seg), ord: 0 }
            }
            const normalize = (s) => String(s || '').replace(/__\d+$/, '')
            let curRef = newItem
            const tailSegs = Array.isArray(meta.tailSegments) ? meta.tailSegments.slice() : []
            for (let tIdx = 0; tIdx < tailSegs.length; tIdx++) {
                const t = tailSegs[tIdx]
                const { base, ord } = parseSeg(t)
                let kidsNow = Array.isArray(curRef.children) ? curRef.children : []
                // Prefer exact name match first, then normalized name match
                let candidates = kidsNow.filter(n => n && n.name === base)
                if (candidates.length === 0) {
                    candidates = kidsNow.filter(n => n && normalize(n.name) === base)
                }
                let pick = candidates[ord] || candidates[0]
                if (!pick) {
                    // Create the missing child; assume leaf is string, otherwise a transparent container
                    const isLast = (tIdx === tailSegs.length - 1)
                    pick = { name: base, node_type: isLast ? 'string' : 'div', parameters: {}, children: [], is_hierarchy_transparent: false }
                    curRef.children = Array.isArray(curRef.children) ? curRef.children : []
                    curRef.children.push(pick)
                    kidsNow = curRef.children
                }
                const idxPick = kidsNow.indexOf(pick)
                const ordPick = idxPick > 0 ? kidsNow.slice(0, idxPick).filter(n => n && n.name === pick.name).length : 0
                const segName = ordPick > 0 ? `${pick.name}#${ordPick}` : pick.name
                realPathArr = realPathArr.concat([segName])
                curRef = pick
            }
            const __finalPath = realPathArr.join('/')
            // Record a direct reference to the newly created leaf target so later edits update exactly this node
            try {
                // Attach diagnostic markers for identity tracking
                const materializeId = `mat_${Date.now()}_${Math.random().toString(36).slice(2)}`
                try { newItem.__materialize_id = materializeId } catch(_) {}
                try { curRef.__materialize_id = materializeId + '_leaf' } catch(_) {}
                // If this appears to be a WeightRecord template with a 'weight' child, ensure it has an explicit starting value so backend reevaluation doesn't cascade-shift others.
                try {
                    const isWeightRecord = /weightrecord/i.test(newItem.name || '')
                    if (isWeightRecord) {
                        const wLeaf = (newItem.children||[]).find(c => c && c.name === 'weight')
                        if (wLeaf) {
                            if (!wLeaf.parameters) wLeaf.parameters = {}
                            const existingExplicit = wLeaf.parameters.value
                            if (existingExplicit === undefined) {
                                const baseVal = wLeaf.parameters._computed_value || wLeaf.parameters._computed_fallback
                                if (baseVal && typeof baseVal === 'object') {
                                    try { wLeaf.parameters.value = JSON.parse(JSON.stringify(baseVal)) } catch(_) {}
                                    wLeaf.parameters._override_present = { Boolean: true }
                                    
                                }
                            }
                        }
                    }
                } catch(_) { /* best-effort */ }
                this._materializedTargets.set(__finalPath, { itemNode: newItem, leafNode: curRef, ts: Date.now(), materializeId, position: pos })
                
            } catch(_) { /* best-effort */ }
            if (DEBUG_MODE) console.debug('[Overseer] _materializePhantomAndComputePath summary', {
                list: listPathArr.join('/'),
                template: tmplName,
                newItemName: newItem && newItem.name,
                keyField,
                keyValue: kvTyped,
                position: pos,
                finalPath: __finalPath
            })
            try {
                const ordering = (listNode.children||[]).map((c,i)=>({ idx:i, name:c && c.name }))
                
            } catch(_) {}
            return __finalPath
        } catch(_) { return null }
    }

    // Interval management: avoid per-element MutationObservers by clearing on re-render
    _registerInterval(id) {
        try { this._liveIntervals.add(id) } catch(_) {}
        return id
    }
    _clearAllIntervals() {
        try {
            for (const id of this._liveIntervals) {
                clearInterval(id)
            }
            this._liveIntervals.clear()
        } catch(_) {}
    }

    // Non-visual node helpers
    isEventHandlerName(name) {
        if (!name) return false
        const n = String(name).toLowerCase()
        // Common UI events supported by Overseer actions
        const eventNames = [
            'click','change','timeout','submit','dblclick','hover','keydown','keyup','input','tick'
        ]
        return eventNames.includes(n)
    }

    isActionName(name) {
        if (!name) return false
        const n = String(name).toLowerCase()
        // Action nodes are not visual; keep this list in sync with backend
        const actionNames = [
            'set','inc','dec','toggle','clear','clear_list','ensure_in_list','ensure','remove','append','move','sort','set_now','set_now_ts','activate','deactivate'
        ]
        return actionNames.includes(n)
    }

    /// Say how many of a list's entries were left out of view.
    ///
    /// A windowed list draws three of forty-three, and without a word about it the other forty
    /// look deleted. The count comes from the resolver, which is the only thing that knows: the
    /// entries are in the document but nothing was worked out for them, so counting the rows on
    /// screen would not find them.
    drawEntriesLeftOut(element, node) {
        try {
            element.querySelector(':scope > .overseer-entries-left-out')?.remove()
            const raw = node?.parameters?.['_left_out_of_view']
            const left = (raw && typeof raw === 'object') ? (raw.Integer ?? raw.Float) : raw
            const count = Number(left)
            if (!Number.isFinite(count) || count <= 0) return
            const note = document.createElement('div')
            note.className = 'overseer-entries-left-out'
            note.textContent = count === 1 ? '1 earlier entry not shown' : `${count} earlier entries not shown`
            element.appendChild(note)
        } catch (_) { /* a missing note is not worth failing a render over */ }
    }

    shouldRenderChild(parentNode, childNode) {
        const parentType = (parentNode?.node_type || parentNode?.type || '').toLowerCase()
        const childName = childNode?.name
        const childType = (childNode?.node_type || childNode?.type || '').toLowerCase()

    // Timer nodes are visual now; don't filter them out
        // Hide any event handler containers and action statements anywhere
        if (this.isEventHandlerName(childName) || this.isActionName(childName)) return false
        
        // Hide plot nodes - they are configuration for chart nodes, not visual elements
        if (childType === 'plot') return false

        // A list entry the resolver left out of view. Not merely hidden: nothing was worked out
        // for it and no template was copied onto it, so there is nothing here to draw - such a
        // row comes out as unformatted text showing raw handles, which is exactly what it is.
        // How many were left out is said once, under the list - see `drawEntriesLeftOut`.
        //
        // Read through `getParameterValue` because a parameter arrives tagged by its type -
        // `{ Boolean: true }`, not `true`. Comparing against the raw value silently never matched.
        try {
            const left = this.getParameterValue(childNode, '_out_of_view')
            if (left === true || String(left).toLowerCase() === 'true') return false
        } catch (_) { /* a node without parameters is not out of view */ }
        
        // Respect hidden=true on child nodes
        try {
            const hid = this.getParameterValue(childNode, 'hidden')
            if (hid === true || String(hid).toLowerCase() === 'true') return false
        } catch(_) {}
        // For buttons specifically, do not render any children other than explicit visual content (none today)
        if (parentType === 'button') return false
        return true
    }

    /// Let charts animate again, for when a different document is opened.
    resetChartAnimations() {
        this._animatedCharts = new Set()
    }

    // Link proxies whose target has moved.
    //
    // A proxy declares (link="/History[key=$(../selected_date)]") and resolves that at render
    // time, so changing the date moves what it shows without changing the proxy node at all.
    // The backend has nothing to report for it - the link is resolved here, not there - so a
    // described change would repaint the date and leave the day's contents showing yesterday.
    // Re-deriving costs one pass over a handful of proxies, against a full render otherwise.
    linkProxiesThatMoved(doc) {
        const moved = []
        const rawLink = (p) => {
            if (!p || p.link === undefined) return null
            const v = p.link
            return (v && typeof v === 'object' && v.String !== undefined) ? v.String : v
        }
        const visit = (nodes) => {
            for (const node of nodes || []) {
                if (!node || typeof node !== 'object') continue
                const p = node.parameters || {}
                const link = rawLink(p)
                if (link !== null && link !== undefined && Array.isArray(node.__overseer_path)) {
                    try {
                        const { computedLink } = this.resolveLinkTarget(link, node.__overseer_path)
                        const previous = p._computed_link && p._computed_link.String
                        if (computedLink !== undefined && String(computedLink) !== String(previous)) {
                            moved.push(node)
                        }
                    } catch (_) { /* an unresolvable link is not a moved one */ }
                }
                if (Array.isArray(node.children)) visit(node.children)
            }
        }
        visit(Array.isArray(doc) ? doc : [doc])
        return moved
    }

    // Repaint just these nodes, rather than rebuilding the document around them.
    //
    // Rendering a large document measured about two seconds for 15,000 elements, and a field
    // edit changes a couple of them. A node remembers where it was rendered, so the subtree
    // can be replaced in place. A node that has never been rendered - or one whose place is
    // unknown - means falling back to a full render, which is correct but is the thing being
    // avoided, so it is worth knowing when it happens.
    repaintNodes(nodes, document_) {
        const doc = document_ || (window.app && window.app.currentDocument)
        if (!doc || !Array.isArray(nodes) || nodes.length === 0) return false
        // Whatever the document says changed, plus anything the client derives from it that
        // has moved as a result.
        const all = nodes.slice()
        try {
            for (const proxy of this.linkProxiesThatMoved(doc)) {
                if (!all.includes(proxy)) all.push(proxy)
            }
        } catch (_) { /* non-fatal */ }
        for (const node of all) {
            const path = node && node.__overseer_path
            // Reported under the profiling flag rather than the debug one: falling back to a
            // full render is the cost this exists to avoid, so it should be visible to whoever
            // is measuring, and the debug flag needs a query string the desktop app has no way
            // to set.
            if (!Array.isArray(path) || path.length === 0) {
                if (PROFILE) console.warn(`[profile] FULL RENDER: '${node && node.name}' has no rendered position`)
                this.renderDocument(doc)
                return false
            }
            if (!this.rerenderSubtree(doc, path)) {
                if (PROFILE) console.warn(`[profile] FULL RENDER: '${node && node.name}' is not on screen at ${JSON.stringify(path)}`)
                this.renderDocument(doc)
                return false
            }
        }
        return true
    }

    renderDocument(overseerDocument) {
        const rendered = profiled('  renderDocument', () => this._renderDocumentProfiled(overseerDocument))
        // Straight away, rather than leaving it to the filter element's own timeout: a full
        // render would otherwise show every entry for a frame before hiding most of them again.
        try { this.applyFilters() } catch (_) { /* a filter must never break a render */ }
        return rendered
    }
    _renderDocumentProfiled(overseerDocument) {
        if (DEBUG_MODE) {
            try {
                console.log('Rendering document:', overseerDocument)
                console.log('Document type:', typeof overseerDocument)
                console.log('Document is array:', Array.isArray(overseerDocument))
                console.log('Document length:', overseerDocument?.length)
            } catch(_) {}
        }

        // Clear previous content
    // Also clear any active intervals from previous render to prevent leaks
    this._clearAllIntervals()
        // Clean up DOM-attached resources (charts, observers) from previous render
        try {
            const cleanupNode = (el) => {
                if (!el || typeof el !== 'object') return
                // Chart.js instance cleanup
                if (el._chartInstance && typeof el._chartInstance.destroy === 'function') {
                    try { el._chartInstance.destroy() } catch(_) {}
                    el._chartInstance = null
                }
                // ResizeObserver cleanup
                if (el._resizeObserver && typeof el._resizeObserver.disconnect === 'function') {
                    try { el._resizeObserver.disconnect() } catch(_) {}
                    el._resizeObserver = null
                }
                // Recurse into children
                if (el.children && el.children.length) {
                    for (const child of Array.from(el.children)) cleanupNode(child)
                }
            }
            cleanupNode(this.contentDisplay)
        } catch(_) {}
        // Defensive guards: in headless test environments elements can be null
        if (this.contentDisplay) {
            try { this.contentDisplay.innerHTML = '' } catch(_) {}
        }
        if (this.tabContainer) {
            try { this.tabContainer.innerHTML = '' } catch(_) {}
        }

        // Optional lightweight debug info (avoid dumping full document JSON)
        if (DEBUG_MODE) {
            const debugInfo = document.createElement('div')
            debugInfo.style.cssText = 'background: #f0f0f0; padding: 6px 10px; margin: 8px; border: 1px solid #ccc; font-family: monospace; color: #000;'
            const rootCount = Array.isArray(overseerDocument) ? overseerDocument.length : 1
            debugInfo.textContent = `DEBUG: roots=${rootCount} type=${typeof overseerDocument}`
            if (this.contentDisplay) this.contentDisplay.appendChild(debugInfo)
        }

        if (!overseerDocument) {
            console.error('Document is null or undefined')
            if (this.contentDisplay) this.contentDisplay.innerHTML += '<p>No document provided</p>'
            return
        }

        if (Array.isArray(overseerDocument)) {
            if (DEBUG_MODE) console.log('Processing array document with', overseerDocument.length, 'nodes')

            if (overseerDocument.length === 0) {
                if (this.contentDisplay) this.contentDisplay.innerHTML += '<p>Document is empty (no nodes parsed)</p>'
                return
            }

            // Document is an array of root nodes
            for (let i = 0; i < overseerDocument.length; i++) {
                if (DEBUG_MODE) console.log(`Rendering node ${i}:`, overseerDocument[i])
                this.renderNode(overseerDocument[i], this.contentDisplay, {}, [overseerDocument[i].name || overseerDocument[i].node_type || overseerDocument[i].type || `root_${i}`])
            }
        } else if (overseerDocument && typeof overseerDocument === 'object') {
            if (DEBUG_MODE) console.log('Processing single root node:', overseerDocument)
            // Single root node
            this.renderNode(overseerDocument, this.contentDisplay, {}, [overseerDocument.name || overseerDocument.node_type || overseerDocument.type || 'root'])
        } else {
            if (DEBUG_MODE) console.warn('Unexpected document format:', overseerDocument)
            this.contentDisplay.innerHTML += '<p>Unexpected document format</p>'
        }

    if (DEBUG_MODE) console.log('Content display after rendering:', this.contentDisplay.innerHTML)
    }

    renderNode(node, container, inheritedStyles = {}, path = []) {
    if (DEBUG_MODE) console.log('renderNode called with:', node, 'container:', container)

        if (!node || typeof node !== 'object') {
            if (DEBUG_MODE) console.warn('Invalid node:', node)
            return
        }

        // A leading underscore marks something the machinery put there - the resolver, the
        // serializer, or this renderer's own bookkeeping. None of it is content, and a field
        // named that way has no label to explain itself, so it shows up as a bare string of
        // characters in the middle of a document nobody wrote it into.
        if (typeof node.name === 'string' && node.name.startsWith('_')) {
            return
        }

        // Record a stable render path on the node for downstream components (charts, events)
        try { node.__overseer_path = Array.isArray(path) ? path.slice() : [] } catch (_) {}

        // Global hidden parameter: skip rendering entire subtree if hidden=true
        try {
            const hiddenParam = this.getParameterValue(node, 'hidden')
            if (hiddenParam === true || String(hiddenParam).toLowerCase() === 'true') {
                return
            }
        } catch (_) { /* no-op */ }

        // Ensure node has a stable uid (for list items and any node we need to target precisely)
        try {
            if (!node.__uid) {
                const existingUid = (() => {
                    try { const u = node.parameters?._uid; if (u && typeof u === 'object' && u.String !== undefined) return String(u.String) } catch(_) {}
                    return null
                })()
                if (existingUid) {
                    node.__uid = existingUid
                } else {
                    const gen = `uid_${Date.now().toString(36)}_${Math.random().toString(36).slice(2)}`
                    if (!node.parameters) node.parameters = {}
                    node.parameters._uid = { String: gen }
                    node.__uid = gen
                }
                if (!this._uidToNode) this._uidToNode = new Map()
                this._uidToNode.set(node.__uid, node)
            } else {
                if (!this._uidToNode) this._uidToNode = new Map()
                if (!this._uidToNode.has(node.__uid)) this._uidToNode.set(node.__uid, node)
            }
        } catch(_) { /* best-effort */ }

        const element = this.createNodeElement(node)
    if (DEBUG_MODE) console.log('Created element:', element)

        if (element) {
            // Attach path metadata for event handling (DOM-only; do not mutate node)
            try {
                if (element.dataset) {
                    element.dataset.path = JSON.stringify(Array.isArray(path) ? path : [])
                    if (node.__uid) element.dataset.uid = node.__uid
                }
            } catch (_) { /* no-op */ }

            // Guard against null/undefined container (can occur during selective patch attempts when DOM node not found)
            if (!container) { if (DEBUG_MODE) console.warn('renderNode: null container for path', path, 'node', node); return }
            container.appendChild(element)
            if (DEBUG_MODE) console.log('Appended element to container')

            // Apply background-color with correct precedence:
            // 1) Own computed background-color when present
            // 2) If no computed and raw is a literal color, use it
            // 3) Otherwise inherit from parent (except for control elements)
            try {
                const nodeTypeLower = (node.node_type || node.type || '').toLowerCase()
                const hasComputedBg = !!(node?.parameters && node.parameters['_computed_background-color'] !== undefined)
                const hasRawFormulaBg = this.parameterHasFormula(node, 'background-color')
                const ownBgAny = this.getParameterValue(node, 'background-color')

                // Compute own effective background only if we can trust it (computed or literal)
                const ownEffectiveBg = hasComputedBg
                    ? this.convertColorValue(ownBgAny)
                    : (!hasRawFormulaBg && ownBgAny !== null && ownBgAny !== undefined
                        ? this.convertColorValue(ownBgAny)
                        : null)

                // Controls/fields should not force inherit to keep native look unless explicitly set
                const skipBgInherit = ['button','checkbox','string','text','int','float','bool','date','timestamp'].includes(nodeTypeLower)
                const effectiveBg = (ownEffectiveBg !== null && ownEffectiveBg !== undefined)
                    ? ownEffectiveBg
                    : (!skipBgInherit ? (inheritedStyles.backgroundColor ?? null) : null)

                if (DEBUG_MODE) {
                    try {
                        const dbgName = node.name || node.node_type || node.type || 'unknown'
                        if (DEBUG_MODE) console.log(`[BG] enter node=${dbgName} type=${node.node_type || node.type} parentBg=${inheritedStyles?.backgroundColor ?? 'null'}`)
                        if (DEBUG_MODE) console.log(`[BG] node=${dbgName} ownAny=${ownBgAny ? JSON.stringify(ownBgAny) : 'null'} computed=${hasComputedBg} rawFormula=${hasRawFormulaBg} ownEffective=${ownEffectiveBg ?? 'null'} effectiveBg=${effectiveBg ?? 'null'}`)
                    } catch (_) { /* no-op */ }
                }

                // Apply final decision: inherit explicitly for containers without own bg
                if ((ownEffectiveBg === null || ownEffectiveBg === undefined) && !skipBgInherit) {
                    element.style.backgroundColor = 'inherit'
                }
                if (effectiveBg !== null && effectiveBg !== undefined) {
                    try { element.style.backgroundColor = effectiveBg } catch (_) {}
                }

                // Prepare styles to pass to children (inherit current effective bg)
                const nextInherited = {
                    backgroundColor: effectiveBg,
                    // Keep legacy key for potential future use; not used for decisions now
                    rawBgFormula: null
                }

                // Render children (filter out non-visual action/event nodes)
                // Special case: link proxy — render another subtree inside this container
                // A container declaring (link="/path") acts as a view into that target.
                // Edits and events should route to the target via dataset.path of the rendered subtree.
                // Always read raw link; we'll compute a transient _computed_link each render.
                // Always read the RAW link parameter; do not use _computed_link here
                let linkVal = null
                try {
                    const p = node.parameters || {}
                    if (p.link !== undefined) {
                        const v = p.link
                        linkVal = (typeof v === 'object' && v !== null && v.String !== undefined) ? v.String : v
                    } else {
                        // Fallback: legacy documents may only have computed
                        linkVal = this.getParameterValue(node, 'link')
                    }
                } catch(_) { linkVal = this.getParameterValue(node, 'link') }
                if (linkVal !== null && linkVal !== undefined) {
                    try {
                        // Resolve target path and node
                        const { targetNode, targetPath, phantomPreviewNode, phantomMeta, computedLink } = this.resolveLinkTarget(linkVal, path)
                        // Cache resolved/interpolated link into computed param to help dependency tracking/refresh
                        try {
                            if (!node.parameters) node.parameters = {}
                            if (computedLink !== undefined) node.parameters._computed_link = { String: String(computedLink) }
                        } catch(_) {}
                        if (phantomPreviewNode && phantomMeta) {
                            // Flattened phantom rendering: behave like future materialized node root.
                            this._linkDepth = (this._linkDepth || 0) + 1
                            if (this._linkDepth <= 6) {
                                try { this.renderEventControls(node, element) } catch(_) {}
                                try { element.setAttribute('data-link-proxy', '1') } catch(_) {}
                                try { element.setAttribute('data-link-phantom', JSON.stringify(phantomMeta)) } catch(_) {}
                                try { element.removeAttribute('data-link-target-path') } catch(_) {}
                                // Policy: auto materialize on-access if configured
                                try {
                                    const policyRaw = this.getParameterValue(node, 'phantom-materialize')
                                    const policy = (policyRaw ? String(policyRaw) : 'none').toLowerCase()
                                    if (policy.endsWith('on-access')) {
                                        const isPrepend = policy.startsWith('prepend')
                                        this._materializePhantomAndComputePath(Object.assign({}, phantomMeta), { position: isPrepend ? 'prepend' : 'append' })
                                            .then((realPath) => {
                                                if (!realPath) return
                                                try { element.removeAttribute('data-link-phantom') } catch(_) {}
                                                const pathArr = String(realPath).split('/')
                                                const nodeReal = this.findNodeByPath(window.app.currentDocument, pathArr)
                                                if (nodeReal) {
                                                    try { element.innerHTML = '' } catch(_) {}
                                                    this.renderNode(nodeReal, element, nextInherited, pathArr)
                                                }
                                            })
                                            .catch(() => {/* ignore */})
                                    }
                                } catch(_) { /* ignore */ }
                                // Merge phantom root params into proxy without clobbering explicit overrides
                                try {
                                    const tgtParams = phantomPreviewNode.parameters || {}
                                    const proxyParams = node.parameters = node.parameters || {}
                                    const proxyHasBgOverride = proxyParams['background-color'] !== undefined && proxyParams['background-color'] !== null
                                    // Initialize tracking list for injected params if not present
                                    if (!Array.isArray(proxyParams._injected_link_params)) proxyParams._injected_link_params = []
                                    for (const k of Object.keys(tgtParams)) {
                                        // Skip copying target's computed bg and raw bg if proxy explicitly overrides bg
                                        if (proxyHasBgOverride && (k === '_computed_background-color' || k === 'background-color')) continue
                                        if (proxyParams[k] !== undefined) continue
                                        proxyParams[k] = tgtParams[k]
                                        try { if (!proxyParams._injected_link_params.includes(k)) proxyParams._injected_link_params.push(k) } catch(_) {}
                                    }
                                    // Remove any lingering computed bg on proxy if it overrides bg
                                    if (proxyHasBgOverride && proxyParams['_computed_background-color'] !== undefined) {
                                        try { delete proxyParams['_computed_background-color'] } catch(_) {}
                                    }
                                } catch(_) { /* ignore */ }
                                // Determine which background should drive inheritance for the phantom subtree:
                                // 1) Explicit proxy override if present
                                // 2) Otherwise, a literal or computed background on the phantom preview root
                                let phantomEffectiveBg = null
                                try {
                                    // Prefer computed if available on preview (rare), else a literal non-formula value
                                    const p = (phantomPreviewNode && phantomPreviewNode.parameters) ? phantomPreviewNode.parameters : {}
                                    const previewHasComputed = p && p['_computed_background-color'] !== undefined
                                    const previewRaw = p ? p['background-color'] : null
                                    const previewHasFormula = this.parameterHasFormula(phantomPreviewNode, 'background-color')
                                    if (previewHasComputed) {
                                        phantomEffectiveBg = this.convertColorValue(p['_computed_background-color'])
                                    } else if (!previewHasFormula && previewRaw !== null && previewRaw !== undefined) {
                                        phantomEffectiveBg = this.convertColorValue(previewRaw)
                                    }
                                } catch(_) { /* ignore */ }
                                let proxyExplicitBg = null
                                let proxyHasBgOverride = false
                                try {
                                    const p = node.parameters || {}
                                    if (p['background-color'] !== undefined && p['background-color'] !== null) {
                                        proxyHasBgOverride = true
                                        proxyExplicitBg = this.convertColorValue(p['background-color']) || p['background-color']
                                    }
                                } catch(_) { /* ignore */ }
                                // Decide selected background and propagate to current element and children
                                const selectedInheritedBg = (proxyExplicitBg !== null && proxyExplicitBg !== undefined)
                                    ? proxyExplicitBg
                                    : ((phantomEffectiveBg !== null && phantomEffectiveBg !== undefined) ? phantomEffectiveBg : null)
                                // Override element background and update inherited styles for children
                                let inheritedForChildren = nextInherited
                                try {
                                    if (selectedInheritedBg !== null && selectedInheritedBg !== undefined) {
                                        element.style.backgroundColor = selectedInheritedBg
                                        inheritedForChildren = Object.assign({}, nextInherited, { backgroundColor: selectedInheritedBg })
                                    }
                                } catch(_) { /* ignore */ }
                                // Apply overrides recursively to a clone of children
                                let mergedChildren = []
                                try {
                                    const baseChildren = Array.isArray(phantomPreviewNode.children) ? JSON.parse(JSON.stringify(phantomPreviewNode.children)) : []
                                    const overrideSpecs = (node.children || []).filter(ch => ch && !this.isEventHandlerName(ch.name) && !this.isActionName(ch.name))
                                    if (overrideSpecs.length > 0) {
                                        const applyOverrideRecursive = (kids, oNode) => {
                                            if (!kids) return
                                            const exact = kids.find(c => c && c.name === oNode.name)
                                            const mergeParams = (target, src) => {
                                                if (!target || !src) return
                                                const sp = src.parameters || {}
                                                if (!target.parameters) target.parameters = {}
                                                for (const k of Object.keys(sp)) target.parameters[k] = sp[k]
                                            }
                                            if (exact) {
                                                mergeParams(exact, oNode)
                                                const oKids = Array.isArray(oNode.children) ? oNode.children : []
                                                for (const ok of oKids) applyOverrideRecursive(exact.children, ok)
                                            }
                                        }
                                        for (const ov of overrideSpecs) applyOverrideRecursive(baseChildren, ov)
                                    }
                                    mergedChildren = baseChildren
                                } catch(_) { mergedChildren = Array.isArray(phantomPreviewNode.children) ? phantomPreviewNode.children : [] }
                                // Sanitize child backgrounds (same as real target flatten path) so inheritance from proxy/phantom works on first paint
                                try {
                                    const proxyBgRaw = (() => { try { const p = node.parameters||{}; return (p['background-color'] !== undefined && p['background-color'] !== null) ? p['background-color'] : null } catch(_) { return null } })()
                                    const useBg = (proxyBgRaw !== null && proxyBgRaw !== undefined) ? proxyBgRaw : selectedInheritedBg
                                    if (useBg !== null && useBg !== undefined) {
                                        const scrubNode = (n) => {
                                            if (!n || !n.parameters) return
                                            // Always remove computed background so proxy override can take effect, even if there is a formula
                                            if (n.parameters['_computed_background-color'] !== undefined) { try { delete n.parameters['_computed_background-color'] } catch(_) {} }
                                            // Remove explicit literal background only when it's not a formula
                                            const hasFormula = this.parameterHasFormula(n, 'background-color')
                                            if (!hasFormula && n.parameters['background-color'] !== undefined) { try { delete n.parameters['background-color'] } catch(_) {} }
                                            const kids = Array.isArray(n.children) ? n.children : []
                                            for (const k of kids) scrubNode(k)
                                        }
                                        mergedChildren = JSON.parse(JSON.stringify(mergedChildren))
                                        for (const c of mergedChildren) scrubNode(c)
                                    }
                                } catch(_) { /* ignore */ }
                                // Leaf target fallback: if link points directly at a field (string/number/etc.) there will be no children to render.
                                // In that case render the target node itself (as if it were a single child) so the field UI appears for editing.
                                try {
                                    if ((!mergedChildren || mergedChildren.length === 0) && phantomPreviewNode && (
                                        !Array.isArray(phantomPreviewNode.children) || phantomPreviewNode.children.length === 0
                                    )) {
                                        // Avoid mutating original preview node; clone shallow.
                                        const leafClone = JSON.parse(JSON.stringify(phantomPreviewNode))
                                        // Ensure it does not accidentally carry a link that would recurse; leaf nodes in tests do not, but guard anyway.
                                        if (leafClone.parameters && leafClone.parameters.link) {
                                            try { delete leafClone.parameters.link } catch(_) {}
                                        }
                                        mergedChildren = [leafClone]
                                    }
                                } catch(_) { /* best-effort leaf fallback */ }
                                const syntheticPath = path.concat(['<phantom>'])
                                for (const ch of mergedChildren) {
                                    const segBase = (ch.name || ch.node_type || ch.type || 'child')
                                    const chPath = syntheticPath.concat([segBase])
                                    this.renderNode(ch, element, inheritedForChildren, chPath)
                                }
                                // Post-pass: enforce inheritance visually for descendants without explicit override using CSS variable
                                try {
                                    if (selectedInheritedBg !== null && selectedInheritedBg !== undefined) {
                                        try {
                                            const conv = selectedInheritedBg
                                            element.style.setProperty('--overseer-link-proxy-bg', conv)
                                            element.setAttribute('data-proxy-bg','1')
                                        } catch(_) {}
                                        const descendants = element.querySelectorAll(':scope *')
                                        for (const d of descendants) {
                                            try {
                                                const styleBg = d.style && d.style.backgroundColor
                                                const hasExplicit = !!styleBg && styleBg !== '' && styleBg !== 'inherit'
                                                if (hasExplicit) {
                                                    if (!d.hasAttribute('data-bg-explicit')) {
                                                        d.style.removeProperty('background-color')
                                                    }
                                                }
                                                if (!d.hasAttribute('data-bg-explicit')) {
                                                    d.style.backgroundColor = 'var(--overseer-link-proxy-bg)'
                                                }
                                            } catch(_) {}
                                        }
                                    }
                                } catch(_) { /* best-effort */ }
                                this._linkDepth -= 1
                                return
                            } else {
                                if (DEBUG_MODE) console.warn('Link depth exceeded; skipping nested phantom rendering')
                            }
                            this._linkDepth -= 1
                        } else if (targetNode && targetPath && Array.isArray(targetPath)) {
                            // Flattened link rendering: make this proxy element act as the target root visually.
                            this._linkDepth = (this._linkDepth || 0) + 1
                            if (this._linkDepth <= 6) {
                                try { element.removeAttribute('data-link-phantom') } catch(_) {}
                                try { this.renderEventControls(node, element) } catch(_) {}
                                try { element.setAttribute('data-link-proxy', '1') } catch(_) {}
                                try { element.setAttribute('data-link-target-path', JSON.stringify(targetPath)) } catch(_) {}
                                // Merge target root parameters into proxy (without overwriting explicit overrides)
                                try {
                                    const tgtParams = targetNode.parameters || {}
                                    const proxyParams = node.parameters = node.parameters || {}
                                    if (!Array.isArray(proxyParams._injected_link_params)) proxyParams._injected_link_params = []
                                    const proxyHasBgOverride = proxyParams['background-color'] !== undefined && proxyParams['background-color'] !== null
                                    for (const k of Object.keys(tgtParams)) {
                                        // Skip copying target's computed background shadow if proxy overrides bg
                                        if (proxyHasBgOverride && (k === '_computed_background-color' || k === 'background-color')) continue
                                        if (proxyParams[k] !== undefined) continue
                                        proxyParams[k] = tgtParams[k]
                                        try { if (!proxyParams._injected_link_params.includes(k)) proxyParams._injected_link_params.push(k) } catch(_) {}
                                    }
                                    // If proxy overrides background-color, remove any lingering computed bg so style block uses override
                                    if (proxyHasBgOverride && proxyParams['_computed_background-color'] !== undefined) {
                                        try { delete proxyParams['_computed_background-color'] } catch(_) {}
                                    }
                                } catch(_) { /* ignore */ }
                                // Prepare cloned children with overrides applied
                                let mergedChildren = []
                                try {
                                    const baseChildren = Array.isArray(targetNode.children) ? JSON.parse(JSON.stringify(targetNode.children)) : []
                                    const overrideSpecs = (node.children || []).filter(ch => ch && !this.isEventHandlerName(ch.name) && !this.isActionName(ch.name))
                                    if (overrideSpecs.length > 0) {
                                        const applyOverrideRecursive = (kids, oNode) => {
                                            if (!kids) return
                                            const exact = kids.find(c => c && c.name === oNode.name)
                                            const mergeParams = (target, src) => {
                                                if (!target || !src) return
                                                const sp = src.parameters || {}
                                                if (!target.parameters) target.parameters = {}
                                                for (const k of Object.keys(sp)) target.parameters[k] = sp[k]
                                            }
                                            if (exact) {
                                                mergeParams(exact, oNode)
                                                const oKids = Array.isArray(oNode.children) ? oNode.children : []
                                                for (const ok of oKids) applyOverrideRecursive(exact.children, ok)
                                            }
                                        }
                                        for (const ov of overrideSpecs) applyOverrideRecursive(baseChildren, ov)
                                    }
                                    mergedChildren = baseChildren
                                } catch(_) { mergedChildren = Array.isArray(targetNode.children) ? targetNode.children : [] }
                                // If proxy sets explicit background-color, strip descendant computed background colors so they inherit.
                                try {
                                    const parentHasBgOverride = (() => { try { const raw = this.getParameterValue(node, 'background-color'); return raw !== null && raw !== undefined } catch(_) { return false } })()
                                    if (parentHasBgOverride) {
                                        // Capture original target root bg (computed or literal) to identify inherited duplicates.
                                        let originalRootBg = null
                                        try {
                                            const tp = targetNode.parameters || {}
                                            if (tp['_computed_background-color'] !== undefined) originalRootBg = tp['_computed_background-color']
                                            else if (tp['background-color'] !== undefined) originalRootBg = tp['background-color']
                                        } catch(_) { /* ignore */ }
                                        const proxyBg = (() => { try { return this.getParameterValue(node, 'background-color') } catch(_) { return null } })()
                                        const normalize = (v) => {
                                            if (v == null) return null
                                            let s = String(v).trim().toLowerCase()
                                            // Collapse 8-digit hex with full alpha to 6-digit for comparison (#rrggbbff -> #rrggbb)
                                            if (/^#([0-9a-f]{8})$/.test(s)) {
                                                const core = s.slice(1)
                                                const rgb = core.slice(0,6)
                                                const alpha = core.slice(6)
                                                if (alpha === 'ff') s = '#'+rgb
                                            }
                                            return s
                                        }
                                        const normOriginal = normalize(originalRootBg)
                                        const normProxy = normalize(proxyBg)
                                        const stripInheritedBg = (n) => {
                                            if (!n || !n.parameters) return
                                            const p = n.parameters
                                            const hasFormula = this.parameterHasFormula(n, 'background-color')
                                            if (!hasFormula) {
                                                const explicit = p['background-color']
                                                const comp = p['_computed_background-color']
                                                const normExplicit = normalize(explicit)
                                                const normComp = normalize(comp)
                                                // Remove computed shadow always so it can inherit proxy override
                                                if (comp !== undefined) { try { delete p['_computed_background-color'] } catch(_) {} }
                                                // Remove explicit literal if:
                                                //  a) it matches original root bg we're overriding OR
                                                //  b) it matches proxy bg (duplicate not needed) OR
                                                //  c) we cannot determine origin but want inheritance (treat as inherited) AND it is not an explicitly overridden child.
                                                // Heuristic (c): if explicit exists but this node name not present in overrideSpecs list (captured earlier) and normProxy is non-null.
                                                const isExplicitlyOverridden = false // we don't track per-child override mark; future improvement could tag
                                                if (explicit !== undefined) {
                                                    if ((normExplicit === normOriginal && normOriginal !== normProxy) ||
                                                        (normExplicit === normProxy) ||
                                                        (!isExplicitlyOverridden && normProxy)) {
                                                        try { delete p['background-color'] } catch(_) {}
                                                    }
                                                }
                                            }
                                            const kids = Array.isArray(n.children) ? n.children : []
                                            for (const k of kids) stripInheritedBg(k)
                                        }
                                        for (const ch of mergedChildren) stripInheritedBg(ch)
                                    }
                                } catch(_) { /* ignore */ }
                                const basePath = targetPath.slice()
                                // Build sanitized clones so descendants without explicit override inherit proxy bg deterministically.
                                let childrenToRender = mergedChildren
                                try {
                                    const proxyBg = (() => { try { return this.getParameterValue(node, 'background-color') } catch(_) { return null } })()
                                    if (proxyBg !== null && proxyBg !== undefined) {
                                        const scrubNode = (n) => {
                                            if (!n || !n.parameters) return
                                            const hasFormula = this.parameterHasFormula(n, 'background-color')
                                            if (!hasFormula) {
                                                if (n.parameters['_computed_background-color'] !== undefined) { try { delete n.parameters['_computed_background-color'] } catch(_) {} }
                                                if (n.parameters['background-color'] !== undefined) { try { delete n.parameters['background-color'] } catch(_) {} }
                                            }
                                            const kids = Array.isArray(n.children) ? n.children : []
                                            for (const k of kids) scrubNode(k)
                                        }
                                        childrenToRender = JSON.parse(JSON.stringify(mergedChildren))
                                        for (const c of childrenToRender) scrubNode(c)
                                    }
                                } catch(_) { /* best-effort */ }
                                for (const ch of childrenToRender) {
                                    const segBase = (ch.name || ch.node_type || ch.type || 'child')
                                    const chPath = basePath.concat([segBase])
                                    this.renderNode(ch, element, nextInherited, chPath)
                                }
                                // Post-pass: enforce inheritance visually for descendants without explicit override.
                                try {
                                    const proxyBg = (() => { try { return this.getParameterValue(node, 'background-color') } catch(_) { return null } })()
                                    if (proxyBg !== null && proxyBg !== undefined) {
                                        // Establish a stable CSS variable for proxy background so descendants consistently inherit it
                                        try {
                                            const conv = this.convertColorValue(proxyBg) || proxyBg
                                            element.style.setProperty('--overseer-link-proxy-bg', conv)
                                            element.setAttribute('data-proxy-bg','1')
                                        } catch(_) {}
                                        const descendants = element.querySelectorAll(':scope *')
                                        for (const d of descendants) {
                                            try {
                                                // If descendant has a hard-coded inline background (not a gradient or transparent), but its dataset path derives from the link target subtree, normalize it.
                                                const styleBg = d.style && d.style.backgroundColor
                                                const hasExplicit = !!styleBg && styleBg !== '' && styleBg !== 'inherit'
                                                if (hasExplicit) {
                                                    // Whitelist: if element carries data-bg-explicit we respect it
                                                    if (!d.hasAttribute('data-bg-explicit')) {
                                                        d.style.removeProperty('background-color')
                                                    }
                                                }
                                                // Always set variable-based background if no explicit override marker.
                                                if (!d.hasAttribute('data-bg-explicit')) {
                                                    d.style.backgroundColor = 'var(--overseer-link-proxy-bg)'
                                                }
                                            } catch(_) {}
                                        }
                                    }
                                } catch(_) { /* best-effort */ }
                                this._linkDepth -= 1
                                return
                            } else {
                                if (DEBUG_MODE) console.warn('Link depth exceeded; skipping nested link rendering')
                            }
                            this._linkDepth -= 1
                        } else {
                            // Show minimal placeholder to indicate broken link
                            const placeholder = document.createElement('div')
                            placeholder.className = 'overseer-link-placeholder'
                            placeholder.textContent = '⚠ broken link: ' + String(linkVal)
                            placeholder.style.opacity = '0.7'
                            placeholder.style.fontStyle = 'italic'
                            element.appendChild(placeholder)
                            // Ensure phantom flag is cleared on broken link as well
                            try { element.removeAttribute('data-link-phantom') } catch(_) {}
                            // Also render any event controls present on this container
                            try { this.renderEventControls(node, element) } catch(_) {}
                            // Skip own children to avoid confusion
                            return
                        }
                    } catch (e) {
                        if (DEBUG_MODE) console.warn('Failed to render link target:', e)
                        const placeholder = document.createElement('div')
                        placeholder.className = 'overseer-link-placeholder'
                        placeholder.textContent = '⚠ link error'
                        placeholder.style.opacity = '0.7'
                        placeholder.style.fontStyle = 'italic'
                        element.appendChild(placeholder)
                        // Render any event controls even if link resolution failed
                        try { this.renderEventControls(node, element) } catch(_) {}
                        return
                    }
                }

                if (node.children && Array.isArray(node.children)) {
                    // For list nodes, render in UI-sorted order using _ui_sort_key
                    const renderChildren = () => {
                        let children = node.children
                        if (nodeTypeLower === 'list') {
                            children = node.children
                                .map((ch, idx) => ({ ch, idx }))
                                .sort((a, b) => {
                                    const ka = (a.ch?.parameters && a.ch.parameters['_ui_sort_key'] !== undefined) ? a.ch.parameters['_ui_sort_key'] : a.idx
                                    const kb = (b.ch?.parameters && b.ch.parameters['_ui_sort_key'] !== undefined) ? b.ch.parameters['_ui_sort_key'] : b.idx
                                    const va = this.coerceSortKey(ka)
                                    const vb = this.coerceSortKey(kb)
                                    if (va < vb) return -1
                                    if (va > vb) return 1
                                    return a.idx - b.idx
                                })
                                .map(x => x.ch)
                        }
                        const filteredChildren = children.filter(ch => this.shouldRenderChild(node, ch))
                        if (DEBUG_MODE) console.log('Rendering', filteredChildren.length, 'children for node:', node)
                        for (const child of filteredChildren) {
                            // Build a logical, disambiguated path segment
                            const segBase = (child.name || child.node_type || child.type || 'child')
                            const isGenericTransparent = !!(child.is_hierarchy_transparent === true && (!child.name || String(child.name).toLowerCase() === String((child.node_type || child.type || '')).toLowerCase()))
                            // Compute ordinal among raw siblings with same name to disambiguate duplicates (name#k)
                            let seg = segBase
                            if (!isGenericTransparent && Array.isArray(node.children)) {
                                const idx = node.children.indexOf(child)
                                if (idx >= 0) {
                                    const k = node.children.slice(0, idx).filter(c => (c?.name || c?.node_type || c?.type) === segBase).length
                                    if (k > 0) seg = `${segBase}#${k}`
                                }
                            }
                            const childPath = isGenericTransparent ? path : [...path, seg]
                            if (DEBUG_MODE) {
                                try {
                                    const dbgParent = node.name || node.node_type || node.type || 'unknown'
                                    const dbgChild = seg
                                    if (DEBUG_MODE) console.log(`[BG] pass to child parent=${dbgParent} child=${dbgChild} inheritedBg=${nextInherited.backgroundColor ?? 'null'}`)
                                } catch (_) { /* no-op */ }
                            }
                            this.renderNode(child, element, nextInherited, childPath)
                        }
                    }
                    renderChildren()
                    if (nodeTypeLower === 'list') {
                        this.setUpTable(element, node)
                        this.drawEntriesLeftOut(element, node)
                    }
                } else {
                    if (DEBUG_MODE) console.log('No children for node:', node)
                    // An empty list still draws its heading, which is when a heading says most.
                    if (nodeTypeLower === 'list') this.setUpTable(element, node)
                }
            } catch (e) { if (DEBUG_MODE) console.warn('Style inheritance error:', e) }
        } else {
            if (DEBUG_MODE) console.warn('Failed to create element for node:', node)
        }
    }

    createNodeElement(node) {
        // Handle both possible node structures
        let nodeType = node.node_type || node.type || node.name || 'div'

        if (DEBUG_MODE) {
            if (DEBUG_MODE) console.log('[DEBUG] createNodeElement:', { nodeType, node });
            if (node.children && Array.isArray(node.children)) {
                if (DEBUG_MODE) console.log(`[DEBUG] Node ${nodeType} has ${node.children.length} children:`, node.children.map(c => ({ name: c.name, type: c.node_type, parameters: c.parameters })));
            }
        }

        switch (nodeType.toLowerCase()) {
            case '-':
                // Simple list entry (dash) parsed form
                return this.createListItemElement(node)
            case 'timer':
                return this.createTimerElement(node)
            case 'tab':
                return this.createTabElement(node)
            case 'div':
                return this.createDivElement(node)
            case 'list':
                return this.createListElement(node)
            case 'list_item':
                return this.createListItemElement(node)
            case 'string':
                return this.createStringElement(node)
            case 'text':
                return this.createTextElement(node)
            case 'tags':
                return this.createTagsElement(node)
            case 'filter':
                return this.createFilterElement(node)
            case 'int':
            case 'float':
                return this.createNumberElement(node)
            case 'date':
                return this.createDateElement(node)
            case 'timestamp':
                return this.createTimestampElement(node)
            case 'bool':
                return this.createBooleanElement(node)
            case 'button':
                return this.createButtonElement(node)
            case 'checkbox':
                return this.createCheckboxElement(node)
            case 'chart':
                return this.createChartElement(node)
            case 'mount':
                return this.createMountElement(node)
            default:
                // Treat unknown node types as divs (custom templates/components)
                return this.createDivElement(node)
        }
    }

    // Resolve a link value (string or object-wrapped String) to a target node and its disambiguated path array
    resolveLinkTarget(linkParam, currentPathArray) {
        try {
            let linkStr = (typeof linkParam === 'string') ? linkParam
                : (linkParam && typeof linkParam === 'object' && linkParam.String !== undefined) ? linkParam.String
                : String(linkParam)
            const doc = (window.app && window.app.currentDocument) ? window.app.currentDocument : null
            if (!doc || !linkStr) return { targetNode: null, targetPath: null }
            // Support basic $(...) interpolation inside link strings, e.g.,
            // "/Root/List[key=$(../selected_date)]". We evaluate inner expressions
            // only for simple relative paths and field reads, returning their display string.
            const interpolate = (s) => {
                try {
                    return String(s).replace(/\$\(([^)]*)\)/g, (_m, expr) => {
                        const e = String(expr || '').trim()
                        // Only support absolute/relative path reads for now
                        // e.g., ../selected_date or /Root/A/B
                        // Evaluate relative to the CURRENT node, not its parent (the resolver bases on parent)
                        const baseForExpr = Array.isArray(currentPathArray) ? currentPathArray.concat(['<self>']) : ['<self>']
                        const target = this.resolvePathStringToNode(doc, baseForExpr, e)
                        if (target && target.node) {
                            // Prefer raw underlying value (unformatted) for link keys to avoid timezone drift
                            try {
                                const params = target.node.parameters || {}
                                const raw = (params._computed_value !== undefined) ? params._computed_value : params.value
                                if (raw !== undefined && raw !== null) {
                                    if (typeof raw === 'object') {
                                        if (raw.Timestamp !== undefined) return String(raw.Timestamp)
                                        if (raw.Date !== undefined) return String(raw.Date)
                                        if (raw.String !== undefined) return String(raw.String)
                                        if (raw.Integer !== undefined) return String(raw.Integer)
                                        if (raw.Float !== undefined) return String(raw.Float)
                                        if (raw.Boolean !== undefined) return String(raw.Boolean)
                                    } else {
                                        return String(raw)
                                    }
                                }
                            } catch(_) { /* fall back to display value */ }
                            const v = this.getNodeValue(target.node)
                            return (v === null || v === undefined) ? '' : String(v)
                        }
                        return ''
                    })
                } catch(_) { return s }
            }
            // Perform interpolation before resolution
            linkStr = interpolate(linkStr)
            const resolved = this.resolvePathStringToNode(doc, currentPathArray, String(linkStr))
            if (resolved && resolved.phantomMeta) {
                return { targetNode: null, targetPath: null, phantomPreviewNode: resolved.phantomPreviewNode, phantomMeta: resolved.phantomMeta, computedLink: linkStr }
            }
            const { node, path } = resolved
            return { targetNode: node, targetPath: path, phantomPreviewNode: null, phantomMeta: null, computedLink: linkStr }
        } catch (_) {
            return { targetNode: null, targetPath: null, computedLink: undefined }
        }
    }

    // Resolve a path string to a node and canonical path array with name#k disambiguation
    // Supports:
    // - absolute paths starting with '/'
    // - relative paths from the parent of currentPathArray
    // - ordinal suffixes using name#k (existing)
    // - list indexing via bracket syntax: List[2] -> selects the 3rd list item (0-based)
    // - list key selection via bracket syntax: List[id=foo] -> selects item whose field 'id' equals 'foo'
    resolvePathStringToNode(documentArray, currentPathArray, pathStr) {
        const segsRaw = String(pathStr).split('/').filter(s => s.length > 0)
        let startNodes = documentArray
        let basePath = []
        if (String(pathStr).startsWith('/')) {
            // Absolute from root; first segment must match a root node name
            if (segsRaw.length === 0) return { node: null, path: null }
        } else {
            // Relative: start from parent of current node path
            const parentPath = Array.isArray(currentPathArray) ? currentPathArray.slice(0, -1) : []
            // Resolve parentPath into a concrete node to get its children
            const parentNode = this.findNodeByPath(documentArray, parentPath)
            if (parentNode && Array.isArray(parentNode.children)) {
                startNodes = parentNode.children
                basePath = parentPath.slice()
            }
        }

        // Helper: parse an ordinal suffix (e.g., "Title#1") into base name + ordinal integer
        const parseSeg = (seg) => {
            if (typeof seg !== 'string') return { base: String(seg), ord: 0 }
            const idx = seg.lastIndexOf('#')
            if (idx > 0) {
                const base = seg.slice(0, idx)
                const ordStr = seg.slice(idx + 1)
                const ord = Math.max(0, parseInt(ordStr, 10) || 0)
                return { base, ord }
            }
            return { base: seg, ord: 0 }
        }
        // Helper: stringify an OverseerValue into a JS primitive for comparison/rendering
        const toPrimitive = (val) => {
            if (val === null || val === undefined) return null
            if (typeof val !== 'object') return val
            if ('String' in val) return String(val.String)
            if ('Integer' in val) return Number(val.Integer)
            if ('Float' in val) return Number(val.Float)
            if ('Boolean' in val) return !!val.Boolean
            if ('Date' in val) return String(val.Date)
            if ('Timestamp' in val) return String(val.Timestamp)
            return String(val)
        }
        // Helper: get a child field value by name from a complex node
        const getFieldValue = (node, fieldName) => {
            try {
                if (!node || !Array.isArray(node.children)) return null
                const ch = node.children.find(c => (c?.name === fieldName))
                if (!ch) return null
                const v = ch.parameters ? (ch.parameters._computed_value ?? ch.parameters.value) : undefined
                return toPrimitive(v)
            } catch { return null }
        }

        let currentNodes = startNodes
        let outPath = basePath
        let currentNode = null
        for (let i = 0; i < segsRaw.length; i++) {
            let raw = segsRaw[i]
            if (!Array.isArray(currentNodes)) return { node: null, path: null }

            // Support explicit current and parent directory semantics in relative paths
            if (raw === '.' || raw === '') {
                // no-op, stay at current level
                currentNode = this.findNodeByPath(documentArray, outPath) || currentNode
                continue
            }
            if (raw === '..') {
                // Move up one level in base path and reset currentNodes accordingly
                outPath = outPath.slice(0, -1)
                const parentNode = this.findNodeByPath(documentArray, outPath)
                currentNodes = Array.isArray(parentNode?.children) ? parentNode.children : documentArray
                currentNode = parentNode || null
                continue
            }

            // Support bracket syntax on this segment: Name[expr]
            // expr can be a numeric index (0-based) or key selection key=value
            const bracket = (typeof raw === 'string') ? raw.match(/^(.*)\[(.+)\]$/) : null
            if (bracket) {
                // baseName may itself have an ordinal suffix (#k).
                const baseNameRaw = bracket[1]
                const selectorRaw = bracket[2]
                const { base: baseName, ord: baseOrd } = parseSeg(baseNameRaw)
                // First pick the base node among currentNodes
                const baseMatches = currentNodes.filter(n => (n && n.name === baseName))
                if (baseMatches.length === 0) return { node: null, path: null }
                const basePicked = baseMatches[baseOrd] || baseMatches[0]
                // Canonicalize base segment with its actual ordinal
                const idxBase = currentNodes.findIndex(n => n === basePicked)
                const actualBaseOrd = currentNodes.slice(0, idxBase).filter(n => n && n.name === baseName).length
                const baseSegWithOrd = actualBaseOrd > 0 ? `${baseName}#${actualBaseOrd}` : baseName
                outPath = outPath.concat([baseSegWithOrd])

                // Now select child under the picked base
                const children = Array.isArray(basePicked.children) ? basePicked.children : []
                if (!Array.isArray(children)) return { node: null, path: null }

                // Determine selection type: numeric index or key=value
                let pickedChild = null
                const numIdx = selectorRaw.match(/^\s*(\d+)\s*$/)
                if (numIdx) {
                    const k = parseInt(numIdx[1], 10)
                    pickedChild = (k >= 0 && k < children.length) ? children[k] : null
                } else {
                    // key selection: fieldName=value (value may be quoted; allow empty)
                    const kv = selectorRaw.match(/^\s*([A-Za-z0-9_]+)\s*=\s*(.*)\s*$/)
                    if (!kv) return { node: null, path: null }
                    let keyField = kv[1]
                    let rhs = kv[2]
                    // Strip quotes if present
                    if ((rhs.startsWith('"') && rhs.endsWith('"')) || (rhs.startsWith("'") && rhs.endsWith("'"))) {
                        rhs = rhs.slice(1, -1)
                    }
                    // Try number if purely numeric
                    const rhsPrim = /^-?\d+(?:\.\d+)?$/.test(rhs) && rhs !== '' ? Number(rhs) : rhs
                    // Normalize by keyPrecision when present (e.g., day precision for dates)
                    const keyPrecisionRaw = basePicked?.parameters?.keyPrecision || basePicked?.parameters?._computed_keyPrecision
                    const keyPrecision = (typeof keyPrecisionRaw === 'object' && keyPrecisionRaw.String !== undefined) ? String(keyPrecisionRaw.String) : (typeof keyPrecisionRaw === 'string' ? keyPrecisionRaw : '')
                    const normalizeByPrecision = (prec, val) => {
                        try {
                            if (val === null || val === undefined) return ''
                            const s = String(val)
                            if (!prec) return s
                            const p = prec.toLowerCase()
                            if (p === 'day' || p === 'days') {
                                // Accept YYYY-MM-DD, YYYY/MM/DD, YYYY.MM.DD, or RFC3339 strings
                                const m = s.match(/^(\d{4})[./-](\d{2})[./-](\d{2})(?:.*)?$/)
                                if (m) return `${m[1]}-${m[2]}-${m[3]}`
                                const d = new Date(s)
                                if (!isNaN(d.getTime())) {
                                    const pad = (n) => String(n).padStart(2, '0')
                                    return `${d.getFullYear()}-${pad(d.getMonth()+1)}-${pad(d.getDate())}`
                                }
                                return s
                            }
                            return s
                        } catch(_) { return String(val) }
                    }
                    // If selector used 'key=value', use the list's configured key field name
                    if (keyField.toLowerCase() === 'key') {
                        try {
                            const listKey = basePicked?.parameters?.key || basePicked?.parameters?._computed_key
                            if (typeof listKey === 'string') keyField = listKey
                            else if (listKey && typeof listKey === 'object' && listKey.String !== undefined) keyField = String(listKey.String)
                        } catch(_) {}
                    }
                    const rhsNorm = normalizeByPrecision(keyPrecision, rhsPrim)
                    pickedChild = children.find(ch => {
                        const v = getFieldValue(ch, keyField)
                        const vNorm = normalizeByPrecision(keyPrecision, v)
                        // Loose equality on normalized values
                        return (vNorm == rhsNorm)
                    }) || null

                    // If no such item exists (or key empty), construct a phantom preview and return with meta
                    if (!pickedChild) {
                        // Determine template name
                        let tmplName = null
                        try {
                            const entry = basePicked?.parameters?.entry
                            if (entry) {
                                if (typeof entry === 'string') tmplName = entry
                                else if (typeof entry === 'object' && entry.Template !== undefined) tmplName = String(entry.Template)
                                else if (typeof entry === 'object' && entry.String !== undefined) tmplName = String(entry.String)
                            }
                        } catch(_) {}
                        if (tmplName && tmplName.startsWith('<') && tmplName.endsWith('>') && tmplName.length >= 2) {
                            tmplName = tmplName.slice(1, -1)
                        }
                        const previewItem = this._buildPhantomItemPreview(documentArray, basePicked, tmplName, keyField, rhsNorm)
                        // Follow remaining segments (if any) into preview tree
                        let previewNode = previewItem
                        let relNodes = Array.isArray(previewItem.children) ? previewItem.children : []
                        const tail = segsRaw.slice(i + 1)
                        let tailPath = []
                        for (const t of tail) {
                            const { base: tb, ord: to } = parseSeg(t)
                            const matches = relNodes.filter(n => n && n.name === tb)
                            if (matches.length === 0) { previewNode = null; break }
                            const picked = matches[to] || matches[0]
                            const idxp = relNodes.findIndex(n => n === picked)
                            const aord = relNodes.slice(0, idxp).filter(n => n && n.name === tb).length
                            const segWithOrd = aord > 0 ? `${tb}#${aord}` : tb
                            tailPath.push(segWithOrd)
                            previewNode = picked
                            relNodes = Array.isArray(picked.children) ? picked.children : []
                        }
                        // Pack meta
                        const phantomMeta = {
                            listPath: outPath.slice(),
                            keyField,
                            keyValue: rhsNorm,
                            templateName: tmplName,
                            tailSegments: segsRaw.slice(i + 1)
                        }
                        return { node: null, path: null, phantomMeta, phantomPreviewNode: previewNode || previewItem }
                    }
                }

                if (!pickedChild) return { node: null, path: null }
                // Append canonical child segment using its display name + ordinal among siblings
                const childBase = (pickedChild.name || pickedChild.node_type || pickedChild.type || 'child')
                const idxChild = children.findIndex(n => n === pickedChild)
                const childOrd = children.slice(0, idxChild).filter(n => (n && (n.name || n.node_type || n.type) === childBase)).length
                const childSeg = childOrd > 0 ? `${childBase}#${childOrd}` : childBase
                outPath = outPath.concat([childSeg])
                currentNode = pickedChild
                currentNodes = Array.isArray(pickedChild.children) ? pickedChild.children : []
                continue
            }

            // Default: support name and optional #ordinal suffix
            const { base, ord } = parseSeg(raw)
            // Choose among children by exact base name match and ordinal
            const matches = currentNodes.filter(n => (n && (n.name === base)))
            if (matches.length === 0) return { node: null, path: null }
            const picked = matches[ord] || matches[0]
            // Determine actual ordinal of picked among siblings with same base name
            const idxOfPicked = currentNodes.findIndex(n => n === picked)
            const actualOrd = currentNodes.slice(0, idxOfPicked).filter(n => n && n.name === base).length
            const segWithOrd = actualOrd > 0 ? `${base}#${actualOrd}` : base
            outPath = outPath.concat([segWithOrd])
            currentNode = picked
            currentNodes = Array.isArray(picked.children) ? picked.children : []
        }
    return { node: currentNode, path: outPath }
    }

    // Mount placeholder renderer (Phase 1): shows summary/placeholder, defers loading
    createMountElement(node) {
        const container = document.createElement('div')
        container.className = 'overseer-div overseer-mount'

        // Header/title from placeholder or source
        const placeholder = this.getParameterValue(node, 'placeholder') || ''
        const source = this.getParameterValue(node, 'source') || ''
        const title = document.createElement('div')
        title.className = 'mount-title'
        title.textContent = placeholder ? String(placeholder) : (source ? String(source) : 'Mount')
        container.appendChild(title)

        // Status line (reflects _mount_status if present)
        const status = document.createElement('div')
        status.className = 'mount-status'
        const st = this.getParameterValue(node, '_mount_status') || 'unloaded'
        status.textContent = `Status: ${st}`
        container.appendChild(status)

        // Error details (if any)
        const err = this.getParameterValue(node, '_mount_error')
        if (err) {
            const errDiv = document.createElement('div')
            errDiv.className = 'mount-error'
            errDiv.textContent = String(err)
            // basic inline style to highlight error without relying on external CSS
            errDiv.style.color = '#b00020'
            errDiv.style.marginTop = '4px'
            container.appendChild(errDiv)
        }

        // Load button (emits 'load' event; backend handling will come later)
        const btn = document.createElement('button')
        btn.className = 'overseer-button'
        const icon = (this.getParameterValue(node, 'icon') || '').toString().trim().toLowerCase()
        if (icon) {
            btn.classList.add('icon-button')
            const span = document.createElement('span')
            span.className = `icon glyph-${icon}`
            span.setAttribute('aria-hidden', 'true')
            btn.appendChild(span)
            const title = this.getParameterValue(node, 'label') || icon
            if (title) btn.title = String(title)
        } else {
            const label = this.getParameterValue(node, 'label') || 'Load'
            const labelSpan = document.createElement('span')
            labelSpan.className = 'overseer-button-label'
            labelSpan.textContent = label
            btn.appendChild(labelSpan)
        }
        btn.addEventListener('click', async () => {
            if (btn.disabled) return
            const prev = btn.textContent
            btn.disabled = true
            btn.textContent = 'Loading…'
            try {
                await this.emitEvent(node, container, 'load')
            } catch (e) {
                // backend surfaces errors via _mount_error
            } finally {
                btn.disabled = false
                btn.textContent = prev
            }
        })
        container.appendChild(btn)

    // Apply general styles
    this.applyLayoutStyles(container, node)
        this.applyNodeStyles(container, node)
        return container
    }

    createTabElement(node) {
        // In headless / test environments tabContainer may be null; bail out gracefully
        if (!this.tabContainer) {
            if (DEBUG_MODE) console.warn('Skipping tab creation: tabContainer is null')
            const placeholder = document.createElement('div')
            placeholder.className = 'tab-content'
            placeholder.style.display = 'none'
            return placeholder
        }
        const tabButton = document.createElement('button')
        tabButton.className = 'tab-button'
    // Bug 8: Tabs should use a 'label' parameter instead of exposing node name
    tabButton.textContent = this.getParameterValue(node, 'label') || node.name || 'Tab'
        // Which tab this button belongs to, so a repaint can find the one it replaces.
        tabButton.dataset.tab = node.name || ''

        const tabContent = document.createElement('div')
        tabContent.className = 'tab-content'
        tabContent.style.display = 'none'

        // A repaint, rather than a first render.
        //
        // This runs again whenever a tab's own parameters change - which an event does every
        // time it touches a list inside one, because the resolver records the overrides on the
        // tab. Appending unconditionally then left a second button for the same tab, and the
        // content returned here starts hidden and is only shown when it is the only tab. So
        // pressing `bought` swapped the visible page for a hidden one and grew a duplicate
        // "Shopping" tab beside it: the page went blank and the new tab was empty.
        //
        // The existing button is replaced in place instead - same position in the row, and no
        // stale click handler left pointing at content that has just been swapped out.
        const name = tabButton.dataset.tab
        const existing = name
            ? Array.from(this.tabContainer.children).find(b => b.dataset && b.dataset.tab === name)
            : null
        const wasActive = !!(existing && existing.classList.contains('active'))
        if (existing) {
            this.tabContainer.replaceChild(tabButton, existing)
        } else {
            this.tabContainer.appendChild(tabButton)
        }

        // Tab click handler
        tabButton.addEventListener('click', () => {
            // Hide all tab contents and deactivate buttons
            document.querySelectorAll('.tab-content').forEach(content => { content.style.display = 'none' })
            document.querySelectorAll('.tab-button').forEach(btn => { btn.classList.remove('active') })
            // Show this tab's content
            tabContent.style.display = 'block'
            tabButton.classList.add('active')
        })

        // Whichever tab was showing goes on showing; failing that, the first one does.
        if (wasActive || (!existing && this.tabContainer.children.length === 1)) {
            tabButton.classList.add('active')
            tabContent.style.display = 'block'
        }

        this.applyNodeStyles(tabContent, node)
        return tabContent
    }

    /// How narrow a column may get before the row holds one fewer.
    ///
    /// `flow` fits as many equal columns as it can, and what "as many as it can" means depends
    /// entirely on how much room a card needs to stay readable - which the document knows and
    /// the stylesheet cannot. Given none, the sheet's own minimum applies.
    applyFlowColumns(element, node, layout) {
        if (layout !== 'flow') return
        const raw = this.getParameterValue(node, 'min-width')
        if (raw === null || raw === undefined) return
        const min = this.convertCssSizeValue(raw)
        if (!min) return
        element.style.gridTemplateColumns = `repeat(auto-fit, minmax(${min}, 1fr))`
    }

    createDivElement(node) {
        const div = document.createElement('div')
        div.className = 'overseer-div'

        // A div written without a name groups its children for layout and stands for nothing
        // itself - the same rule the addresses, the formulas and the actions follow. Drawing it
        // a box of its own is the visual version of giving it an address: every group added for
        // arrangement becomes another frame, and the borders stop meaning anything.
        const isWrapper = node.is_hierarchy_transparent &&
            (!node.name || node.name === node.node_type)
        if (isWrapper) {
            div.classList.add('layout-wrapper')
        }

        if (node.name) {
            div.setAttribute('data-name', node.name)
        }
    // legacy div hidden handling removed in favor of global hidden check
        
        // Apply layout (use effective layout calculated by resolver, or fall back to explicit parameter)
        const layout = this.getEffectiveLayout(node)
        div.classList.add(`layout-${layout}`)
        this.applyFlowColumns(div, node, layout)
        
        // Apply spacing and margins
        this.applyLayoutStyles(div, node)
        this.applyNodeStyles(div, node)
        return div
    }

    createListElement(node) {
        const list = document.createElement('div')
        list.className = 'overseer-list'
        
        // Don't show list name as header - we just want the list contents
        // Lists should be transparent containers for their items
        
        // Apply layout (use effective layout calculated by resolver, or fall back to explicit parameter)
        const layout = this.getEffectiveLayout(node)
        list.classList.add(`layout-${layout}`)
        this.applyFlowColumns(list, node, layout)
        
    // Apply spacing and margins
    this.applyLayoutStyles(list, node)
    this.applyNodeStyles(list, node)
        return list
    }

    /**
     * A list drawn as a table: columns that line up all the way down.
     *
     * Rows line up today only by coincidence - every row carries the same percentage widths - and
     * the coincidence breaks wherever a field is hidden, because a hidden field is not rendered
     * at all and everything after it slides left to fill the gap. A task with children shows a
     * percentage and a leaf does not, so the two kinds of row disagree about where every later
     * column starts, and a list of both reads ragged.
     *
     * So the columns are declared once on the list and each cell is placed in the one that is
     * its own. A cell that is missing leaves its column empty instead of moving its neighbours.
     *
     * The column set comes from the resolver, which reads it off the entry template - see
     * `table_columns` there - and it has to come from the template rather than from the rows: no
     * single row knows the whole set, and a list with nothing in it has no rows to ask.
     *
     * `view` rather than `layout`, because a table is not a direction: a table of rows and a
     * table of columns are both tables, and `layout` means something already on every other kind
     * of node.
     */
    setUpTable(list, node) {
        if (!list || String(this.getParameterValue(node, 'view') || '') !== 'table') return
        const columns = this.getParameterValue(node, '_columns')
        if (!columns) return
        // Kept on the element so a later repaint can put back what it undid without the node.
        list.dataset.columns = String(columns)
        list.dataset.header = this.getParameterValue(node, 'header') ? '1' : ''
        list.dataset.stickyHeader = this.getParameterValue(node, 'sticky') ? '1' : ''
        // `true` for both, or name the one wanted: `vertical` rules between the columns,
        // `horizontal` between the rows.
        const lines = this.getParameterValue(node, 'lines')
        list.dataset.lines = (lines === null || lines === undefined) ? '' : String(lines)
        this.arrangeTable(list)
    }

    /**
     * Put every cell in its column.
     *
     * Written to be run again at any time. Anything repainted underneath a table - one field
     * re-rendered after an edit - comes back as fresh elements that know nothing about the grid
     * over them, which is the same hazard the filter has and is handled the same way.
     */
    arrangeTable(list) {
        let columns
        try { columns = JSON.parse(list.dataset.columns || '[]') } catch (_) { return }
        if (!Array.isArray(columns) || columns.length === 0) return

        const across = columns.filter((c) => !c.span)
        const ownLine = new Set(columns.filter((c) => c.span).map((c) => c.name))
        if (across.length === 0) return

        // The list stops arranging its children itself and becomes the grid they sit in. Its
        // layout classes carry `display: flex !important`, so they have to go rather than be
        // overridden - which also keeps every rule here free of `!important`, and so below the
        // one that hides a filtered row.
        list.classList.add('view-table')
        list.classList.remove('layout-vertical', 'layout-horizontal', 'layout-flow')
        const lines = list.dataset.lines || ''
        list.classList.toggle('table-lines-columns', lines === 'true' || lines === 'vertical')
        list.classList.toggle('table-lines-rows', lines === 'true' || lines === 'horizontal')
        list.style.gridTemplateColumns = across.map((c) => c.width || '1fr').join(' ')

        const column = new Map(across.map((c, i) => [c.name, i + 1]))

        for (const row of Array.from(list.children)) {
            if (row.classList.contains('table-header')) continue
            row.classList.add('table-row')
            row.classList.remove('layout-horizontal', 'layout-vertical', 'layout-flow')
            // A horizontal row centres its children on the line and lets them be as tall as they
            // are. A table row wants the opposite: cells that fill its height, so that a
            // separator drawn between two of them runs the whole way down instead of stopping
            // where the shorter one's text does. The content is centred inside the cell instead.
            row.style.alignItems = ''

            for (const cell of Array.from(row.children)) {
                const name = this.fieldNameOf(cell)
                // The column owns the width now. Left on the cell, a percentage would be read
                // against its own column rather than the row, and every cell would be a sliver
                // of the space it asked for.
                cell.style.width = ''
                if (column.has(name)) {
                    cell.style.gridColumn = String(column.get(name))
                } else if (ownLine.has(name)) {
                    // Its own line under the rest, inside the row - so it keeps the row's box
                    // and whatever colour the row is tinted, rather than becoming a row of its
                    // own that happens to sit next to it.
                    cell.style.gridColumn = '1 / -1'
                    cell.classList.add('table-own-line')
                }
                // The heading says what the column holds, so the cell need not repeat it.
                const label = cell.querySelector(':scope > label')
                if (label) label.remove()
                cell.classList.remove('label-beside', 'label-above')

                // Room a table does not have.
                //
                // A field is padded to sit in a form, one of half a dozen down a page. In a
                // table it is one of a hundred rows, and that padding was about two thirds of
                // the height of one.
                //
                // Only a padding the renderer chose is dropped; one the document asked for is
                // its own business. Which is which is read from the flag set where the default
                // is applied, rather than by recognising the value - there are three of those
                // and they would drift.
                if (cell.dataset.paddingIsDefault) cell.style.padding = ''
                const value = cell.querySelector(':scope > .field-value, :scope > .text-content')
                if (value && value.dataset.minHeightIsDefault) value.style.minHeight = ''
            }
        }

        this.drawTableHeading(list, across)
    }

    /**
     * The heading row.
     *
     * Rebuilt rather than patched: this runs again after every repaint, and appending would
     * leave a table wearing three headings - the same bug the tab bar had.
     */
    drawTableHeading(list, across) {
        const existing = list.querySelector(':scope > .table-header')
        if (existing) existing.remove()
        if (list.dataset.header !== '1') return

        const header = document.createElement('div')
        header.className = 'table-header'
        // Sticky holds it at the top of the scrolling area while the table is on screen, and
        // lets it leave with the table - which is what sticky does by itself, being confined to
        // its parent's box. Nothing on the way up to the scroller sets an `overflow`, which is
        // the usual reason this silently does nothing.
        if (list.dataset.stickyHeader === '1') header.classList.add('sticky')

        across.forEach((c, i) => {
            const cell = document.createElement('span')
            cell.className = 'table-heading'
            cell.textContent = c.label || ''
            cell.style.gridColumn = String(i + 1)
            header.appendChild(cell)
        })
        list.insertBefore(header, list.firstChild)
    }

    /** Which field a cell holds, by the last segment of the path it carries. */
    fieldNameOf(element) {
        try {
            const path = JSON.parse(element.dataset.path || '[]')
            return Array.isArray(path) && path.length ? String(path[path.length - 1]) : ''
        } catch (_) {
            return ''
        }
    }

    /** Re-place every table on the page, after something underneath one was repainted. */
    arrangeTables() {
        for (const list of document.querySelectorAll('.view-table')) this.arrangeTable(list)
    }

    coerceSortKey(key) {
        // Support numbers and string keys; fall back to string comparison
        if (key === null || key === undefined) return Number.MAX_SAFE_INTEGER
        if (typeof key === 'number') return key
        // If OverseerValue serialized object, try common shapes
        if (typeof key === 'object') {
            if (key.Integer !== undefined) return Number(key.Integer)
            if (key.Float !== undefined) return Number(key.Float)
            if (key.String !== undefined) return String(key.String)
            // try toString last
            try { return String(key) } catch (_) { return Number.MAX_SAFE_INTEGER }
        }
        const n = Number(key)
        return isNaN(n) ? String(key) : n
    }

    createListItemElement(node) {
        const listItem = document.createElement('div')
        listItem.className = 'overseer-list-item'

    if (DEBUG_MODE) console.log('[DEBUG] createListItemElement:', { nodeType: node.node_type, node });

        // Check if this is a simple value list item (has a value parameter but no children)
        const hasValue = node.parameters && node.parameters["value"] !== undefined
        const hasChildren = node.children && Array.isArray(node.children) && node.children.length > 0
        
        if (hasValue && !hasChildren) {
            // This is a simple value list item like - "some string"
            if (DEBUG_MODE) console.log('[DEBUG] List item is simple value node:', node);
            const value = this.getNodeValue(node)
            if (value !== null && value !== undefined) {
                const valueElement = document.createElement('span')
                valueElement.className = 'overseer-list-value'
                valueElement.textContent = value
                listItem.appendChild(valueElement)
            }
        } else {
            // Check if it's a typed value node (string, int, float, bool, date, text)
            const valueTypes = ['string', 'int', 'float', 'bool', 'date', 'text']
            const nodeType = (node.node_type || node.type || '').toLowerCase()
            
            if (valueTypes.includes(nodeType)) {
                let valueElement
                if (DEBUG_MODE) console.log('[DEBUG] List item is typed value node, extracting value:', node);
                switch (nodeType) {
                    case 'string':
                        valueElement = this.createStringElement(node)
                        break
                    case 'int':
                    case 'float':
                        valueElement = this.createNumberElement(node)
                        break
                    case 'bool':
                        valueElement = this.createBooleanElement(node)
                        break
                    case 'date':
                        valueElement = this.createDateElement(node)
                        break
                    case 'text':
                        valueElement = this.createTextElement(node)
                        break
                    default:
                        valueElement = document.createTextNode(this.getNodeValue(node) || '')
                }
                listItem.appendChild(valueElement)
            } else {
                // Complex list item with children: do NOT render children here.
                // Let the generic renderNode() flow render node.children exactly once
                // so inherited background-color is computed consistently per parent.
                if (hasChildren) {
                    if (DEBUG_MODE) console.log(`[DEBUG] List item is complex node with ${node.children.length} children (deferred to renderNode):`, node.children.map(c => ({ name: c.name, type: c.node_type, parameters: c.parameters })));
                } else {
                    if (DEBUG_MODE) console.log('[DEBUG] List item has no value or children:', node);
                }
            }
        }

    // Apply layout and node styles so spacing/appearance are consistent
    try { this.applyLayoutStyles(listItem, node) } catch(_) {}
    this.applyNodeStyles(listItem, node)
        return listItem
    }

    // Render simple controls for event handler blocks (e.g., an on click button)
    renderEventControls(node, container) {
        try {
            if (!node || !Array.isArray(node.children)) return
            const handlers = node.children.filter(ch => this.isEventHandlerName(ch?.name))
            for (const h of handlers) {
                const ev = String(h.name).toLowerCase()
                // Look for a button under the handler to derive label/icon
                let label = 'Action'
                let icon = null
                try {
                    const btn = (h.children || []).find(c => (c.node_type || c.type || '').toLowerCase() === 'button')
                    if (btn) {
                        const maybe = this.getParameterValue(btn, 'label')
                        if (maybe) label = String(maybe)
                        const ic = this.getParameterValue(btn, 'icon')
                        if (ic) icon = String(ic)
                    }
                } catch(_) {}

                const btnEl = document.createElement('button')
                btnEl.className = 'overseer-button'
                if (icon) {
                    btnEl.classList.add('icon-button')
                    const span = document.createElement('span')
                    span.className = `icon glyph-${icon}`
                    span.setAttribute('aria-hidden', 'true')
                    btnEl.appendChild(span)
                } else {
                    const labelSpan = document.createElement('span')
                    labelSpan.className = 'overseer-button-label'
                    labelSpan.textContent = label
                    btnEl.appendChild(labelSpan)
                }
                btnEl.addEventListener('click', async () => {
                    try { await this.emitEvent(node, container, ev) } catch(_) {}
                })
                container.appendChild(btnEl)
            }
        } catch(_) { /* ignore */ }
    }

    /**
     * A set of tags, drawn as what it is rather than as the line it is stored on.
     *
     * The value is a comma-separated set - already a real collection to the evaluator, which
     * can `filter` and `count` it - but it was rendered as a plain string, so a field holding
     * six labels read as forty characters of prose. Here each one gets a chip.
     *
     * `vocabulary` names a keyed list of the tags that exist: its entries supply the label to
     * show and the colour to show it in, so the palette is document data rather than something
     * the renderer decides. Without one the chips take a default colour, which is what an
     * existing document with a bare `tags` field gets.
     *
     * Editing goes through the same path as every other field: the set is written back as the
     * line it came from and `reevaluateDocumentSelective` works out the rest. There is nothing
     * here the server needs to know about.
     */
    /**
     * A filter over a list: some text to match, and tags to narrow by.
     *
     * Client-side, and deliberately. The alternative was a field in the document and `hidden`
     * formulas on the entries, which would have meant re-resolving the whole document on every
     * keystroke - 289ms measured on a document smaller than the one this is for - and storing
     * the filter, so it would be written to the file, committed by the backup and synced to
     * whoever else is reading. A filter is a view, like which tab is open or where you have
     * scrolled. `sort_by` is presentation-only for the same reason.
     *
     * Nothing here writes to the document, and the server never hears about it.
     *
     * What it needs from the document:
     *   target     - the list to filter
     *   text       - which of an entry's fields the typed text is matched against
     *   tags       - which field holds an entry's tags, if it has any
     *   vocabulary - the tag list, for drawing the chips to narrow by
     *   status     - a nought-to-a-hundred field, offered as three boxes: not started, in
     *                progress, finished. Derived rather than stored, because "in progress" is
     *                not a state anything writes down - it is what a percentage between the
     *                two ends means.
     */
    createFilterElement(node) {
        const container = document.createElement('div')
        container.className = 'overseer-filter'

        const target = String(this.getParameterValue(node, 'target') || '')
            .split('/').filter(Boolean)
        const textFields = String(this.getParameterValue(node, 'text') || '')
            .split(',').map(s => s.trim()).filter(Boolean)
        const tagField = this.getParameterValue(node, 'tags')
        const statusField = this.getParameterValue(node, 'status')
        if (target.length === 0) {
            // Nothing to filter is worth saying out loud rather than rendering an inert box.
            container.textContent = 'filter: no target list named'
            container.classList.add('filter-broken')
            return container
        }

        // Kept on the renderer rather than in the element, so it survives the element being
        // replaced - which happens on every repaint of the list it filters. A filter that
        // silently cleared itself whenever something changed nearby would be worse than none.
        const key = target.join('/')
        const state = this.filterState(key, { target, textFields, tagField, statusField })

        if (textFields.length > 0) {
            const box = document.createElement('input')
            box.className = 'filter-text'
            box.type = 'text'
            box.placeholder = this.getParameterValue(node, 'label') || 'filter'
            box.value = state.text
            // On input, not on blur: the point of typing into it is watching the list shrink.
            box.addEventListener('input', () => {
                state.text = box.value
                this.applyFilters()
            })
            container.appendChild(box)
        }

        if (tagField) {
            const vocabulary = this.tagVocabulary(node)
            const chips = document.createElement('span')
            chips.className = 'filter-tags'
            for (const [tag, known] of vocabulary) {
                const chip = this.tagChip(tag, known, null)
                chip.classList.add('filter-tag')
                if (state.tags.has(tag)) chip.classList.add('filter-tag-on')
                chip.addEventListener('click', () => {
                    if (state.tags.has(tag)) state.tags.delete(tag)
                    else state.tags.add(tag)
                    chip.classList.toggle('filter-tag-on', state.tags.has(tag))
                    this.applyFilters()
                })
                chips.appendChild(chip)
            }
            container.appendChild(chips)
        }

        if (statusField) {
            const boxes = document.createElement('span')
            boxes.className = 'filter-status'
            for (const [which, label] of [['none', 'not started'],
                                          ['some', 'in progress'],
                                          ['done', 'finished']]) {
                const holder = document.createElement('label')
                holder.className = 'filter-status-option'
                const box = document.createElement('input')
                box.type = 'checkbox'
                box.checked = state.status.has(which)
                box.addEventListener('change', () => {
                    if (box.checked) state.status.add(which)
                    else state.status.delete(which)
                    this.applyFilters()
                })
                holder.appendChild(box)
                holder.appendChild(document.createTextNode(label))
                boxes.appendChild(holder)
            }
            container.appendChild(boxes)
        }

        const count = document.createElement('span')
        count.className = 'filter-count'
        count.dataset.filterCount = key
        container.appendChild(count)

        this.applyNodeStyles(container, node)
        // Applied after this returns, when the list it filters is on the page too.
        setTimeout(() => this.applyFilters(), 0)
        return container
    }

    /** What a filter is currently set to, remembered across repaints. */
    filterState(key, about) {
        if (!this._filters) this._filters = new Map()
        const held = this._filters.get(key)
        if (held) {
            // The document may have been reloaded with the fields named differently.
            Object.assign(held, about)
            return held
        }
        const fresh = Object.assign({ text: '', tags: new Set(), status: new Set() }, about)
        this._filters.set(key, fresh)
        return fresh
    }

    /**
     * Hide the entries of every filtered list that do not match, and say how many are left.
     *
     * Called after any render as well as on every keystroke, because a repaint replaces the
     * entries with fresh elements that know nothing about the filter. Reading the fields from
     * the document rather than from the page: what is on screen is abbreviated, formatted and
     * sometimes hidden, and none of that is what you meant to search.
     */
    applyFilters() {
        if (!this._filters || this._filters.size === 0) return
        const doc = window.app && window.app.currentDocument
        if (!doc) return

        // Every element that carries a path, gathered once. Asking the document for each entry
        // in turn instead - `querySelectorAll` per row - cost 338ms for one keystroke over 120
        // rows, which is what the document-side filter would have cost and the reason this one
        // is here at all. Several elements can share a path, because a transparent layout div
        // contributes no segment of its own.
        const byPath = new Map()
        for (const element of document.querySelectorAll('[data-path]')) {
            const held = byPath.get(element.dataset.path)
            if (held) held.push(element)
            else byPath.set(element.dataset.path, [element])
        }

        for (const [key, state] of this._filters) {
            const list = this.findNodeByPath(doc, state.target)
            if (!list || !Array.isArray(list.children)) continue
            const wanted = state.text.trim().toLowerCase()
            let showing = 0

            for (const entry of list.children) {
                const matches = this.entryMatchesFilter(entry, wanted, state)
                if (matches) showing += 1
                const path = JSON.stringify([...state.target, entry.name])
                for (const element of byPath.get(path) || []) {
                    element.classList.toggle('filtered-out', !matches)
                }
            }

            for (const label of document.querySelectorAll(`[data-filter-count='${key}']`)) {
                const total = list.children.length
                const narrowed = wanted !== '' || state.tags.size > 0 || state.status.size > 0
                label.textContent = narrowed ? `${showing} of ${total}` : `${total}`
            }
        }
    }

    /** Whether one entry survives the filter. Empty filter, everything survives. */
    entryMatchesFilter(entry, wanted, state) {
        // Searched all the way down, not just among the entry's own children. An entry's fields
        // sit inside the layout divs that arrange them - `title` is a grandchild of the row in
        // every document written so far - so looking only at direct children found nothing and
        // quietly filtered everything away.
        const fieldText = (name) => {
            const seek = (node) => {
                for (const child of node.children || []) {
                    if (child && child.name === name) return child
                    const found = seek(child)
                    if (found) return found
                }
                return null
            }
            const child = seek(entry)
            if (!child) return ''
            const value = this.getParameterValue(child, 'value')
            return value === null || value === undefined ? '' : String(value)
        }

        if (wanted !== '') {
            const haystack = state.textFields.map(fieldText).join(' ').toLowerCase()
            if (!haystack.includes(wanted)) return false
        }

        if (state.status.size > 0) {
            if (!state.statusField) return false
            const figure = Number(fieldText(state.statusField))
            // Anything that is not a number counts as not started: a task with nothing in the
            // field has not been begun, which is the honest reading of an empty percentage.
            const which = !Number.isFinite(figure) || figure <= 0 ? 'none'
                : figure >= 100 ? 'done'
                : 'some'
            if (!state.status.has(which)) return false
        }

        if (state.tags.size > 0) {
            if (!state.tagField) return false
            // Narrowing, not widening: picking a second tag asks for the things that are both,
            // which is what adding a condition to a filter is usually taken to mean.
            const held = new Set(fieldText(state.tagField).split(',').map(s => s.trim()))
            for (const tag of state.tags) {
                if (!held.has(tag)) return false
            }
        }

        return true
    }

    createTagsElement(node) {
        const container = document.createElement('div')
        container.className = 'overseer-field tags-field'

        const labelText = this.getParameterValue(node, 'label')
        if (labelText) {
            const label = document.createElement('label')
            label.textContent = labelText
            container.appendChild(label)
        }

        const chips = document.createElement('span')
        chips.className = 'tag-chips'
        container.appendChild(chips)

        const held = () => String(this.getNodeValue(node) || '')
            .split(',').map(s => s.trim()).filter(Boolean)

        const write = (tags) => {
            // Read before writing: the backend is told what this changed *from* and *to*, and
            // the first of those is gone the moment the node is updated.
            const before = String(this.getNodeValue(node) || '')
            const after = tags.join(', ')
            this.updateNodeValue(node, after)
            try { window.app.markDocumentModified && window.app.markDocumentModified() } catch (_) {}
            try {
                // The value has to travel, not just the path. What gets written to the file is
                // the text the backend produced, and it can only put this edit into that text
                // if it is told what the edit was - without it the tag shows on screen, the
                // save writes text resolved from content that never had it, and the tag is gone
                // on the next load.
                const path = this.buildNodePath(container).join('/')
                window.app.reevaluateDocumentSelective(
                    [path], [{ path, oldValue: before, newValue: after }]
                )
            } catch (_) {
                try { window.app.reevaluateDocumentSelective([]) } catch (_) {}
            }
        }

        const editable = () => this.getEffectiveMutableMode(node, container) !== 'false'

        const paint = () => {
            chips.textContent = ''
            const vocabulary = this.tagVocabulary(node)
            for (const tag of held()) {
                const removing = editable()
                    ? () => { write(held().filter(t => t !== tag)); paint() }
                    : null
                chips.appendChild(this.tagChip(tag, vocabulary.get(tag), removing))
            }
            if (!editable()) return
            // Offered rather than always shown: a hundred tasks each with a picker open is
            // unreadable, and the tags themselves are what you came to look at.
            const add = document.createElement('button')
            add.className = 'tag-add'
            add.textContent = '+'
            add.title = 'add a tag'
            add.addEventListener('click', (event) => {
                event.stopPropagation()
                const spare = [...vocabulary.keys()].filter(tag => !held().includes(tag))
                this.offerTags(add, spare, vocabulary, (tag) => { write([...held(), tag]); paint() })
            })
            chips.appendChild(add)
        }
        paint()

        this.applyNodeStyles(container, node)
        return container
    }

    /**
     * The tags a field may hold: display name and colour, by tag.
     *
     * Read from the list named by `vocabulary`, whose entries are ordinary document data. Empty
     * when the field names no list, or names one that is not there - a field without a
     * vocabulary is still perfectly usable, it simply has no colours to draw with.
     */
    tagVocabulary(node) {
        const found = new Map()
        const where = this.getParameterValue(node, 'vocabulary')
        if (!where) return found
        try {
            const segments = String(where).split('/').filter(Boolean)
            const list = this.findNodeByPath(window.app.currentDocument, segments)
            for (const entry of (list && list.children) || []) {
                const field = (name) => {
                    const child = (entry.children || []).find(c => c && c.name === name)
                    return child ? this.getParameterValue(child, 'value') : null
                }
                const tag = field('tag')
                if (tag === null || tag === undefined || tag === '') continue
                found.set(String(tag), {
                    name: field('name') || String(tag),
                    colour: field('colour') || field('color') || null,
                })
            }
        } catch (_) { /* a field with no readable vocabulary just has no colours */ }
        return found
    }

    /** One chip. `remove` absent means the field cannot be edited, so it is not offered. */
    tagChip(tag, known, remove) {
        const chip = document.createElement('span')
        chip.className = 'tag-chip'
        chip.dataset.tag = tag
        chip.textContent = (known && known.name) || tag
        if (known && known.colour) {
            chip.style.backgroundColor = known.colour
            chip.style.color = this.readableOn(known.colour)
        }
        if (!known) {
            // A tag the vocabulary does not list. Shown rather than hidden: it is in the
            // document, and dropping it from the display would misreport what the field holds.
            chip.classList.add('tag-unknown')
            chip.title = 'not in this document\u2019s tag list'
        }
        if (remove) {
            const cross = document.createElement('button')
            cross.className = 'tag-remove'
            cross.textContent = '\u00d7'
            cross.title = `remove ${tag}`
            cross.addEventListener('click', (event) => { event.stopPropagation(); remove() })
            chip.appendChild(cross)
        }
        return chip
    }

    /** The tags not yet on this field, to pick from. Closes on the next click anywhere. */
    offerTags(beside, available, vocabulary, chosen) {
        document.querySelectorAll('.tag-picker').forEach(picker => picker.remove())
        if (available.length === 0) return
        const picker = document.createElement('span')
        picker.className = 'tag-picker'
        for (const tag of available) {
            const option = this.tagChip(tag, vocabulary.get(tag), null)
            option.classList.add('tag-option')
            option.addEventListener('click', (event) => {
                event.stopPropagation()
                picker.remove()
                chosen(tag)
            })
            picker.appendChild(option)
        }
        beside.parentElement.appendChild(picker)
        const dismiss = () => { picker.remove(); document.removeEventListener('click', dismiss) }
        setTimeout(() => document.addEventListener('click', dismiss), 0)
    }

    /**
     * Black or white, whichever can be read on this background.
     *
     * The document picks the colours and has no way to say what to write on them, so this is
     * worked out rather than asked for. Perceived brightness rather than a plain average: a
     * saturated blue and a saturated yellow average the same and want opposite ink.
     */
    readableOn(colour) {
        try {
            const probe = document.createElement('span')
            probe.style.color = colour
            document.body.appendChild(probe)
            const computed = getComputedStyle(probe).color
            probe.remove()
            const [r, g, b] = (computed.match(/\d+/g) || ['0', '0', '0']).map(Number)
            return (0.299 * r + 0.587 * g + 0.114 * b) > 140 ? '#101010' : '#f5f5f5'
        } catch (_) {
            return '#f5f5f5'
        }
    }

    createStringElement(node) {
        const container = document.createElement('div')
        container.className = 'overseer-field string-field'
        
        // Only show a label if a user-friendly label is provided (e.g., via a 'label' parameter)
        const labelText = this.getParameterValue(node, 'label');
        if (labelText) {
            const label = document.createElement('label')
            label.textContent = labelText
            container.appendChild(label)
        }
        
    const value = document.createElement('span')
    value.className = 'field-value'
        value.textContent = this.getNodeValue(node) || ''
        
        // Make it editable on double-click (respect mutable)
        value.addEventListener('dblclick', () => {
            const mode = this.getEffectiveMutableMode(node, value)
            if (mode === 'false') return
            if (mode === 'guarded') { try { value.setAttribute('data-guarded-edit','1') } catch(_) {} }
            this.makeFieldEditable(value, node)
        })
        
        container.appendChild(value)
        
    // Apply layout overrides (only explicit margins/padding; defaults handled for containers)
    this.applyLayoutStyles(container, node)
    // Apply default field styling if no explicit parameters are set
    this.applyFieldDefaultStyles(container, node)
        this.applyNodeStyles(container, node)
        return container
    }

    createTextElement(node) {
        const container = document.createElement('div')
        container.className = 'overseer-field text-field'
        
        // Only show a label if a user-friendly label is provided (e.g., via a 'label' parameter)
        const labelText = this.getParameterValue(node, 'label');
        if (labelText) {
            const label = document.createElement('label')
            label.textContent = labelText
            container.appendChild(label)
        }
        
        const value = document.createElement('div')
        value.className = 'field-value text-content'
        
    // Check if markdown is enabled for this text field. Default to true when not specified.
    const markdownParam = this.getParameterValue(node, 'markdown')
    const isMarkdownEnabled = (markdownParam === undefined || markdownParam === null) ? true : markdownParam === true
        const textContent = this.getNodeValue(node) || ''
        
        if (isMarkdownEnabled) {
            // Use markdown rendering
            value.innerHTML = this.renderMarkdown(textContent)
            value.classList.add('markdown-enabled')
        } else {
            // Plain text with line breaks preserved
            value.textContent = textContent
            value.style.whiteSpace = 'pre-wrap'
        }
        
        // Make it editable on double-click (respect mutable)
        value.addEventListener('dblclick', () => {
            const mode = this.getEffectiveMutableMode(node, value)
            if (mode === 'false') return
            if (mode === 'guarded') { try { value.setAttribute('data-guarded-edit','1') } catch(_) {} }
            const hasFormula = node?.parameters && typeof node.parameters.value === 'object' && node.parameters.value?.Formula !== undefined
            if (hasFormula) {
                // Always use formula editor when a formula exists
                this.makeFieldEditable(value, node, true)
                return
            }
            if (isMarkdownEnabled) {
                this.makeMarkdownFieldEditable(value, node)
            } else {
                this.makeFieldEditable(value, node, true)
            }
        })
        
        container.appendChild(value)
        
    // Apply layout overrides (only explicit margins/padding; defaults handled for containers)
    this.applyLayoutStyles(container, node)
    // Apply default field styling if no explicit parameters are set
    this.applyFieldDefaultStyles(container, node)
        this.applyNodeStyles(container, node)
        return container
    }

    createNumberElement(node) {
        const container = document.createElement('div')
        container.className = 'overseer-field number-field'
        
        // Only show a label if a user-friendly label is provided (e.g., via a 'label' parameter)
        const labelText = this.getParameterValue(node, 'label');
        if (labelText) {
            const label = document.createElement('label')
            label.textContent = labelText
            container.appendChild(label)
        }
        
    const value = document.createElement('span')
    value.className = 'field-value'
        // Support precision, prefix, suffix formatting for numeric nodes
        const rawVal = this.getNodeValue(node)
        const pref = this.getParameterValue(node, 'prefix') || ''
        const suf = this.getParameterValue(node, 'suffix') || ''
        const precRaw = this.getParameterValue(node, 'precision')
        // `format="trim"` makes `precision` a limit rather than a width: two portions of
        // something reads as "2", one and a half as "1.5". A count is not a measurement, and
        // writing it "2.00" says a precision nobody claimed.
        const numberFormat = (this.getParameterValue(node, 'format') || '').toString().toLowerCase()
        const fmtNumber = (v) => {
            if (v === null || v === undefined) return ''
            if (typeof v === 'string') {
                const t = v.trim()
                if (t === '') return ''
                // treat literal "Null" as empty
                if (t.toLowerCase() === 'null') return ''
            }
            // Try to coerce to number when possible
            let n = (typeof v === 'number') ? v : Number(v)
            if (!isNaN(n)) {
                const p = (precRaw === null || precRaw === undefined) ? undefined : parseInt(precRaw, 10)
                if (!isNaN(p) && p >= 0) {
                    const fixed = n.toFixed(p)
                    if (numberFormat === 'trim' && fixed.includes('.')) {
                        // The zeros, and the point if nothing is left after it.
                        return fixed.replace(/0+$/, '').replace(/\.$/, '')
                    }
                    return fixed
                }
                // No precision specified; render integers without decimals
                if (Number.isInteger(n)) return String(n)
                return String(n)
            }
            // Fallback to string
            return String(v)
        }
        value.textContent = `${pref}${fmtNumber(rawVal)}${suf}`
        
        // Make it editable on double-click (respect mutable)
        value.addEventListener('dblclick', () => {
            const mode = this.getEffectiveMutableMode(node, value)
            if (mode === 'false') return
            if (mode === 'guarded') { try { value.setAttribute('data-guarded-edit','1') } catch(_) {} }
            this.makeFieldEditable(value, node)
        })
        
        container.appendChild(value)
        
    // Apply layout overrides (only explicit margins/padding; defaults handled for containers)
    this.applyLayoutStyles(container, node)
        
    // Apply default field styling if no explicit parameters are set
    this.applyFieldDefaultStyles(container, node)
        this.applyNodeStyles(container, node)
        return container
    }

    createDateElement(node) {
        const container = document.createElement('div')
        container.className = 'overseer-field date-field'
        
        // Only show a label if a user-friendly label is provided (e.g., via a 'label' parameter)
        const labelText = this.getParameterValue(node, 'label');
        if (labelText) {
            const label = document.createElement('label')
            label.textContent = labelText
            container.appendChild(label)
        }
        
    const value = document.createElement('span')
    value.className = 'field-value'
        value.textContent = this.getNodeValue(node) || ''
        
        container.appendChild(value)
    // Apply layout overrides (only explicit margins/padding; defaults handled for containers)
    this.applyLayoutStyles(container, node)
        
    // Apply default field styling if no explicit parameters are set
    this.applyFieldDefaultStyles(container, node)
        this.applyNodeStyles(container, node)
        return container
    }

    createTimestampElement(node) {
        const container = document.createElement('div')
        container.className = 'overseer-field date-field'

        const labelText = this.getParameterValue(node, 'label');
        if (labelText) {
            const label = document.createElement('label')
            label.textContent = labelText
            container.appendChild(label)
        }

    const value = document.createElement('span')
    value.className = 'field-value'
        container.appendChild(value)

        const mode = (this.getParameterValue(node, 'mode') || '').toString().toLowerCase()
        const fmt = (this.getParameterValue(node, 'format') || '').toString().toLowerCase()

        // Helper to extract a raw RFC3339 timestamp string from node
        const extractTs = () => {
            const params = node.parameters || {}
            const pick = (x) => {
                if (!x) return null
                if (typeof x === 'string') return x
                if (typeof x === 'object') {
                    if (x.Timestamp !== undefined) return x.Timestamp
                    if (x.String !== undefined) return x.String
                    if (x.Date !== undefined) return `${x.Date}T00:00:00Z`
                }
                return null
            }
            // Prefer computed value when available
            if (params._computed_value !== undefined) {
                const v = params._computed_value
                const picked = pick(v)
                if (picked) return picked
            }
            if (params.value !== undefined) {
                const picked = pick(params.value)
                if (picked) return picked
            }
            return null
        }

        // Relative formatter borrowed from timer
        const formatRelative = (ms) => {
            const timerFormat = fmt
            const sign = ms >= 0 ? 1 : -1
            const absMs = Math.abs(ms)
            if (absMs <= 0) return timerFormat === 'seconds' ? '0' : '00:00'
            const totalSec = Math.ceil(absMs / 1000)
            const days = Math.floor(totalSec / 86400)
            const hrsTotal = Math.floor(totalSec / 3600)
            const hrs = Math.floor((totalSec % 86400) / 3600)
            const mins = Math.floor((totalSec % 3600) / 60)
            const secs = totalSec % 60
            const prefix = sign < 0 ? '-' : ''
            switch (timerFormat) {
                case 'seconds':
                    return prefix + String(totalSec)
                case 'hh:mm:ss':
                    return prefix + `${String(hrsTotal).padStart(2,'0')}:${String(mins).padStart(2,'0')}:${String(secs).padStart(2,'0')}`
                case 'mm:ss':
                case '':
                    if (days > 0) return prefix + `${days}d ${hrs}h ${String(mins).padStart(2,'0')}:${String(secs).padStart(2,'0')}`
                    if (hrsTotal > 0) return prefix + `${String(hrsTotal).padStart(2,'0')}:${String(mins).padStart(2,'0')}:${String(secs).padStart(2,'0')}`
                    return prefix + `${String(mins).padStart(2,'0')}:${String(secs).padStart(2,'0')}`
                case 'long':
                    return prefix + (days > 0
                        ? `${days} day${days>1?'s':''} ${hrs} hour${hrs!==1?'s':''} ${String(mins).padStart(2,'0')}:${String(secs).padStart(2,'0')}`
                        : (hrsTotal > 0
                            ? `${String(hrsTotal).padStart(2,'0')}:${String(mins).padStart(2,'0')}:${String(secs).padStart(2,'0')}`
                            : `${String(mins).padStart(2,'0')}:${String(secs).padStart(2,'0')}`))
                default:
                    if (days > 0) return prefix + `${days}d ${hrs}h ${String(mins).padStart(2,'0')}:${String(secs).padStart(2,'0')}`
                    if (hrsTotal > 0) return prefix + `${String(hrsTotal).padStart(2,'0')}:${String(mins).padStart(2,'0')}:${String(secs).padStart(2,'0')}`
                    return prefix + `${String(mins).padStart(2,'0')}:${String(secs).padStart(2,'0')}`
            }
        }

        let intervalId = null
        const update = () => {
            const ts = extractTs()
            if (!ts) { value.textContent = '\u2014'; return }
            if (!mode) {
                // Absolute formatting
                value.textContent = this.formatTimestampValue(node, ts) || ''
                return
            }
            const due = Date.parse(ts)
            if (isNaN(due)) { value.textContent = '\u2014'; return }
            if (mode === 'elapsed') {
                value.textContent = formatRelative(Date.now() - due)
            } else if (mode === 'remaining') {
                value.textContent = formatRelative(due - Date.now())
            } else {
                value.textContent = this.formatTimestampValue(node, ts) || ''
            }
        }
        update()
        if (mode === 'elapsed' || mode === 'remaining') {
            intervalId = this._registerInterval(setInterval(update, 1000))
        }

    this.applyLayoutStyles(container, node)
    this.applyFieldDefaultStyles(container, node)
        this.applyNodeStyles(container, node)
        return container
    }

    // Timer: show remaining time until 'at', updating live; optional label/format params
    createTimerElement(node) {
        const container = document.createElement('div')
        container.className = 'overseer-field timer-field'

        const labelText = this.getParameterValue(node, 'label')
        if (labelText) {
            const label = document.createElement('label')
            label.textContent = labelText
            container.appendChild(label)
        }

        const value = document.createElement('span')
        value.className = 'field-value'
        value.textContent = ''
        container.appendChild(value)

        const params = node.parameters || {}
        const extractAt = () => {
            const comp = params._computed_at
            const raw = params.at
            const pick = (x) => {
                if (!x) return null
                if (typeof x === 'string') return x
                if (typeof x === 'object') {
                    if (x.Timestamp) return x.Timestamp
                    if (x.String) return x.String
                    if (x.Date) return `${x.Date}T00:00:00Z`
                }
                return null
            }
            return pick(comp) || pick(raw)
        }

        const timerFormat = (this.getParameterValue(node, 'format') || '').toString().toLowerCase()
        const elapsedMode = (this.getParameterValue(node, 'mode') || '').toString().toLowerCase() === 'elapsed'
        const getOffsetMs = () => {
            const raw = this.getParameterValue(node, 'offset')
            if (raw === null || raw === undefined) return 0
            if (typeof raw === 'number') return Math.floor(raw * 1000)
            const s = String(raw).trim().toLowerCase()
            if (s.endsWith('ms')) return parseInt(s.slice(0, -2), 10) || 0
            if (s.endsWith('s')) return (parseInt(s.slice(0, -1), 10) || 0) * 1000
            if (s.endsWith('m')) return (parseInt(s.slice(0, -1), 10) || 0) * 60_000
            if (s.endsWith('h')) return (parseInt(s.slice(0, -1), 10) || 0) * 3_600_000
            if (s.endsWith('d')) return (parseInt(s.slice(0, -1), 10) || 0) * 86_400_000
            const n = parseInt(s, 10); if (!isNaN(n)) return n * 1000
            return 0
        }

        const formatRemaining = (ms) => {
            const sign = ms >= 0 ? 1 : -1
            const absMs = Math.abs(ms)
            if (absMs <= 0) return timerFormat === 'seconds' ? '0' : '00:00'
            const totalSec = Math.ceil(absMs / 1000)
            const days = Math.floor(totalSec / 86400)
            const hrsTotal = Math.floor(totalSec / 3600)
            const hrs = Math.floor((totalSec % 86400) / 3600)
            const mins = Math.floor((totalSec % 3600) / 60)
            const secs = totalSec % 60
            const prefix = sign < 0 ? '-' : ''
            switch (timerFormat) {
                case 'seconds':
                    return prefix + String(totalSec)
                case 'hh:mm:ss':
                    return prefix + `${String(hrsTotal).padStart(2,'0')}:${String(mins).padStart(2,'0')}:${String(secs).padStart(2,'0')}`
                case 'mm:ss':
                case '': // default concise
                    if (days > 0) return prefix + `${days}d ${hrs}h ${String(mins).padStart(2,'0')}:${String(secs).padStart(2,'0')}`
                    if (hrsTotal > 0) return prefix + `${String(hrsTotal).padStart(2,'0')}:${String(mins).padStart(2,'0')}:${String(secs).padStart(2,'0')}`
                    return prefix + `${String(mins).padStart(2,'0')}:${String(secs).padStart(2,'0')}`
                case 'long':
                    return prefix + (days > 0
                        ? `${days} day${days>1?'s':''} ${hrs} hour${hrs!==1?'s':''} ${String(mins).padStart(2,'0')}:${String(secs).padStart(2,'0')}`
                        : (hrsTotal > 0
                            ? `${String(hrsTotal).padStart(2,'0')}:${String(mins).padStart(2,'0')}:${String(secs).padStart(2,'0')}`
                            : `${String(mins).padStart(2,'0')}:${String(secs).padStart(2,'0')}`))
                default:
                    if (days > 0) return prefix + `${days}d ${hrs}h ${String(mins).padStart(2,'0')}:${String(secs).padStart(2,'0')}`
                    if (hrsTotal > 0) return prefix + `${String(hrsTotal).padStart(2,'0')}:${String(mins).padStart(2,'0')}:${String(secs).padStart(2,'0')}`
                    return prefix + `${String(mins).padStart(2,'0')}:${String(secs).padStart(2,'0')}`
            }
        }

        const atStr = extractAt()
        let intervalId = null
        const update = () => {
            // Respect active flag: show nothing when inactive
            const activeParam = this.getParameterValue(node, 'active')
            const isActive = activeParam === true || String(activeParam).toLowerCase() === 'true'
            if (!isActive) { 
                const placeholder = this.getParameterValue(node, 'placeholder')
                value.textContent = (placeholder !== null && placeholder !== undefined) ? String(placeholder) : '\u2014'
                return 
            }
            if (!atStr) { value.textContent = '—'; return }
            const due = Date.parse(atStr)
            if (isNaN(due)) { value.textContent = '—'; return }
            if (elapsedMode) {
                const elapsed = Date.now() - due
                value.textContent = formatRemaining(elapsed)
            } else {
                const rem = (due + getOffsetMs()) - Date.now()
                value.textContent = formatRemaining(rem)
            }
        }
    update()
    intervalId = this._registerInterval(setInterval(update, 1000))

    this.applyLayoutStyles(container, node)
    this.applyFieldDefaultStyles(container, node)
        this.applyNodeStyles(container, node)
        return container
    }

    createBooleanElement(node) {
        const container = document.createElement('div')
        container.className = 'overseer-field boolean-field'
        
        // Only show a label if a user-friendly label is provided (e.g., via a 'label' parameter)
        // Through the accessor, like everywhere else: the parameter arrives as
        // { String: "Irregular" }, and setting that as text gives "[object Object]".
        const labelText = this.getParameterValue(node, 'label');
        if (labelText) {
            const label = document.createElement('label')
            label.textContent = labelText
            container.appendChild(label)
        }
        
        const checkbox = document.createElement('input')
        checkbox.type = 'checkbox'
        checkbox.checked = this.getNodeValue(node) === 'true' || this.getNodeValue(node) === true

        container.appendChild(checkbox)

        // Handle changes (respect mutability)
        checkbox.addEventListener('change', () => {
            const mode = this.getEffectiveMutableMode(node, checkbox)
            if (mode === 'false') {
                // Revert UI toggle to the current node value
                const current = this.getNodeValue(node) === 'true' || this.getNodeValue(node) === true
                if (checkbox.checked !== current) checkbox.checked = current
                return
            }
            if (mode === 'guarded') { try { checkbox.setAttribute('data-guarded-edit','1') } catch(_) {} }
            const was = this.getNodeValue(node)
            this.updateNodeValue(node, checkbox.checked)
            if (window.app && window.app.markDocumentModified) {
                window.app.markDocumentModified()
            }
            if (window.app && window.app.reevaluateDocumentSelective) {
                // Try to determine field path for selective update
                try {
                    const fieldPath = this.buildNodePath(container).join('/')
                    // With the value, or the save writes text that never saw this - see the
                    // note in `createTagsElement`, which had the same fault.
                    window.app.reevaluateDocumentSelective(
                        [fieldPath],
                        [{ path: fieldPath, oldValue: was, newValue: checkbox.checked }]
                    )
                } catch (e) {
                    if (DEBUG_MODE) console.warn('Failed to build field path, falling back to full update:', e)
                    window.app.reevaluateDocumentSelective([])
                }
            }
            // Mark guarded flag on target if applicable so serializer may skip it
            try {
                if (mode === 'guarded') {
                    const fieldPath = this.buildNodePath(container).join('/')
                    const target = this.findNodeByPath(window.app.currentDocument, fieldPath.split('/'))
                    if (target) {
                        if (!target.parameters) target.parameters = {}
                        target.parameters._guarded_edit = { Boolean: true }
                    }
                    try { checkbox.removeAttribute('data-guarded-edit') } catch(_) {}
                }
            } catch(_) { /* best-effort */ }
        })
        
    // Apply layout overrides (only explicit margins/padding; defaults handled for containers)
    this.applyLayoutStyles(container, node)
    // Apply default field styling if no explicit parameters are set
    this.applyFieldDefaultStyles(container, node)
        this.applyNodeStyles(container, node)
        return container
    }

    createButtonElement(node) {
        const button = document.createElement('button')
        button.className = 'overseer-button'
        // Support icon-only or icon+label buttons
        const icon = (this.getParameterValue(node, 'icon') || '').toString().trim().toLowerCase()
        const labelText = this.getParameterValue(node, 'label')
        if (icon && (!labelText || String(labelText).trim() === '')) {
            // Icon-only button
            button.classList.add('icon-button')
            const span = document.createElement('span')
            span.className = `icon glyph-${icon}`
            span.setAttribute('aria-hidden', 'true')
            button.appendChild(span)
            // Set tooltip from node name or icon name for accessibility
            const title = node.name || icon
            if (title) button.title = String(title)
        } else {
            // Label (and optional leading icon)
            if (icon) {
                const span = document.createElement('span')
                span.className = `icon glyph-${icon}`
                span.setAttribute('aria-hidden', 'true')
                // small spacing between icon and text
                span.style.marginRight = '6px'
                button.appendChild(span)
            }
            const labelSpan = document.createElement('span')
            labelSpan.className = 'overseer-button-label'
            // Button label comes from explicit 'label' parameter; do not use node name
            labelSpan.textContent = labelText ? String(labelText) : (icon ? '' : '')
            button.appendChild(labelSpan)
        }
        // Guard: ensure no internal children (like event handlers/actions) are appended
        // If any children exist, they are non-visual and handled by emitEvent only
        // We intentionally do not render node.children for buttons
        // Wire to backend actions on click (path is read from dataset set by renderNode)
        button.addEventListener('click', async () => {
            try {
                await this.emitEvent(node, button, 'click')
            } catch (err) {
                console.warn('Action execution failed:', err)
            }
        })
    // Honor explicit margin/padding on buttons (do not apply container defaults)
    this.applyLayoutStyles(button, node)
    this.applyNodeStyles(button, node)
        return button
    }

    createCheckboxElement(node) {
        const container = document.createElement('div')
        container.className = 'overseer-field checkbox-field'

        const checkbox = document.createElement('input')
        checkbox.type = 'checkbox'
        checkbox.checked = this.getNodeValue(node) === 'true' || this.getNodeValue(node) === true

        // Only show a label if a user-friendly label is provided (e.g., via a 'label' parameter)
        const labelText = this.getParameterValue(node, 'label');
        if (labelText) {
            const label = document.createElement('label')
            label.appendChild(checkbox)
            label.appendChild(document.createTextNode(labelText))
            container.appendChild(label)
        } else {
            // Just the checkbox without any label for unnamed or internal checkboxes
            container.appendChild(checkbox)
        }

        // Handle checkbox changes with default events: toggle, check, uncheck (respect mutability)
        checkbox.addEventListener('change', async () => {
            const mode = this.getEffectiveMutableMode(node, checkbox)
            if (mode === 'false') {
                // Revert UI toggle to the current node value
                const current = this.getNodeValue(node) === 'true' || this.getNodeValue(node) === true
                if (checkbox.checked !== current) checkbox.checked = current
                return
            }
            if (mode === 'guarded') { try { checkbox.setAttribute('data-guarded-edit','1') } catch(_) {} }
            if (DEBUG_MODE) console.log('Checkbox changed:', node.name, checkbox.checked)
            const was = this.getNodeValue(node)
            this.updateNodeValue(node, checkbox.checked)

            // Mark document as modified
            if (window.app && window.app.markDocumentModified) {
                window.app.markDocumentModified()
            }
            // Trigger reevaluation so formulas/computed params refresh
            if (window.app && window.app.reevaluateDocumentSelective) {
                // Try to determine field path for selective update
                try {
                    const fieldPath = this.buildNodePath(container).join('/')
                    // With the value - see the note in `createTagsElement`.
                    window.app.reevaluateDocumentSelective(
                        [fieldPath],
                        [{ path: fieldPath, oldValue: was, newValue: checkbox.checked }]
                    )
                } catch (e) {
                    if (DEBUG_MODE) console.warn('Failed to build field path, falling back to full update:', e)
                    window.app.reevaluateDocumentSelective([])
                }
            }
            // Determine which events are defined to avoid unnecessary backend calls
            const hasHandler = (evt) => Array.isArray(node.children) && node.children.some(c => (c.name||'').toLowerCase() === evt)
            const eventsToEmit = ['toggle']
            if (checkbox.checked) eventsToEmit.push('check')
            else eventsToEmit.push('uncheck')
            for (const evt of eventsToEmit) {
                if (hasHandler(evt)) {
                    try { await this.emitEvent(node, checkbox, evt) } catch(_) {}
                }
            }
            // Also keep legacy 'change' if present
            if (hasHandler('change')) {
                try { await this.emitEvent(node, checkbox, 'change') } catch(_) {}
            }
            // Mark guarded flag on target if applicable so serializer may skip it
            try {
                if (mode === 'guarded') {
                    const fieldPath = this.buildNodePath(container).join('/')
                    const target = this.findNodeByPath(window.app.currentDocument, fieldPath.split('/'))
                    if (target) {
                        if (!target.parameters) target.parameters = {}
                        target.parameters._guarded_edit = { Boolean: true }
                    }
                    try { checkbox.removeAttribute('data-guarded-edit') } catch(_) {}
                }
            } catch(_) { /* best-effort */ }
        })

    // Apply layout overrides (only explicit margins/padding; defaults handled for containers)
    this.applyLayoutStyles(container, node)
    // Apply default field styling if no explicit parameters are set
    this.applyFieldDefaultStyles(container, node)
        this.applyNodeStyles(container, node)
        return container
    }

    createChartElement(node) {
        const container = document.createElement('div')
        container.className = 'overseer-chart'
        
        // Create canvas for Chart.js
        const canvas = document.createElement('canvas')
        container.appendChild(canvas)
        
        // Size handling - support width/height parameters.
        //
        // Through the css converter: `260px` reaches here as { Pixels: 260 }, which the size
        // parser below turns into "[object Object]" and quietly discards in favour of the
        // container's width. Percentages happen to arrive as strings, which is why charts sized
        // that way have always worked and ones sized in pixels never did.
        const asCss = (v) => (v === null || v === undefined ? v : this.convertCssSizeValue(v))
        const rawW = asCss(this.getParameterValue(node, 'width'))
        const rawH = asCss(this.getParameterValue(node, 'height'))
        const rawAR = this.getParameterValue(node, 'aspect-ratio')
        
        const parseAspectRatio = (v) => {
            if (v === null || v === undefined) return null
            if (typeof v === 'number') return v > 0 ? Number(v) : null
            const s = String(v).trim()
            if (!s) return null
            // Formats: "16:9", "4/3", "1.777", "1:1"
            if (s.includes(':') || s.includes('/')) {
                const sep = s.includes(':') ? ':' : '/'
                const [a, b] = s.split(sep)
                const na = parseFloat(a), nb = parseFloat(b)
                if (!isNaN(na) && !isNaN(nb) && nb > 0) return na / nb
                return null
            }
            const n = parseFloat(s)
            return isNaN(n) || n <= 0 ? null : n
        }
        
        const aspect = parseAspectRatio(rawAR) // width/height
        
        const parseDim = (raw, parentPx, fallback) => {
            if (raw === null || raw === undefined) return fallback
            if (typeof raw === 'number') return Math.max(10, Math.floor(raw))
            const s = String(raw).trim()
            if (s.endsWith('%')) {
                const p = parseFloat(s.slice(0, -1))
                if (!isNaN(p)) return Math.max(10, Math.floor((parentPx * p) / 100))
            }
            const n = parseInt(s, 10)
            return isNaN(n) ? fallback : Math.max(10, n)
        }
        
        let width = 400, height = 200
        
        const computeSize = () => {
            // Use the container's content box as the reference for percentage sizes
            const rect = container.getBoundingClientRect()
            const parentW = container.parentElement ? container.parentElement.clientWidth : 0
            const viewportW = Math.max(document.documentElement.clientWidth || 0, window.innerWidth || 0)
            const pw = Math.max(10, Math.floor(container.clientWidth || parentW || rect.width || viewportW || 0))
            const phMeasured = Math.max(0, Math.floor(container.clientHeight || rect.height || 0))
            
            width = parseDim(rawW, pw, pw || 400)
            const defaultH = aspect ? Math.max(10, Math.round(width / aspect)) : Math.max(200, Math.round(width * 0.5))
            
            // If height is %, but parent height is 0/unknown, fall back to aspect/default
            const isPercentH = typeof rawH === 'string' && rawH.trim().endsWith('%')
            if (rawH === null || rawH === undefined) {
                height = defaultH
            } else if (isPercentH && phMeasured <= 1) {
                height = defaultH
            } else {
                height = parseDim(rawH, phMeasured, defaultH)
            }
            
            // Cap width to container (avoid overflow on 100%) and enforce a sensible floor
            width = Math.max(50, Math.min(width, pw))
            // Only cap height if we have a measurable container height (non-zero)
            if (phMeasured > 1) height = Math.max(50, Math.min(height, phMeasured))
            else height = Math.max(50, height)
            
            // Set CSS size
            canvas.style.width = width + 'px'
            canvas.style.height = height + 'px'
        }
        
        computeSize()

        // A chart given a size in pixels keeps it. The stylesheet stretches a canvas to its
        // container, which is right for a chart that owns a row and wrong for one placed beside
        // other things: in a horizontal group the container has no width of its own, so it
        // collapses and takes the chart with it - a bordered box with nothing in it.
        const isPixelSize = (v) => v !== null && v !== undefined && !String(v).trim().endsWith('%')
        if (isPixelSize(rawW) || isPixelSize(rawH)) {
            container.classList.add('sized')
            container.style.flex = '0 0 auto'
            if (isPixelSize(rawW)) container.style.width = width + 'px'
            if (isPixelSize(rawH)) container.style.height = height + 'px'
        }

        // Get plot children
        const plotsAll = (node.children || []).filter(c => (c.node_type||'').toLowerCase() === 'plot')

        // A pie is a different shape of question: not a series over an axis, but a handful of
        // parts of one whole. Each plot contributes a single `amount` and its own colour, and
        // the axes and domains below have nothing to say about it.
        const kind = (this.getParameterValue(node, 'kind') || 'line').toString().toLowerCase()
        if (kind === 'pie' || kind === 'doughnut') {
            const labels = []
            const values = []
            const colors = []
            for (const plot of plotsAll) {
                const raw = this.getParameterValue(plot, '_computed_amount')
                const amount = Number(raw !== undefined && raw !== null ? raw : this.getParameterValue(plot, 'amount'))
                if (!isFinite(amount) || amount <= 0) continue
                labels.push(this.getParameterValue(plot, 'label') || plot.name || 'Slice')
                values.push(amount)
                colors.push(this.convertColorValue(this.getParameterValue(plot, 'color') || '#4A90E2'))
            }
            if (values.length === 0) return container

            const pie = new Chart(canvas, {
                type: kind,
                data: {
                    labels,
                    datasets: [{ data: values, backgroundColor: colors, borderWidth: 0 }],
                },
                options: {
                    responsive: true,
                    maintainAspectRatio: false,
                    plugins: {
                        legend: { display: true, position: 'right', labels: { usePointStyle: true, boxWidth: 8 } },
                        tooltip: {
                            callbacks: {
                                // The number itself is rarely the point; the share is.
                                label: (item) => {
                                    const total = values.reduce((a, b) => a + b, 0)
                                    const share = total > 0 ? Math.round((item.parsed / total) * 100) : 0
                                    return `${item.label}: ${Math.round(item.parsed)} (${share}%)`
                                },
                            },
                        },
                    },
                },
            })
            canvas._chartInstance = pie
            container._chartInstance = pie
            return container
        }

        // A bar chart against a limit. Not a series and not parts of a whole: a handful of
        // figures that are each supposed to stay under a number of their own.
        //
        // The bars are normalised, so what is drawn is the *share of the limit used* rather than
        // the figure itself. That is the whole point of it: salt in grams and caffeine in
        // milligrams differ by a factor of a thousand, and on a shared axis the salt bar is
        // invisible and the chart says nothing. Against their own limits they are comparable,
        // and the limit line is the one thing worth reading - a bar over it is over it, whatever
        // the units were.
        //
        // The real figures are kept for the tooltip, because "84% of the salt you are allowed"
        // is the reading and "4.2 g of 5 g" is the fact behind it.
        if (kind === 'bar') {
            const labels = []
            const shares = []
            const colors = []
            const facts = []
            for (const plot of plotsAll) {
                const rawAmount = this.getParameterValue(plot, '_computed_amount')
                const amount = Number(rawAmount !== undefined && rawAmount !== null
                    ? rawAmount : this.getParameterValue(plot, 'amount'))
                const rawLimit = this.getParameterValue(plot, '_computed_limit')
                const limit = Number(rawLimit !== undefined && rawLimit !== null
                    ? rawLimit : this.getParameterValue(plot, 'limit'))
                // A plot with no limit has nothing to be a share of. Skipped rather than drawn
                // against the others, which would put it on an axis it does not belong to.
                if (!isFinite(amount) || !isFinite(limit) || limit <= 0) continue
                const suffix = (this.getParameterValue(plot, 'suffix') || '').toString()
                const share = amount / limit
                labels.push(this.getParameterValue(plot, 'label') || plot.name || 'Bar')
                shares.push(share)
                // Over the limit is the thing the chart exists to show, so it is not left to the
                // reader to compare a bar against a line: it changes colour.
                colors.push(share > 1
                    ? this.convertColorValue('#e63e11')
                    : this.convertColorValue(this.getParameterValue(plot, 'color') || '#4A90E2'))
                facts.push({ amount, limit, suffix })
            }
            if (shares.length === 0) return container

            // How tall to make the axis, which is the one real design decision here.
            //
            // Not simply the tallest bar: on real days the sugar figure reaches four times its
            // limit, and an axis that fits it leaves the other four bars a few pixels high and
            // the limit line down in the noise. Not a fixed ceiling either, or a day when
            // everything came in under would draw five stubs against a line near the top.
            //
            // So: always leave room above the line, always keep the line in the upper half, and
            // stop growing at three times over. A bar past that is clipped, stays red, and says
            // the real figure when pointed at - by then "a lot" is the whole reading anyway.
            const tallest = Math.max(1.5, Math.min(3, Math.max(...shares) * 1.1))

            const bars = new Chart(canvas, {
                type: 'bar',
                data: {
                    labels,
                    datasets: [
                        {
                            data: shares,
                            backgroundColor: colors,
                            borderWidth: 0,
                            order: 2,
                        },
                        {
                            // The limit, as a line across every bar. A dataset rather than an
                            // annotation because the annotation plugin is not loaded, and one
                            // flat dataset says the same thing with nothing new to register.
                            type: 'line',
                            data: labels.map(() => 1),
                            borderColor: 'rgba(230, 62, 17, 0.75)',
                            borderWidth: 1,
                            borderDash: [4, 3],
                            pointRadius: 0,
                            fill: false,
                            order: 1,
                        },
                    ],
                },
                options: {
                    responsive: true,
                    maintainAspectRatio: false,
                    scales: {
                        x: { grid: { display: false }, ticks: { font: { size: 10 } } },
                        y: {
                            beginAtZero: true,
                            max: tallest,
                            grid: { display: false },
                            ticks: {
                                font: { size: 10 },
                                // As shares of the limit, since that is what the heights are.
                                callback: (value) => Math.round(value * 100) + '%',
                            },
                        },
                    },
                    plugins: {
                        legend: { display: false },
                        tooltip: {
                            filter: (item) => item.datasetIndex === 0,
                            callbacks: {
                                label: (item) => {
                                    const fact = facts[item.dataIndex]
                                    if (!fact) return item.label
                                    const round = (n) => (n >= 100 ? Math.round(n) : Math.round(n * 10) / 10)
                                    const share = Math.round(item.parsed.y * 100)
                                    return `${item.label}: ${round(fact.amount)}${fact.suffix} of `
                                        + `${round(fact.limit)}${fact.suffix} (${share}%)`
                                },
                            },
                        },
                    },
                },
            })
            canvas._chartInstance = bars
            container._chartInstance = bars
            return container
        }


        
        // Check if we have computed series data
        const hasData = plotsAll.some(plot => {
            const seriesJson = this.getParameterValue(plot, '_computed_series')
            if (!seriesJson) return false
            try {
                const series = JSON.parse(seriesJson)
                return Array.isArray(series) && series.length > 0
            } catch (_) {
                return false
            }
        })
        
        if (!hasData) {
            // Show placeholder when no data is available
            const ctx = canvas.getContext('2d')
            ctx.fillStyle = '#666'
            ctx.font = '14px system-ui, Arial'
            ctx.fillText('Chart: ' + (node.name || 'Unnamed'), 10, 30)
            ctx.fillText('(no series yet — add plot nodes with source/x/y)', 10, 50)
            this.applyNodeStyles(container, node)
            return container
        }
        
        // Prepare datasets for Chart.js
        const datasets = []
        const allDataPoints = []
        
        for (const plot of plotsAll) {
            const seriesJson = this.getParameterValue(plot, '_computed_series')
            if (!seriesJson) continue
            
            let series
            try {
                series = JSON.parse(seriesJson)
            } catch (_) {
                continue
            }
            
            if (!Array.isArray(series) || series.length === 0) continue
            
            // Convert series data to Chart.js format
            const data = series.map(([x, y]) => ({ x: Number(x), y: Number(y) }))
            allDataPoints.push(...data)
            
            // Get plot styling
            const color = this.convertColorValue(this.getParameterValue(plot, 'color') || '#4A90E2')
            const label = this.getParameterValue(plot, 'label') || plot.name || 'Series'
            
            datasets.push({
                label: label,
                data: data,
                borderColor: color,
                backgroundColor: color + '20', // Add transparency for fill
                borderWidth: 2,
                fill: false,
                tension: 0.1,
                pointRadius: 3,
                pointHoverRadius: 5
            })
        }
        
        // Determine if X axis looks like time data
        const xValues = allDataPoints.map(p => p.x)
        const xLooksLikeTime = xValues.length > 0 && xValues.every(x => Math.abs(x) > 1e10)
        
        // Get explicit domain bounds if provided
        let xmin = parseFloat(this.getParameterValue(node, 'domain-x-min'))
        let xmax = parseFloat(this.getParameterValue(node, 'domain-x-max'))
        let ymin = parseFloat(this.getParameterValue(node, 'domain-y-min'))
        let ymax = parseFloat(this.getParameterValue(node, 'domain-y-max'))
        
        // If not explicit, try computed bounds
        if (isNaN(xmin)) xmin = parseFloat(this.getParameterValue(node, '_computed_x_min'))
        if (isNaN(xmax)) xmax = parseFloat(this.getParameterValue(node, '_computed_x_max'))
        if (isNaN(ymin)) ymin = parseFloat(this.getParameterValue(node, '_computed_y_min'))
        if (isNaN(ymax)) ymax = parseFloat(this.getParameterValue(node, '_computed_y_max'))
        
        // Chart.js configuration
        const config = {
            type: 'line',
            data: {
                datasets: datasets
            },
            options: {
                responsive: true,
                maintainAspectRatio: false,
                interaction: {
                    intersect: false,
                    mode: 'index'
                },
                plugins: {
                    legend: {
                        display: true,
                        position: 'top',
                        labels: {
                            usePointStyle: true,
                            padding: 20,
                            color: '#cccccc'
                        }
                    },
                    tooltip: {
                        backgroundColor: 'rgba(0, 0, 0, 0.8)',
                        titleColor: '#ffffff',
                        bodyColor: '#ffffff',
                        borderColor: '#333333',
                        borderWidth: 1,
                        callbacks: {
                            title: function(tooltipItems) {
                                const item = tooltipItems[0]
                                if (xLooksLikeTime) {
                                    // Format timestamp as readable date
                                    return new Date(item.parsed.x).toLocaleString()
                                }
                                return item.label || item.parsed.x
                            }
                        }
                    }
                },
                scales: {
                    x: {
                        type: 'linear',
                        display: true,
                        title: {
                            display: false
                        },
                        grid: {
                            color: '#333333'
                        },
                        ticks: {
                            color: '#cccccc',
                            callback: function(value) {
                                if (xLooksLikeTime) {
                                    // Format timestamp ticks as dates
                                    return new Date(value).toLocaleDateString()
                                }
                                return value
                            }
                        }
                    },
                    y: {
                        type: 'linear',
                        display: true,
                        title: {
                            display: false
                        },
                        grid: {
                            color: '#333333'
                        },
                        ticks: {
                            color: '#cccccc'
                        }
                    }
                }
            }
        }
        
        // Apply explicit bounds if provided
        if (!isNaN(xmin) && !isNaN(xmax)) {
            config.options.scales.x.min = xmin
            config.options.scales.x.max = xmax
        }
        if (!isNaN(ymin) && !isNaN(ymax)) {
            config.options.scales.y.min = ymin
            config.options.scales.y.max = ymax
        }
        
        // Apply chart background color if specified
        const bgColor = this.getParameterValue(node, 'background-color')
        if (bgColor) {
            container.style.backgroundColor = this.convertColorValue(bgColor)
        }
        
        // Animate a chart the first time it appears, and not afterwards.
        //
        // A re-render destroys the Chart.js instance and builds a new one, which replays the
        // entry animation from scratch. That reads as a flourish on load and as a distraction
        // on every subsequent edit - including edits that change nothing the chart plots.
        try {
            const chartKey = Array.isArray(node.__overseer_path)
                ? node.__overseer_path.join('/')
                : (node.name || node.node_type || 'chart')
            if (!this._animatedCharts) this._animatedCharts = new Set()
            if (this._animatedCharts.has(chartKey)) {
                if (!config.options) config.options = {}
                config.options.animation = false
            } else {
                this._animatedCharts.add(chartKey)
            }
        } catch (e) {
            if (DEBUG_MODE) console.warn('Chart animation gating failed:', e)
        }

        // Create Chart.js instance
        let chartInstance = null
        try {
            chartInstance = new Chart(canvas, config)
        } catch (error) {
            console.error('Failed to create Chart.js instance:', error)
            // Fallback to canvas text
            const ctx = canvas.getContext('2d')
            ctx.fillStyle = '#cc6666'
            ctx.font = '14px system-ui, Arial'
            ctx.fillText('Chart Error: ' + error.message, 10, 30)
            this.applyNodeStyles(container, node)
            return container
        }
        
        // Handle resize
        const resizeObserver = new ResizeObserver(() => {
            computeSize()
            if (chartInstance) {
                chartInstance.resize()
            }
        })
        resizeObserver.observe(container)
        
        // Store chart instance for cleanup
        container._chartInstance = chartInstance
        container._resizeObserver = resizeObserver
        
        this.applyNodeStyles(container, node)
        return container
    }

    getEffectiveLayout(node) {
        // First check if resolver calculated an effective layout
        if (node.parameters && node.parameters._effective_layout) {
            return this.getParameterValue(node, '_effective_layout') || 'vertical'
        }
        
        // Fall back to explicit layout parameter or default
        return this.getParameterValue(node, 'layout') || 'vertical'
    }

    applyLayoutStyles(element, node) {
        if (!node.parameters) return
        
          // Check if user wants zero spacing/margin layout (tight grid)
        const spacing = this.getParameterValue(node, 'spacing')
        const margin = this.getParameterValue(node, 'margin')
        const isTightLayout = (spacing === 0 || margin === 0)
        // Mark tight layout so CSS rules can respect it (e.g., suppress top breathing room)
        try {
            if (isTightLayout) {
                element.setAttribute('data-tight', '1')
            } else {
                element.removeAttribute('data-tight')
            }
        } catch (_) { /* no-op */ }
        // Apply cross-axis alignment if provided (near|center|far)
        const layout = this.getEffectiveLayout(node)
        const align = (this.getParameterValue(node, '_effective_alignment') || this.getParameterValue(node, 'alignment') || '').toString().toLowerCase()
        const mapAlign = (a) => (a === 'center' ? 'center' : (a === 'far' ? 'flex-end' : (a === 'near' ? 'flex-start' : '')))
        const crossAlign = mapAlign(align)
        if (crossAlign) {
            if (layout === 'horizontal') {
                // horizontal layout => align vertically
                element.style.alignItems = crossAlign
            } else if (layout === 'vertical') {
                // vertical layout => align horizontally
                element.style.alignItems = crossAlign
            }
            // add class for CSS-based child align-self helpers
            if (['near','center','far'].includes(align)) {
                element.classList.remove('align-near','align-center','align-far')
                element.classList.add(`align-${align}`)
            }
        }

        // Helper to convert numeric to px, pass through strings
        const cssSize = (v) => {
            if (v === null || v === undefined) return undefined
            if (typeof v === 'number') return `${v}px`
            const s = String(v).trim()
            if (!s) return undefined
            return s
        }

        // Apply explicit margin/padding if provided
        const hasExplicitMargin = node.parameters.margin !== undefined ||
                                  node.parameters['margin-top'] !== undefined ||
                                  node.parameters['margin-bottom'] !== undefined ||
                                  node.parameters['margin-left'] !== undefined ||
                                  node.parameters['margin-right'] !== undefined
        const hasExplicitPadding = node.parameters.padding !== undefined ||
                                   node.parameters['padding-top'] !== undefined ||
                                   node.parameters['padding-bottom'] !== undefined ||
                                   node.parameters['padding-left'] !== undefined ||
                                   node.parameters['padding-right'] !== undefined

        if (node.parameters.margin !== undefined) {
            const v = cssSize(node.parameters.margin)
            if (v !== undefined) element.style.margin = v
        }
        if (node.parameters['margin-top'] !== undefined) {
            const v = cssSize(node.parameters['margin-top']); if (v !== undefined) element.style.marginTop = v
        }
        if (node.parameters['margin-bottom'] !== undefined) {
            const v = cssSize(node.parameters['margin-bottom']); if (v !== undefined) element.style.marginBottom = v
        }
        if (node.parameters['margin-left'] !== undefined) {
            const v = cssSize(node.parameters['margin-left']); if (v !== undefined) element.style.marginLeft = v
        }
        if (node.parameters['margin-right'] !== undefined) {
            const v = cssSize(node.parameters['margin-right']); if (v !== undefined) element.style.marginRight = v
        }

        if (node.parameters.padding !== undefined) {
            const v = cssSize(node.parameters.padding)
            if (v !== undefined) element.style.padding = v
        }
        if (node.parameters['padding-top'] !== undefined) {
            const v = cssSize(node.parameters['padding-top']); if (v !== undefined) element.style.paddingTop = v
        }
        if (node.parameters['padding-bottom'] !== undefined) {
            const v = cssSize(node.parameters['padding-bottom']); if (v !== undefined) element.style.paddingBottom = v
        }
        if (node.parameters['padding-left'] !== undefined) {
            const v = cssSize(node.parameters['padding-left']); if (v !== undefined) element.style.paddingLeft = v
        }
        if (node.parameters['padding-right'] !== undefined) {
            const v = cssSize(node.parameters['padding-right']); if (v !== undefined) element.style.paddingRight = v
        }

        // Apply defaults only if not explicitly set
        if (!hasExplicitMargin) {
            if (isTightLayout) {
                element.style.margin = '0px'
            } else {
                element.style.marginTop = '8px'
                element.style.marginBottom = '8px'
            }
        }
        if (!hasExplicitPadding) {
            const nodeTypeLower = ((node.node_type || node.type || '') + '').toLowerCase()
            // Keep button internal padding from CSS; only override if explicitly set
            if (nodeTypeLower !== 'button') {
                if (isTightLayout) {
                    element.style.padding = '0px'
                } else {
                    element.style.padding = '8px'
                }
                // Ours, not the document's. A table undoes this to get a row down to one line,
                // and must not undo a padding that was asked for - see `arrangeTable`.
                element.dataset.paddingIsDefault = '1'
            }
        }
    }

    applyNodeStyles(element, node) {
        if (!node.parameters) return
        
        // Apply basic styling parameters
        const params = node.parameters
        
        // Legacy background support (keep for compatibility) and computed background
        // Do not override background already set by renderNode's effectiveBg logic.
        if (!element.style.backgroundColor) {
            if (params.background) {
                element.style.backgroundColor = params.background
            } else {
                // New styling parameters (prefer computed values)
                const bgColor = this.getParameterValue(node, 'background-color')
                const hasComputedBg = !!(node?.parameters && node.parameters['_computed_background-color'] !== undefined)
                const hasRawFormulaBg = this.parameterHasFormula(node, 'background-color')
                if (bgColor !== null) {
                    // Avoid setting raw Formula as a CSS color; wait for computed value or inherit
                    if (!(hasRawFormulaBg && !hasComputedBg)) {
                        element.style.backgroundColor = this.convertColorValue(bgColor)
                    }
                }
            }
        }
        
        const fontColor = this.getParameterValue(node, 'font-color')
        if (fontColor !== null) {
            element.style.color = this.convertColorValue(fontColor)
            
            // Also apply font-color to any field-value children to override CSS class specificity
            const fieldValueElements = element.querySelectorAll('.field-value')
            fieldValueElements.forEach(fieldValue => {
                fieldValue.style.setProperty('color', this.convertColorValue(fontColor), 'important')
            })
        }
        
        const fontSize = this.getParameterValue(node, 'font-size')
        if (fontSize !== null) {
            element.style.setProperty('font-size', this.convertCssSizeValue(fontSize), 'important')
            
            // Also apply font-size to any field-value children to override CSS class specificity
            const fieldValueElements = element.querySelectorAll('.field-value')
            fieldValueElements.forEach(fieldValue => {
                fieldValue.style.setProperty('font-size', this.convertCssSizeValue(fontSize), 'important')
            })
        }
        
        // Width parameter
        if (params.width) {
            element.style.width = this.convertCssSizeValue(params.width)
        }
        
        // Height parameter  
        if (params.height) {
            element.style.height = this.convertCssSizeValue(params.height)
        }
        
        // Explicit overflow control parameters
        if (params['overflow-x']) {
            element.style.overflowX = params['overflow-x']
        }
        if (params['overflow-y']) {
            element.style.overflowY = params['overflow-y']
        }
        if (params.overflow) {
            element.style.overflow = params.overflow
        }
        
    // Border style parameter (prefer computed value)
    {
        let borderDisabled = false
            const borderParam = this.getParameterValue(node, 'border-style') ?? params['border-style']
            if (borderParam !== undefined) {
                // Special-case: explicit none should remove both border and shadow to avoid default look
                if (this.isBorderNone(borderParam)) {
                    element.style.setProperty('border', 'none', 'important')
                    element.style.setProperty('box-shadow', 'none', 'important')
            borderDisabled = true

                    // Also remove borders and shadows from common inner wrappers to avoid residual lines
                    try {
                        const innerSelectors = [
                            ':scope .field-value',
                            ':scope .overseer-list',
                            ':scope .overseer-list-value'
                        ]
                        const innerEls = element.querySelectorAll(innerSelectors.join(','))
                        innerEls.forEach(el => {
                            el.style.setProperty('border', 'none', 'important')
                            el.style.setProperty('box-shadow', 'none', 'important')
                        })
                    } catch (_) { /* no-op */ }
                } else {
                    const borderStyle = this.convertBorderStyleValue(borderParam)
                    if (borderStyle) {
                        element.style.border = borderStyle
                    }
                }
            }
        }
        
        // Selective border controls
        if (params['border-top']) {
            const borderStyle = this.convertBorderStyleValue(params['border-top'])
            if (borderStyle) {
                element.style.borderTop = borderStyle
            }
        }
        
        if (params['border-bottom']) {
            const borderStyle = this.convertBorderStyleValue(params['border-bottom'])
            if (borderStyle) {
                element.style.borderBottom = borderStyle
            }
        }
        
        if (params['border-left']) {
            const borderStyle = this.convertBorderStyleValue(params['border-left'])
            if (borderStyle) {
                element.style.borderLeft = borderStyle
            }
        }
        
        if (params['border-right']) {
            const borderStyle = this.convertBorderStyleValue(params['border-right'])
            if (borderStyle) {
                element.style.borderRight = borderStyle
            }
        }
        
        // Shadow, either by name or as a css value of its own. Named, because a document is
        // describing what a thing is rather than dictating pixels - and because four sizes
        // that agree with each other look better than four that were each guessed at.
        if (params.shadow !== undefined) {
            const asked = String(this.getParameterValue(node, 'shadow') ?? '').trim()
            const named = {
                none: 'none',
                soft: '0 1px 2px rgba(0, 0, 0, 0.20)',
                lifted: '0 2px 6px rgba(0, 0, 0, 0.28)',
                floating: '0 6px 16px rgba(0, 0, 0, 0.35)',
            }
            const shadow = named[asked.toLowerCase()] ?? asked
            if (shadow) {
                element.style.setProperty('box-shadow', shadow, 'important')
            }
        }

        // Border radius parameter for controlling corner rounding
        if (params['border-radius']) {
            const borderRadiusValue = this.convertCssSizeValue(params['border-radius'])
            
            // Check for zero radius before applying - handle both string and object cases
            const isZeroRadius = borderRadiusValue === '0px' || borderRadiusValue === '0' || borderRadiusValue === 0 ||
                                String(borderRadiusValue) === '0px' || String(borderRadiusValue) === '0' ||
                                (typeof params['border-radius'] === 'object' && 
                                 params['border-radius'].CssSize && 
                                 params['border-radius'].CssSize.Pixels === 0)
            
            if (isZeroRadius) {
                // For zero radius, be extra explicit to override default CSS
                element.style.setProperty('border-radius', '0px', 'important')
                element.style.cssText += `; border-radius: 0px !important;`
            } else {
                // Use converted value for non-zero radius
                element.style.setProperty('border-radius', borderRadiusValue, 'important')
            }
            
            // Apply to field-value children for consistent appearance
            setTimeout(() => {
                const fieldValues = element.querySelectorAll('.field-value')
                fieldValues.forEach(fieldValue => {
                    if (isZeroRadius) {
                        fieldValue.style.setProperty('border-radius', '0px', 'important')
                        fieldValue.style.cssText += `; border-radius: 0px !important;`
                    } else {
                        fieldValue.style.setProperty('border-radius', borderRadiusValue, 'important')
                    }
                })
            }, 0)
        }
        
        // Legacy border support (keep for compatibility) - but do not override explicit none
        if (params.border) {
            const currentBorder = element.style.getPropertyValue('border')
            const hasBorderNone = currentBorder && currentBorder.trim().toLowerCase() === 'none'
            if (!hasBorderNone) {
                element.style.border = params.border
            }
        }
        
        // Legacy horizontal-size support (keep for compatibility)
        if (params['horizontal-size']) {
            element.style.width = params['horizontal-size']
        }
        
        // Apply margins to all elements
        this.applyMarginStyles(element, node)
        
        // Add more style mappings as needed
    }

    // Apply conservative default styles for fields (string/number/text/date/timestamp/bool/checkbox)
    // without overriding explicit parameters. This mainly ensures labels/values are readable
    // and laid out consistently even when no styling parameters are provided.
    applyFieldDefaultStyles(container, node) {
        try {
            const params = node?.parameters || {}
            const hasExplicitFont = params['font-color'] !== undefined || params['font-size'] !== undefined

            // Where the label sits relative to the value.
            //
            // Alternating with the nesting, the way a div's layout does: above the value inside
            // a horizontal row, beside it inside a vertical column. The resolver works it out -
            // same calculation as a container's, same `layout` parameter to override it - and
            // leaves the answer under `_label_layout`.
            //
            // Only when there is a label to place. A field written `label=""` renders the value
            // alone, and making its container a flex row would change how that value sizes for
            // no reason at all.
            const label = container.querySelector('label')
            if (label) {
                const beside = this.getParameterValue(node, '_label_layout') === 'horizontal'
                container.classList.add(beside ? 'label-beside' : 'label-above')
                // Inline, because these are inline already and an inline style wins: leaving
                // the stylesheet to say it would be overruled by the line below.
                if (!label.style.marginBottom) label.style.marginBottom = beside ? '0' : '4px'
                if (!label.style.display) label.style.display = beside ? 'inline-block' : 'block'
            }

            // Value element defaults
            const valueEl = container.querySelector('.field-value') || container.querySelector('.text-content')
            if (valueEl) {
                // Avoid collapsing to 0 height when empty
                if (!valueEl.style.minHeight) {
                    valueEl.style.minHeight = OverseerRenderer.VALUE_MIN_HEIGHT
                    valueEl.dataset.minHeightIsDefault = '1'
                }
                // Keep inline-block so borders/padding wrap text nicely - except where the
                // value is meant to line up under its heading. An inline-block is only as wide
                // as its digits and sits at the left of the field, so `text-align` has nothing
                // to move: the heading went to the right edge and the figure stayed at the
                // left, which reads as every heading belonging to the column after it.
                //
                // Set here rather than in the stylesheet because this is an inline style, and
                // an inline style wins.
                if (!valueEl.style.display) {
                    const linesUpUnderItsHeading = container.classList.contains('number-field')
                    valueEl.style.display = linesUpUnderItsHeading ? 'block' : 'inline-block'
                }
                // Inherit typography unless explicitly overridden later
                if (!hasExplicitFont) {
                    valueEl.style.color = 'inherit'
                    valueEl.style.fontSize = 'inherit'
                }
            }

            // Container baseline padding only if nothing else is set; layout/margin handled elsewhere
            const hasExplicitPadding = params.padding !== undefined ||
                params['padding-top'] !== undefined || params['padding-bottom'] !== undefined ||
                params['padding-left'] !== undefined || params['padding-right'] !== undefined
            if (!hasExplicitPadding) {
                if (!container.style.padding) {
                    container.style.padding = OverseerRenderer.FIELD_PADDING
                    container.dataset.paddingIsDefault = '1'
                }
            }
        } catch (_) { /* no-op */ }
    }

    applyMarginStyles(element, node) {
        if (!node || !node.parameters) return
        const get = (k) => this.getParameterValue(node, k)
        const setSize = (prop, val) => {
            if (val === null || val === undefined) return
            if (typeof val === 'object') {
                element.style[prop] = this.convertCssSizeValue(val)
            } else {
                element.style[prop] = String(val)
            }
        }
        // Shorthand margin (can be a CSS string like "8px 4px")
        const m = get('margin')
        if (m !== null && m !== undefined) {
            if (typeof m === 'string') {
                element.style.margin = m
            } else {
                element.style.margin = this.convertCssSizeValue(m)
            }
        }
        // Side-specific margins override shorthand
        setSize('marginTop', get('margin-top'))
        setSize('marginBottom', get('margin-bottom'))
        setSize('marginLeft', get('margin-left'))
        setSize('marginRight', get('margin-right'))
    }

    convertColorValue(colorParam) {
        // Handle different color value types from the Rust backend
        if (typeof colorParam === 'string') {
            // Support 8-digit hex (#RRGGBBAA) by converting to rgba() for wider compatibility
            const hex = colorParam.trim()
            const m = /^#([0-9a-fA-F]{8})$/.exec(hex)
            if (m) {
                const h = m[1]
                const r = parseInt(h.slice(0, 2), 16)
                const g = parseInt(h.slice(2, 4), 16)
                const b = parseInt(h.slice(4, 6), 16)
                const a = parseInt(h.slice(6, 8), 16) / 255
                return `rgba(${r}, ${g}, ${b}, ${a})`
            }
            return colorParam // Legacy string colors
        }
        
        if (typeof colorParam === 'object' && colorParam !== null) {
            // Wrapped enum variant: { Color: { ... } }
            if (colorParam.Color) {
                const color = colorParam.Color
                if (color.Hex) return color.Hex
                if (color.Named) return color.Named
                if (color.Rgb) {
                    const [r, g, b] = color.Rgb
                    const r255 = Math.round(r * 255)
                    const g255 = Math.round(g * 255)
                    const b255 = Math.round(b * 255)
                    return `rgb(${r255}, ${g255}, ${b255})`
                }
            }
            // Unwrapped inner variant: { Hex: "#..." } | { Named: "blue" } | { Rgb: [r,g,b] }
            if (colorParam.Hex) return colorParam.Hex
            if (colorParam.Named) return colorParam.Named
            if (colorParam.Rgb) {
                const [r, g, b] = colorParam.Rgb
                const r255 = Math.round(r * 255)
                const g255 = Math.round(g * 255)
                const b255 = Math.round(b * 255)
                return `rgb(${r255}, ${g255}, ${b255})`
            }
        }
        
        return colorParam // Fallback
    }

    convertCssSizeValue(sizeParam) {
        // Handle different CSS size value types from the Rust backend
        if (typeof sizeParam === 'string') {
            return sizeParam // Legacy string sizes
        }
        
        if (typeof sizeParam === 'object' && sizeParam !== null) {
            // Wrapped enum variant: { CssSize: { ... } }
            if (sizeParam.CssSize) {
                const size = sizeParam.CssSize
                if (size.Pixels !== undefined) return `${size.Pixels}px`
                if (size.Percentage !== undefined) return `${size.Percentage}%`
                if (size.Em !== undefined) return `${size.Em}em`
                if (size.Rem !== undefined) return `${size.Rem}rem`
                if (size.ViewportWidth !== undefined) return `${size.ViewportWidth}vw`
                if (size.ViewportHeight !== undefined) return `${size.ViewportHeight}vh`
                if (size.Auto !== undefined) return 'auto'
                if (size.FitContent !== undefined) return 'fit-content'
            }
            // Unwrapped inner variant: { Pixels: n } | { Percentage: n } | ...
            if (sizeParam.Pixels !== undefined) return `${sizeParam.Pixels}px`
            if (sizeParam.Percentage !== undefined) return `${sizeParam.Percentage}%`
            if (sizeParam.Em !== undefined) return `${sizeParam.Em}em`
            if (sizeParam.Rem !== undefined) return `${sizeParam.Rem}rem`
            if (sizeParam.ViewportWidth !== undefined) return `${sizeParam.ViewportWidth}vw`
            if (sizeParam.ViewportHeight !== undefined) return `${sizeParam.ViewportHeight}vh`
            if (sizeParam.Auto !== undefined) return 'auto'
            if (sizeParam.FitContent !== undefined) return 'fit-content'
        }
        
        return sizeParam // Fallback
    }

    convertBorderStyleValue(borderParam) {
        // Handle different border style value types from the Rust backend
        if (typeof borderParam === 'string') {
            // Accept common keywords; normalize 'none' to CSS none
            if (borderParam.toLowerCase() === 'none') return 'none'
            if (borderParam.toLowerCase() === 'default') return ''
            return borderParam // Legacy string borders
        }
        
        if (typeof borderParam === 'object' && borderParam !== null) {
            if (borderParam.BorderStyle) {
                const style = borderParam.BorderStyle
                if (style === 'None') return 'none'
                if (style === 'Default') return '' // Use default browser styling
                if (style.Solid) {
                    const [thickness, color] = style.Solid
                    const thicknessStr = this.convertCssSizeValue({CssSize: thickness})
                    const colorStr = this.convertColorValue({Color: color})
                    return `${thicknessStr} solid ${colorStr}`
                }
                if (style.Dashed) {
                    const [thickness, color] = style.Dashed
                    const thicknessStr = this.convertCssSizeValue({CssSize: thickness})
                    const colorStr = this.convertColorValue({Color: color})
                    return `${thicknessStr} dashed ${colorStr}`
                }
                if (style.Dotted) {
                    const [thickness, color] = style.Dotted
                    const thicknessStr = this.convertCssSizeValue({CssSize: thickness})
                    const colorStr = this.convertColorValue({Color: color})
                    return `${thicknessStr} dotted ${colorStr}`
                }
            }
        }
        
        return borderParam // Fallback
    }

    isBorderNone(borderParam) {
        if (borderParam === undefined || borderParam === null) return false
        if (typeof borderParam === 'string') return borderParam.toLowerCase() === 'none'
        if (typeof borderParam === 'object') {
            // Wrapped enum
            if (borderParam.BorderStyle) {
                const style = borderParam.BorderStyle
                if (style === 'None') return true
                // Unwrapped spellings sometimes serialize as { None: null }
                if (style.None !== undefined) return true
            }
            // Alternative shape: { BorderStyle: { None: null } }
            if (borderParam.None !== undefined) return true
        }
        return false
    }

    getNodeValue(node) {
        // If the node itself is a string, return it
        if (typeof node === 'string') {
            if (DEBUG_MODE) console.log('[DEBUG] getNodeValue: node is a string:', node);
            return node
        }

        // Prefer raw value when it is not a Formula; otherwise defer to computed
        if (node.parameters && node.parameters["value"] !== undefined) {
        const raw = node.parameters["value"]
            const isFormula = typeof raw === 'object' && raw !== null && raw.Formula !== undefined
            if (!isFormula) {
                if (typeof raw === 'string') {
                    // Treat literal "Null" as explicit null and use fallback
                    if (raw.trim().toLowerCase() === 'null') {
                        try {
                            const fb = node.parameters?._computed_fallback
                            if (fb && typeof fb === 'object') {
                                if (fb.String !== undefined) return fb.String
                                if (fb.Integer !== undefined) return String(fb.Integer)
                                if (fb.Float !== undefined) return String(fb.Float)
                                if (fb.Boolean !== undefined) return String(fb.Boolean)
                                if (fb.Date !== undefined) return fb.Date
                                if (fb.Timestamp !== undefined) return this.formatTimestampValue(node, fb.Timestamp)
                            } else if (typeof fb === 'string') {
                                return fb
                            }
                            const rawFb = node.parameters?.fallback
                            if (rawFb && typeof rawFb === 'object') {
                                if (rawFb.String !== undefined) return rawFb.String
                                if (rawFb.Integer !== undefined) return String(rawFb.Integer)
                                if (rawFb.Float !== undefined) return String(rawFb.Float)
                                if (rawFb.Boolean !== undefined) return String(rawFb.Boolean)
                                if (rawFb.Date !== undefined) return rawFb.Date
                                if (rawFb.Timestamp !== undefined) return this.formatTimestampValue(node, rawFb.Timestamp)
                            } else if (typeof rawFb === 'string') {
                                return rawFb
                            }
                        } catch(_) { /* fall through */ }
                        return ''
                    }
                    const nt = (node.node_type || node.type || '').toLowerCase()
                    if (nt === 'timestamp') return this.formatTimestampValue(node, raw)
                    return raw
                }
                if (typeof raw === 'object' && raw !== null) {
            if (raw.Null !== undefined) {
                        // If value is explicitly Null, try to surface fallback (computed first, then raw)
                        try {
                            const fb = node.parameters?._computed_fallback
                            if (fb && typeof fb === 'object') {
                                if (fb.String !== undefined) return fb.String
                                if (fb.Integer !== undefined) return String(fb.Integer)
                                if (fb.Float !== undefined) return String(fb.Float)
                                if (fb.Boolean !== undefined) return String(fb.Boolean)
                                if (fb.Date !== undefined) return fb.Date
                                if (fb.Timestamp !== undefined) return this.formatTimestampValue(node, fb.Timestamp)
                            } else if (typeof fb === 'string') {
                                return fb
                            }
                            const rawFb = node.parameters?.fallback
                            if (rawFb && typeof rawFb === 'object') {
                                if (rawFb.String !== undefined) return rawFb.String
                                if (rawFb.Integer !== undefined) return String(rawFb.Integer)
                                if (rawFb.Float !== undefined) return String(rawFb.Float)
                                if (rawFb.Boolean !== undefined) return String(rawFb.Boolean)
                                if (rawFb.Date !== undefined) return rawFb.Date
                                if (rawFb.Timestamp !== undefined) return this.formatTimestampValue(node, rawFb.Timestamp)
                            } else if (typeof rawFb === 'string') {
                                return rawFb
                            }
                        } catch (_) { /* ignore and fall through to blank */ }
                        return ''
                    }
                    if (raw.String !== undefined) return raw.String
                    if (raw.Integer !== undefined) return raw.Integer.toString()
                    if (raw.Float !== undefined) return raw.Float.toString()
                    if (raw.Boolean !== undefined) return raw.Boolean.toString()
                    if (raw.Date !== undefined) return raw.Date
                    if (raw.Timestamp !== undefined) return this.formatTimestampValue(node, raw.Timestamp)
                }
                return String(raw)
            }
            // It's a Formula: do not surface raw formula text; fall through to computed or blank
        }

        // Use computed value if present
        if (node.parameters && node.parameters["_computed_value"] !== undefined) {
            const value = node.parameters["_computed_value"]
            if (typeof value === 'string') {
                // Some backends may serialize explicit Null as the literal string "Null"
                // Treat this as an explicit null and surface fallback instead of showing "Null" to users.
                if (value.trim().toLowerCase() === 'null') {
                    try {
                        const fb = node.parameters?._computed_fallback
                        if (fb && typeof fb === 'object') {
                            if (fb.String !== undefined) return fb.String
                            if (fb.Integer !== undefined) return String(fb.Integer)
                            if (fb.Float !== undefined) return String(fb.Float)
                            if (fb.Boolean !== undefined) return String(fb.Boolean)
                            if (fb.Date !== undefined) return fb.Date
                            if (fb.Timestamp !== undefined) return this.formatTimestampValue(node, fb.Timestamp)
                        } else if (typeof fb === 'string') {
                            return fb
                        }
                        const rawFb = node.parameters?.fallback
                        if (rawFb && typeof rawFb === 'object') {
                            if (rawFb.String !== undefined) return rawFb.String
                            if (rawFb.Integer !== undefined) return String(rawFb.Integer)
                            if (rawFb.Float !== undefined) return String(rawFb.Float)
                            if (rawFb.Boolean !== undefined) return String(rawFb.Boolean)
                            if (rawFb.Date !== undefined) return rawFb.Date
                            if (rawFb.Timestamp !== undefined) return this.formatTimestampValue(node, rawFb.Timestamp)
                        } else if (typeof rawFb === 'string') {
                            return rawFb
                        }
                    } catch (_) { /* fall through to blank */ }
                    return ''
                }
                return value
            }
            if (typeof value === 'object') {
                if (value && value.Null !== undefined) return ''
                if (value.String !== undefined) return value.String
                if (value.Integer !== undefined) return value.Integer.toString()
                if (value.Float !== undefined) return value.Float.toString()
                if (value.Boolean !== undefined) return value.Boolean.toString()
                if (value.Date !== undefined) return value.Date
                if (value.Timestamp !== undefined) return this.formatTimestampValue(node, value.Timestamp)
                // Computed value should not be a Formula at display time; if it is, do not show literal
                if (value.Formula !== undefined) return ''
            }
            return value.toString()
        }

        // Try parameters["value"] next
        if (node.parameters && node.parameters["value"] !== undefined) {
            const value = node.parameters["value"]
            if (DEBUG_MODE) console.log('[DEBUG] getNodeValue: found parameters["value"]:', value, 'in node:', node);
            if (typeof value === 'string') {
                // Guard against literal "Null" strings produced upstream
                if (value.trim().toLowerCase() === 'null') {
                    try {
                        const fb = node.parameters?._computed_fallback
                        if (fb && typeof fb === 'object') {
                            if (fb.String !== undefined) return fb.String
                            if (fb.Integer !== undefined) return String(fb.Integer)
                            if (fb.Float !== undefined) return String(fb.Float)
                            if (fb.Boolean !== undefined) return String(fb.Boolean)
                            if (fb.Date !== undefined) return fb.Date
                            if (fb.Timestamp !== undefined) return this.formatTimestampValue(node, fb.Timestamp)
                        } else if (typeof fb === 'string') {
                            return fb
                        }
                        const rawFb = node.parameters?.fallback
                        if (rawFb && typeof rawFb === 'object') {
                            if (rawFb.String !== undefined) return rawFb.String
                            if (rawFb.Integer !== undefined) return String(rawFb.Integer)
                            if (rawFb.Float !== undefined) return String(rawFb.Float)
                            if (rawFb.Boolean !== undefined) return String(rawFb.Boolean)
                            if (rawFb.Date !== undefined) return rawFb.Date
                            if (rawFb.Timestamp !== undefined) return this.formatTimestampValue(node, rawFb.Timestamp)
                        } else if (typeof rawFb === 'string') {
                            return rawFb
                        }
                    } catch (_) { /* fall through */ }
                    return ''
                }
                // If this node is a timestamp-typed field, format string value as timestamp
                const nt = (node.node_type || node.type || '').toLowerCase()
                if (nt === 'timestamp') return this.formatTimestampValue(node, value)
                return value
            }
            if (typeof value === 'object') {
                if (value && value.Null !== undefined) return ''
                if (value.String !== undefined) return value.String
                if (value.Integer !== undefined) return value.Integer.toString()
                if (value.Float !== undefined) return value.Float.toString()
                if (value.Boolean !== undefined) return value.Boolean.toString()
                if (value.Date !== undefined) return value.Date
                if (value.Timestamp !== undefined) return this.formatTimestampValue(node, value.Timestamp)
                if (value.Formula !== undefined) return ''
            }
            return value.toString()
        }

        // Fallback: check node.value directly
    if (node.value !== undefined && node.value !== null) {
            if (DEBUG_MODE) console.log('[DEBUG] getNodeValue: found node.value:', node.value, 'in node:', node);
            if (typeof node.value === 'string') {
                // Guard against literal "Null" strings produced upstream
                if (node.value.trim().toLowerCase() === 'null') {
                    try {
                        const fb = node.parameters?._computed_fallback
                        if (fb && typeof fb === 'object') {
                            if (fb.String !== undefined) return fb.String
                            if (fb.Integer !== undefined) return String(fb.Integer)
                            if (fb.Float !== undefined) return String(fb.Float)
                            if (fb.Boolean !== undefined) return String(fb.Boolean)
                            if (fb.Date !== undefined) return fb.Date
                            if (fb.Timestamp !== undefined) return this.formatTimestampValue(node, fb.Timestamp)
                        } else if (typeof fb === 'string') {
                            return fb
                        }
                        const rawFb = node.parameters?.fallback
                        if (rawFb && typeof rawFb === 'object') {
                            if (rawFb.String !== undefined) return rawFb.String
                            if (rawFb.Integer !== undefined) return String(rawFb.Integer)
                            if (rawFb.Float !== undefined) return String(rawFb.Float)
                            if (rawFb.Boolean !== undefined) return String(rawFb.Boolean)
                            if (rawFb.Date !== undefined) return rawFb.Date
                            if (rawFb.Timestamp !== undefined) return this.formatTimestampValue(node, rawFb.Timestamp)
                        } else if (typeof rawFb === 'string') {
                            return rawFb
                        }
                    } catch (_) { /* fall through */ }
                    return ''
                }
                return node.value
            }
            if (typeof node.value === 'object') {
                if (node.value && node.value.Null !== undefined) return ''
                if (node.value.String !== undefined) return node.value.String
                if (node.value.Integer !== undefined) return node.value.Integer.toString()
                if (node.value.Float !== undefined) return node.value.Float.toString()
                if (node.value.Boolean !== undefined) return node.value.Boolean.toString()
                if (node.value.Date !== undefined) return node.value.Date
                if (node.value.Timestamp !== undefined) return this.formatTimestampValue(node, node.value.Timestamp)
        if (node.value.Formula !== undefined) return ''
            }
            return node.value.toString()
        }

        // Fallback: check node.String (for string nodes)
        if (node.String !== undefined && node.String !== null) {
            if (DEBUG_MODE) console.log('[DEBUG] getNodeValue: found node.String:', node.String, 'in node:', node);
            return node.String
        }

        // As a last resort, return the first string property that isn't a metadata field
        if (typeof node === 'object' && node !== null) {
            const skip = new Set(['name', 'type', 'node_type', 'parameters', 'children', 'label'])
            for (const key in node) {
                if (!skip.has(key) && typeof node[key] === 'string') {
                    if (DEBUG_MODE) console.log(`[DEBUG] getNodeValue: found string property '${key}' in node:`, node);
                    return node[key]
                }
            }
        }

        if (DEBUG_MODE) console.log('[DEBUG] getNodeValue: no value found for node:', node);
        return null
    }

    // Format a RFC3339 timestamp string according to node parameters 'format' or 'precision'
    // Render in the user's local timezone (serialization remains UTC).
    // format overrides precision when provided.
    // Supported precision: 'seconds' (default), 'minutes', 'hours', 'days'
    // Supported format: 'datetime' (YYYY-MM-DD HH:MM:SS), 'date', 'time', 'iso'
    formatTimestampValue(node, rfc3339) {
        if (!rfc3339 || typeof rfc3339 !== 'string') return rfc3339
        // Parse to a Date to get local components
        const d = new Date(rfc3339)
        if (isNaN(d.getTime())) return rfc3339
        const pad = (n) => String(n).padStart(2, '0')
        const Y = d.getFullYear()
        const M = pad(d.getMonth() + 1)
        const D = pad(d.getDate())
        const h = pad(d.getHours())
        const m = pad(d.getMinutes())
        const s = pad(d.getSeconds())

        const datePart = `${Y}-${M}-${D}`
        const timePart = `${h}:${m}:${s}`

        // Extract format and precision preferences (computed or raw)
        const formatPref = (this.getParameterValue(node, 'format') || '').toString().toLowerCase()
        const precision = (this.getParameterValue(node, 'precision') || 'seconds').toString().toLowerCase()

        // Apply explicit format if provided
        switch (formatPref) {
            case 'date':
                return datePart
            case 'time':
                // Honours precision, like the default path does. A meal wants the clock to the
                // minute; seconds on it are noise, and a `format` that silently ignored the
                // `precision` beside it would be one more parameter that means nothing where
                // you happened to put it.
                if (precision === 'minutes' || precision === 'minute') return `${h}:${m}`
                if (precision === 'hours' || precision === 'hour') return `${h}:00`
                return timePart
            case 'iso': {
                // ISO-like local (no Z to avoid implying UTC)
                return `${datePart}T${timePart}`
            }
            case 'datetime':
                // fall through to precision default below
                break
            default:
                // no explicit format -> use precision rules
                break
        }

        // Apply precision (local time)
        switch (precision) {
            case 'days':
            case 'day':
                return datePart
            case 'hours':
            case 'hour':
                return `${datePart} ${h}:00:00`
            case 'minutes':
            case 'minute':
                return `${datePart} ${h}:${m}:00`
            case 'seconds':
            case 'second':
            default:
                return `${datePart} ${timePart}`
        }
    }

    // Helper function to extract parameter values from OverseerValue objects
    getParameterValue(node, parameterName) {
        // Special-case: internal computed fields already include the prefix
        // e.g. '_computed_series', '_computed_x_min', etc. Read them directly.
        try {
            if (parameterName && parameterName.startsWith('_computed_')) {
                if (node.parameters && node.parameters[parameterName] !== undefined) {
                    const v = node.parameters[parameterName]
                    if (typeof v === 'string') return v
                    if (typeof v === 'object' && v !== null) {
                        if (v.String !== undefined) return v.String
                        if (v.Integer !== undefined) return v.Integer
                        if (v.Float !== undefined) return v.Float
                        if (v.Boolean !== undefined) return v.Boolean
                        if (v.Date !== undefined) return v.Date
                        if (v.Timestamp !== undefined) return v.Timestamp
                        if (v.Formula !== undefined) return v.Formula
                        if (v.Color !== undefined) return v.Color
                        if (v.CssSize !== undefined) return v.CssSize
                        if (v.BorderStyle !== undefined) return v
                    }
                    return v
                }
                return null
            }
        } catch(_) { /* fall through */ }
        // Prefer computed parameter if present
    if (node.parameters && node.parameters[`_computed_${parameterName}`] !== undefined) {
            const paramValue = node.parameters[`_computed_${parameterName}`]
            if (typeof paramValue === 'string') return paramValue
            if (typeof paramValue === 'object' && paramValue !== null) {
                if (paramValue.String !== undefined) return paramValue.String
                if (paramValue.Integer !== undefined) return paramValue.Integer
                if (paramValue.Float !== undefined) return paramValue.Float
                if (paramValue.Boolean !== undefined) return paramValue.Boolean
                if (paramValue.Date !== undefined) return paramValue.Date
        if (paramValue.Timestamp !== undefined) return paramValue.Timestamp
                if (paramValue.Formula !== undefined) return paramValue.Formula
                if (paramValue.Color !== undefined) return paramValue.Color
                if (paramValue.CssSize !== undefined) return paramValue.CssSize
                if (paramValue.BorderStyle !== undefined) return paramValue // keep full shape for converter
            }
            return paramValue
        }

        if (!node.parameters || node.parameters[parameterName] === undefined) {
            return null
        }
        
        const paramValue = node.parameters[parameterName]
        
        // If it's already a simple string, return it
        if (typeof paramValue === 'string') {
            return paramValue
        }
        
        // If it's an OverseerValue object, extract the actual value
        if (typeof paramValue === 'object' && paramValue !== null) {
            if (paramValue.String !== undefined) return paramValue.String
            if (paramValue.Integer !== undefined) return paramValue.Integer
            if (paramValue.Float !== undefined) return paramValue.Float
            if (paramValue.Boolean !== undefined) return paramValue.Boolean
            if (paramValue.Date !== undefined) return paramValue.Date
            if (paramValue.Timestamp !== undefined) return paramValue.Timestamp
            if (paramValue.Formula !== undefined) return paramValue.Formula
            if (paramValue.Color !== undefined) return paramValue.Color
            if (paramValue.CssSize !== undefined) return paramValue.CssSize
            if (paramValue.BorderStyle !== undefined) return paramValue // keep full shape for converter
        }
        
        // Fallback: return as-is (let callers handle unknown shapes)
        return paramValue
    }

    // Helper to compute a stable name-based path from the current DOM render context
    buildNodePath(node) {
        // Prefer dataset.path created during render
        try {
            if (node && node.dataset && node.dataset.path) {
                return JSON.parse(node.dataset.path)
            }
        } catch (_) { /* ignore */ }
        // Fallback: attempt to find nearest ancestor with data-path
        try {
            let el = node
            while (el && !el.dataset?.path) el = el.parentElement
            if (el && el.dataset && el.dataset.path) return JSON.parse(el.dataset.path)
        } catch (_) { /* ignore */ }
        return ['root']
    }

    // Detects whether the raw parameter (not computed) is a Formula
    parameterHasFormula(node, parameterName) {
        try {
            if (!node?.parameters) return false
            const raw = node.parameters[parameterName]
            if (!raw || typeof raw !== 'object') return false
            return raw.Formula !== undefined
        } catch (_) { return false }
    }

    // Returns the raw formula string for a parameter, if any
    getRawFormulaText(node, parameterName) {
        try {
            if (!node?.parameters) return null
            const raw = node.parameters[parameterName]
            if (raw && typeof raw === 'object' && raw.Formula !== undefined) {
                return String(raw.Formula)
            }
        } catch (_) { /* no-op */ }
        return null
    }

    makeFieldEditable(element, node, isMultiline = false) {
        // Capture the old displayed value before editing starts (for selective update diff only)
        const oldValue = element.textContent

        // Initialize editor with the raw user-provided value, not the displayed fallback/computed value
        const originalParam = node?.parameters?.value
        const hasFormula = originalParam && typeof originalParam === 'object' && originalParam.Formula !== undefined
        const initialEditorText = (() => {
            // 1) If raw value is a formula, show it
            if (hasFormula) return `$(${originalParam.Formula})`
            // 2) Prefer parameters.value, else node.value
            const raw = (originalParam !== undefined) ? originalParam : (node && node.value !== undefined ? node.value : undefined)
            if (raw === undefined || raw === null) return ''
            if (typeof raw === 'string') {
                // Treat literal "Null" as empty when editing
                return raw.trim().toLowerCase() === 'null' ? '' : raw
            }
            if (typeof raw === 'object') {
                if (raw.Null !== undefined) return ''
                if (raw.String !== undefined) return String(raw.String)
                if (raw.Integer !== undefined) return String(raw.Integer)
                if (raw.Float !== undefined) return String(raw.Float)
                if (raw.Boolean !== undefined) return String(raw.Boolean)
                if (raw.Date !== undefined) return String(raw.Date)
                if (raw.Timestamp !== undefined) return String(raw.Timestamp)
                if (raw.Formula !== undefined) return `$(${raw.Formula})`
                // Unknown shape -> best-effort
                try { return JSON.stringify(raw) } catch(_) { return '' }
            }
            try { return String(raw) } catch(_) { return '' }
        })()

        const input = document.createElement(isMultiline ? 'textarea' : 'input')
        input.value = initialEditorText
        input.className = 'field-editor'
        
        if (isMultiline) {
            input.rows = 3
        }
        
        // Replace the element with the input
        element.style.display = 'none'
        element.parentNode.insertBefore(input, element.nextSibling)
        input.focus()
        input.select()
        
        let editingFinished = false
        const finishEditing = async () => {
            if (editingFinished) return
            editingFinished = true
            
            // Capture field path before any DOM manipulation (may be synthetic for phantom)
            let fieldPath = null
            try {
                fieldPath = this.buildNodePath(element).join('/')
            } catch (e) {
                if (DEBUG_MODE) console.warn('Failed to build field path before editing:', e)
            }
            // Detect if this edit is occurring under a phantom link preview (before we touch DOM text)
            let isUnderPhantom = false
            try {
                let scan = element
                while (scan && scan !== document.body && !scan.hasAttribute?.('data-link-phantom')) { scan = scan.parentElement }
                if (scan && scan.hasAttribute && scan.hasAttribute('data-link-phantom')) {
                    isUnderPhantom = true
                }
            } catch(_) {}

            const newValue = input.value
            const prevDisplay = element.textContent
            const isFormulaInput = typeof newValue === 'string' && /\$\([\s\S]*\)/.test(newValue.trim())
            element.style.display = 'inline'

            // Safely remove input element
            try {
                input.remove()
            } catch (e) {
                if (DEBUG_MODE) console.warn('Input element already removed:', e)
            }

            // Show what was just typed, without waiting for the round trip. Re-resolving a
            // document takes hundreds of milliseconds, and the display used to keep its old
            // value for that whole window - so the edit looked like it had been ignored, and
            // on a slow document there was time to retype it. The authoritative value still
            // arrives with the response and overwrites this.
            //
            // A formula is the exception: what a field displays is the computed result, which
            // only the backend can produce, so showing the typed source would be a lie that
            // then flickers.
            if (!isFormulaInput) {
                try {
                    const asText = (newValue == null) ? '' : String(newValue)
                    if (element.classList && element.classList.contains('markdown-enabled')) {
                        element.innerHTML = this.renderMarkdown(asText)
                    } else {
                        element.textContent = asText
                    }
                } catch (e) {
                    if (DEBUG_MODE) console.warn('Optimistic repaint failed:', e)
                }
            }
            
            // If this edit is under a phantom link preview, materialize the item first and recompute a real path
            let materializedRealPath = null
            // Will hold the link container path (the proxy that hosted the phantom) for targeted refresh after materialization.
            let linkContainerPathArr = null
            try {
                let p = element
                while (p && p !== document.body && !p.hasAttribute?.('data-link-phantom')) { p = p.parentElement }
                if (p && p.hasAttribute && p.hasAttribute('data-link-phantom')) {
                    const meta = JSON.parse(p.getAttribute('data-link-phantom') || '{}')
                    try { linkContainerPathArr = JSON.parse(p.dataset.path || '[]') } catch(_) {}
                    // Derive tail segments from the edited element's synthetic path relative to the link container
                    let metaWithTail = meta
                    try {
                        const containerPathArr = JSON.parse(p.dataset.path || '[]')
                        const elementPathArr = this.buildNodePath(element)
                        // Expect element path: [...containerPathArr, '<phantom>', ...tail]
                        const idxAfterPhantom = containerPathArr.length + 1
                        const hasPhantomMarker = elementPathArr[containerPathArr.length] === '<phantom>'
                        if (hasPhantomMarker && elementPathArr.length > idxAfterPhantom) {
                            const derivedTail = elementPathArr.slice(idxAfterPhantom)
                            const existingTail = Array.isArray(meta.tailSegments) ? meta.tailSegments : []
                            // Remove common prefix to avoid duplication when link already included trailing segments
                            let i = 0
                            while (i < derivedTail.length && i < existingTail.length && derivedTail[i] === existingTail[i]) i++
                            const toAdd = derivedTail.slice(i)
                            metaWithTail = Object.assign({}, meta, { tailSegments: existingTail.concat(toAdd) })
                        }
                    } catch (_) { /* fallback to original meta */ }
                    // Respect phantom-materialize policy on the link container element
                    let position = 'append'
                    try {
                        const containerNodePath = JSON.parse(p.dataset.path || '[]')
                        const containerNode = this.findNodeByPath(window.app.currentDocument, containerNodePath)
                        const policyRaw = this.getParameterValue(containerNode, 'phantom-materialize')
                        const policy = (policyRaw ? String(policyRaw) : 'none').toLowerCase()
                        if (policy.endsWith('on-edit')) {
                            position = policy.startsWith('prepend') ? 'prepend' : 'append'
                        }
                    } catch(_) { /* default to append */ }
                    const realPath = await this._materializePhantomAndComputePath(metaWithTail, { position })
                    
                    if (DEBUG_MODE) console.debug('[Overseer] Phantom materialized on edit. Computed realPath:', realPath)
                    if (realPath) { fieldPath = realPath; materializedRealPath = realPath }
                    // After materialization, switch the proxy container from phantom preview to the real target
                    try {
                        p.removeAttribute('data-link-phantom')
                        const containerPathArr = JSON.parse(p.dataset.path || '[]')
                        if (Array.isArray(containerPathArr) && containerPathArr.length > 0) {
                            if (DEBUG_MODE) console.debug('[Overseer] Re-rendering link proxy container after materialization at path:', containerPathArr.join('/'))
                            this.rerenderSubtree(window.app.currentDocument, containerPathArr)
                        }
                    } catch(_) { /* best-effort */ }
                    // Additionally, re-render the owning list subtree to ensure DOM paths align with the new instance
                    try {
                        const listPathArr = Array.isArray(metaWithTail.listPath) ? metaWithTail.listPath : []
                        if (listPathArr.length > 0) {
                            
                            this.rerenderSubtree(window.app.currentDocument, listPathArr)
                        }
                    } catch(_) { /* best-effort */ }
                }
            } catch(_) {}

            // Update the node value in the document structure (using real path if computed)
            // Instead of using the local node reference, find and update the node in the main document
            let skipElementEvent = false
            // Track if we performed a direct update on a newly materialized target to suppress duplicate events later
            let usedDirectMaterializedUpdate = false
            // Snapshot the existing explicit value (if any) before applying updates to support guarded revert
            let preExistingValueSnapshot = undefined
            try {
                const probePath = materializedRealPath ? materializedRealPath : fieldPath
                if (probePath && window.app && window.app.currentDocument) {
                    const arr = String(probePath).split('/')
                    const target = this.findNodeByPath(window.app.currentDocument, arr)
                    if (target && target.parameters && target.parameters.value !== undefined) {
                        try { preExistingValueSnapshot = JSON.parse(JSON.stringify(target.parameters.value)) } catch(_) { preExistingValueSnapshot = target.parameters.value }
                    }
                }
            } catch(_) { /* best-effort */ }
            // Optimization/guard: if we just materialized and the target field already equals the edited value,
            // skip issuing an update to avoid redundant UI churn.
            try {
                if (materializedRealPath) {
                    // Strong guarantee: Prefer a direct update of the newly materialized node immediately
                    try {
                        if (this._materializedTargets && this._materializedTargets.has(materializedRealPath)) {
                            const t = this._materializedTargets.get(materializedRealPath)
                            if (t && t.leafNode) {
                                // Capture previous value for change record
                                let prevVal = null
                                try { prevVal = this.getNodeValue(t.leafNode) } catch(_) {}
                                // Verify that the stored leaf belongs to the expected freshly inserted item
                                try {
                                    const targetPathArr = materializedRealPath.split('/')
                                    const listPathArr = targetPathArr.slice(0, -2)
                                    const itemName = targetPathArr[targetPathArr.length - 2]
                                    const listNode = this.findNodeByPath(window.app.currentDocument, listPathArr)
                                    if (listNode && Array.isArray(listNode.children)) {
                                        const liveItem = listNode.children.find(ch => ch && ch.name === itemName)
                                        if (liveItem) {
                                            const liveLeaf = (liveItem.children||[]).find(ch => ch && ch.name === 'weight') || null
                                            if (liveLeaf && liveLeaf !== t.leafNode) {
                                                if (DEBUG_MODE) console.warn('[Overseer] materialized leaf mismatch; correcting pointer', { materializedRealPath, materializeId: t.materializeId })
                                                t.leafNode = liveLeaf
                                            }
                                        } else {
                                            if (DEBUG_MODE) console.warn('[Overseer] could not find live item for materialized path', materializedRealPath)
                                        }
                                    }
                                } catch(_) { /* diagnostics best-effort */ }
                                this.updateNodeValue(t.leafNode, newValue)
                                
                                // Immediately refresh computed values/DOM via selective reevaluation
                                try {
                                    const metaTarget = this._materializedTargets.get(materializedRealPath)
                                    if (metaTarget && metaTarget.position === 'prepend') {
                                        
                                        // Direct DOM paint via UID (new item should be at dataset.uid = newItem.__uid)
                                        try {
                                            const uid = metaTarget.itemNode && metaTarget.itemNode.__uid
                                            if (uid) {
                                                const el = document.querySelector(`[data-uid='${uid}']`)
                                                if (el) {
                                                    // Find weight field element inside this item
                                                    let valueHolder = el.querySelector(`[data-path*='${materializedRealPath.split('/').slice(-2).join('/')}'] .field-value`)
                                                    if (!valueHolder) {
                                                        // fallback: any descendant with class field-value
                                                        valueHolder = el.querySelector('.field-value')
                                                    }
                                                    if (valueHolder) {
                                                        valueHolder.textContent = String(newValue)
                                                        
                                                    }
                                                }
                                            }
                                        } catch(_) { /* best-effort */ }
                                        // Targeted update: we still need the link container (e.g., SelectedWeightRecord) to reflect new selection.
                                        // Strategy: pin existing sibling weights (explicit value) then reevaluate only the link container path if available.
                                        try {
                                            const targetPathArr = materializedRealPath.split('/')
                                            const listPathArr = targetPathArr.slice(0, -2)
                                            const listNode = this.findNodeByPath(window.app.currentDocument, listPathArr)
                                            if (listNode && Array.isArray(listNode.children)) {
                                                for (const sib of listNode.children) {
                                                    if (!sib || !Array.isArray(sib.children)) continue
                                                    const wLeaf = sib.children.find(c => c && c.name === 'weight')
                                                    if (!wLeaf) continue
                                                    if (!wLeaf.parameters) wLeaf.parameters = {}
                                                    if (wLeaf.parameters.value === undefined) {
                                                        const curVal = wLeaf.parameters._computed_value || wLeaf.parameters._computed_fallback
                                                        if (curVal && typeof curVal === 'object') {
                                                            try { wLeaf.parameters.value = JSON.parse(JSON.stringify(curVal)) } catch(_) {}
                                                            wLeaf.parameters._override_present = { Boolean: true }
                                                        }
                                                    }
                                                }
                                            }
                                        } catch(_) { /* best-effort pin */ }
                                        if (linkContainerPathArr) {
                                            try {
                                                this.rerenderSubtree(window.app.currentDocument, linkContainerPathArr)
                                                
                                            } catch(e) { console.warn('[Overseer] link container rerender failed', e) }
                                        }
                                    } else if (window.app && typeof window.app.reevaluateDocumentSelective === 'function') {
                                        await window.app.reevaluateDocumentSelective([materializedRealPath], [{ path: materializedRealPath, oldValue: prevVal, newValue }])
                                    }
                                } catch(_) { /* best-effort */ }
                                // Ensure the owning list subtree is in sync in case DOM nodes were not yet present
                                try {
                                    const listPathArr = materializedRealPath.split('/').slice(0, -2)
                                    this.rerenderSubtree(window.app.currentDocument, listPathArr)
                                } catch(_) { /* ignore */ }
                                skipElementEvent = true
                                try { this._materializedTargets.delete(materializedRealPath) } catch(_) {}
                            }
                        }
                    } catch(_) { /* fall through to gates below if direct route not available */ }
                    
                    // First, compare against what the user actually saw (prevDisplay). If equal, skip.
                    const prevStr = String(prevDisplay ?? '')
                    const newStrDirect = String(newValue ?? '')
                    const eqNum = (() => {
                        // Use parseFloat to tolerate suffixes like ` kg` in display
                        const a = parseFloat(prevStr)
                        const b = parseFloat(newStrDirect)
                        return !isNaN(a) && !isNaN(b) && Math.abs(a - b) < 1e-9
                    })()
                    
                    if (prevStr === newStrDirect || eqNum) {
                        
                        try { window.app && window.app.markDocumentModified && window.app.markDocumentModified() } catch(_) {}
                        return
                    }
                    const arr = materializedRealPath.split('/')
                    const nodeAtTarget = this.findNodeByPath(window.app.currentDocument, arr)
                    if (nodeAtTarget) {
                        const currentStr = String(this.getNodeValue(nodeAtTarget) ?? '')
                        const newStr = String(newValue ?? '')
                        const eqNum2 = (() => { const a=parseFloat(currentStr), b=parseFloat(newStr); return !isNaN(a)&&!isNaN(b)&&Math.abs(a-b)<1e-9 })()
                        
                        if (currentStr === newStr || eqNum2) {
                            
                            // Paint the DOM immediately so the user sees the updated value on the new instance
                            try {
                                const targetPathArr = materializedRealPath.split('/')
                                const selector = `[data-path='${JSON.stringify(targetPathArr)}']`
                                let targetEl = null
                                // Scope to owning list to avoid touching stale duplicates
                                try {
                                    const listPathArr = targetPathArr.slice(0, -2)
                                    const listSelector = `[data-path='${JSON.stringify(listPathArr)}']`
                                    const listEl = document.querySelector(listSelector)
                                    if (listEl) targetEl = listEl.querySelector(selector)
                                } catch(_) {}
                                if (!targetEl) targetEl = document.querySelector(selector)
                                if (targetEl) {
                                    const holder = targetEl.querySelector('.field-value, .text-content, .overseer-list-value') || targetEl
                                    // Try to respect numeric formatting (precision, prefix, suffix)
                                    let display = String(newStr)
                                    try {
                                        const nodeAt = this.findNodeByPath(window.app.currentDocument, targetPathArr)
                                        const pref = String(this.getParameterValue(nodeAt, 'prefix') ?? '')
                                        const suf = String(this.getParameterValue(nodeAt, 'suffix') ?? '')
                                        const precRaw = this.getParameterValue(nodeAt, 'precision')
                                        let body = newStr
                                        const asNum = parseFloat(newStr)
                                        if (!isNaN(asNum)) {
                                            const p = (precRaw === null || precRaw === undefined) ? undefined : parseInt(precRaw, 10)
                                            if (!isNaN(p) && p >= 0) body = asNum.toFixed(p)
                                            else body = String(asNum)
                                        }
                                        display = `${pref}${body}${suf}`
                                    } catch(_) { /* fallback to raw newStr */ }
                                    holder.textContent = display
                                } else {
                                    // As a fallback, re-render the owning list subtree
                                    try {
                                        const listPathArr = materializedRealPath.split('/').slice(0, -2)
                                        this.rerenderSubtree(window.app.currentDocument, listPathArr)
                                    } catch(_) {}
                                }
                            } catch(_) { /* best-effort paint */ }
                            // We still want to mark the document modified minimally so save picks up the new instance.
                            try { window.app && window.app.markDocumentModified && window.app.markDocumentModified() } catch(_) {}
                            return
                        }
                    }
                }
            } catch(_) {}
            if (fieldPath && window.app && window.app.currentDocument) {
                if (DEBUG_MODE) console.log('🔧 Updating node in main document at path:', fieldPath, 'with value:', newValue)
                // If we have a materialized path, check the rendered element first; if it already shows the same value, skip
                try {
                    if (materializedRealPath && typeof newValue === 'string') {
                        const targetPathArr = materializedRealPath.split('/')
                        const sel = `[data-path='${JSON.stringify(targetPathArr)}']`
                        const el = document.querySelector(sel)
                        if (el) {
                            const holder = el.querySelector('.field-value, .text-content, .overseer-list-value') || el
                            const shownRaw = holder ? String(holder.textContent ?? '') : ''
                            // Try to account for numeric fields that render with prefix/suffix (e.g., kg)
                            let prefix = '', suffix = ''
                            try {
                                const n = this.findNodeByPath(window.app.currentDocument, targetPathArr)
                                prefix = String(this.getParameterValue(n, 'prefix') ?? '')
                                suffix = String(this.getParameterValue(n, 'suffix') ?? '')
                            } catch(_) {}
                            const stripAffixes = (s) => {
                                let out = String(s || '')
                                if (prefix && out.startsWith(prefix)) out = out.slice(prefix.length)
                                if (suffix && out.endsWith(suffix)) out = out.slice(0, -suffix.length)
                                return out.trim()
                            }
                            const shown = stripAffixes(shownRaw)
                            const want = String(newValue ?? '')
                            const a = parseFloat(shown)
                            const b = parseFloat(want)
                            const eqNum3 = (!isNaN(a) && !isNaN(b) && Math.abs(a - b) < 1e-9)
                            
                            if (shown === want || eqNum3) {
                                
                                try { window.app && window.app.markDocumentModified && window.app.markDocumentModified() } catch(_) {}
                                return
                            }
                        }
                    }
                } catch(_) {}
                // Prefer a direct update of the newly materialized target (exact node reference) to avoid any mis-targeting
                let success = false
                try {
                    if (materializedRealPath && this._materializedTargets && this._materializedTargets.has(materializedRealPath)) {
                        const t = this._materializedTargets.get(materializedRealPath)
                        if (t && t.leafNode) {
                            try {
                                let prevVal = null
                                try { prevVal = this.getNodeValue(t.leafNode) } catch(_) {}
                                // Re-verify pointer integrity before second-stage direct update
                                try {
                                    const targetPathArr = materializedRealPath.split('/')
                                    const listPathArr = targetPathArr.slice(0, -2)
                                    const itemName = targetPathArr[targetPathArr.length - 2]
                                    const listNode = this.findNodeByPath(window.app.currentDocument, listPathArr)
                                    if (listNode && Array.isArray(listNode.children)) {
                                        const liveItem = listNode.children.find(ch => ch && ch.name === itemName)
                                        if (liveItem) {
                                            const liveLeaf = (liveItem.children||[]).find(ch => ch && ch.name === 'weight') || null
                                            if (liveLeaf && liveLeaf !== t.leafNode) {
                                                if (DEBUG_MODE) console.warn('[Overseer] late materialized leaf mismatch; correcting pointer', { materializedRealPath, materializeId: t.materializeId })
                                                t.leafNode = liveLeaf
                                            }
                                        }
                                    }
                                } catch(_) { /* best-effort */ }
                                // Apply update (guarded updates are marked below after path resolution)
                                this.updateNodeValue(t.leafNode, newValue)
                                success = true
                                usedDirectMaterializedUpdate = true
                                
                                // Refresh computed values/DOM
                                try {
                                    if (window.app && typeof window.app.reevaluateDocumentSelective === 'function') {
                                        await window.app.reevaluateDocumentSelective([materializedRealPath], [{ path: materializedRealPath, oldValue: prevVal, newValue }])
                                    }
                                } catch(_) { /* best-effort */ }
                                try {
                                    const listPathArr = materializedRealPath.split('/').slice(0, -2)
                                    this.rerenderSubtree(window.app.currentDocument, listPathArr)
                                } catch(_) { /* ignore */ }
                            } catch(_) {}
                        }
                        // Clean up the entry after use
                        try { this._materializedTargets.delete(materializedRealPath) } catch(_) {}
                    }
                } catch(_) { /* fall back to path-based below */ }
                if (!success) {
                    
                    success = this.updateNodeValueByPath(window.app.currentDocument, fieldPath, newValue)
                }
                if (!success) {
                    if (DEBUG_MODE) console.warn('⚠️ Failed to update node by path, attempting loose path resolution')
                    try {
                        const targetNode = this.resolveNodeByPathLoose(window.app.currentDocument, fieldPath)
                        if (targetNode) {
                            this.updateNodeValue(targetNode, newValue)
                        } else {
                            // Final fallback: use app-level robust resolver to locate the node
                            try {
                                const alt = (window.app && typeof window.app.getNodeByPath === 'function')
                                    ? window.app.getNodeByPath(window.app.currentDocument, fieldPath)
                                    : null
                                if (alt) {
                                    this.updateNodeValue(alt, newValue)
                                } else {
                                    if (DEBUG_MODE) console.warn('⚠️ Could not resolve target node via any resolver; falling back to local node update')
                                    this.updateNodeValue(node, newValue)
                                }
                            } catch (_) {
                                this.updateNodeValue(node, newValue)
                            }
                        }
                    } catch (_) {
                        this.updateNodeValue(node, newValue)
                    }
                }

                // If this was a guarded edit, mark the node so serializer can drop or revert the change
                try {
                    const mode = this.getEffectiveMutableMode(node, element)
                    const wasGuarded = (element && element.getAttribute && element.getAttribute('data-guarded-edit') === '1') || mode === 'guarded'
                    if (wasGuarded) {
                        const arr = String(fieldPath).split('/')
                        const target = this.findNodeByPath(window.app.currentDocument, arr)
                        if (target) {
                            if (!target.parameters) target.parameters = {}
                            target.parameters._guarded_edit = { Boolean: true }
                            if (preExistingValueSnapshot === undefined) {
                                // No explicit value existed prior; this override is new in-session -> allow serializer to drop it
                                target.parameters._guarded_was_new_override = { Boolean: true }
                            } else {
                                // Preserve the original explicit value to restore during serialization
                                try { target.parameters._guarded_original_value = JSON.parse(JSON.stringify(preExistingValueSnapshot)) } catch(_) { target.parameters._guarded_original_value = preExistingValueSnapshot }
                            }
                        }
                        try { element.removeAttribute('data-guarded-edit') } catch(_) {}
                    }
                } catch(_) { /* best-effort */ }
                // Immediately record this user edit for save-time merge to guard against
                // any interim resolve that might overwrite the value before persisting.
                try {
                    if (window.app && window.app._pendingUserEdits && typeof window.app._pendingUserEdits.set === 'function') {
                        window.app._pendingUserEdits.set(fieldPath, { value: newValue, ts: Date.now() })
                    }
                } catch(_) { /* best-effort only */ }
            } else {
                this.updateNodeValue(node, newValue)
            }
            // If user entered a formula, also set a client-side computed value to avoid showing raw formula on re-render
            if (isFormulaInput) {
                try {
                    if (!node.parameters) node.parameters = {}
                    node.parameters["_computed_value"] = { String: prevDisplay }
                } catch (e) {
                    // no-op
                }
            }
            if (DEBUG_MODE) console.log('Field updated:', node.name, newValue)
            
            // Mark document as modified
            if (window.app && window.app.markDocumentModified) {
                window.app.markDocumentModified()
            }
            // Trigger reevaluation so formulas and computed values refresh
            if (window.app && window.app.reevaluateDocumentSelective) {
                // Use pre-captured field path for selective update
                if (fieldPath) {
                    if (DEBUG_MODE) console.log('🔄 Triggering selective update for field:', fieldPath)
                    // Pass the old and new values to help selective update system
                    const updateResult = await window.app.reevaluateDocumentSelective([fieldPath], [{
                        path: fieldPath,
                        oldValue: oldValue,
                        newValue: newValue
                    }])
                    
                    // Skip event emission for any successful selective update (DOM-only or backend selective)
                    if (updateResult && updateResult.success) {
                        if (updateResult.domOnly) {
                            if (DEBUG_MODE) console.log('🎯 Skipping element event for DOM-only update (prevents chart refresh)')
                        } else {
                            if (DEBUG_MODE) console.log('🎯 Skipping element event for successful selective backend update (prevents chart refresh)')
                        }
                        skipElementEvent = true
                    }
                    // If we just materialized a phantom, ensure the correct instance element reflects the new value immediately
                    try {
                        if (materializedRealPath) {
                            const targetPathArr = materializedRealPath.split('/')
                            const selector = `[data-path='${JSON.stringify(targetPathArr)}']`
                            // Try to scope the search within the owning list container to avoid stale duplicates
                            let targetEl = null
                            try {
                                const listPathArr = targetPathArr.slice(0, -2)
                                const listSelector = `[data-path='${JSON.stringify(listPathArr)}']`
                                const listEl = document.querySelector(listSelector)
                                if (listEl) {
                                    // Remove any stray duplicates for the same path outside this list container
                                    const allMatches = Array.from(document.querySelectorAll(selector))
                                    for (const m of allMatches) {
                                        if (!listEl.contains(m)) {
                                            try { m.remove() } catch(_) {}
                                        }
                                    }
                                    targetEl = listEl.querySelector(selector)
                                }
                            } catch(_) { /* ignore; fall back to global */ }
                            if (!targetEl) targetEl = document.querySelector(selector)
                            if (targetEl) {
                                const holder = targetEl.querySelector('.field-value, .text-content, .overseer-list-value') || targetEl
                                const nv = (newValue == null) ? '' : String(newValue)
                                if (holder.classList && holder.classList.contains('text-content') && holder.classList.contains('markdown-enabled')) {
                                    holder.textContent = nv
                                } else {
                                    holder.textContent = nv
                                }
                                if (DEBUG_MODE) console.debug('[Overseer] Painted materialized field at', materializedRealPath)
                            } else {
                                if (DEBUG_MODE) console.debug('[Overseer] No DOM element yet for', materializedRealPath, '— refreshing list subtree again')
                                // Last resort: refresh owning list subtree again
                                try {
                                    const listPathArr = materializedRealPath.split('/').slice(0, -2) // .../WeightRecord__N/field -> take list path
                                    this.rerenderSubtree(window.app.currentDocument, listPathArr)
                                } catch(_) {}
                            }
                        }
                    } catch(_) {}
                    // If we used a direct update for a materialized target, suppress subsequent event emission to avoid unintended side-effects
                    if (usedDirectMaterializedUpdate) {
                        skipElementEvent = true
                    }
                } else {
                    if (DEBUG_MODE) console.warn('No field path available, falling back to full update')
                    await window.app.reevaluateDocumentSelective([])
                }
            }
            // Emit change event for actions (only if not DOM-only/ selective backend update)
            if (!skipElementEvent) {
                try { await this.emitEvent(node, element, 'change') } catch(_) {}
                // Also bubble a change event to the nearest link-proxy container (if any)
                try {
                    let p = element
                    while (p && p !== document.body && !p.hasAttribute?.('data-link-proxy')) { p = p.parentElement }
                    if (p && p.dataset && p.dataset.path) { await this.emitEvent(node, p, 'change') }
                } catch(_) {}
            }
        }
        
        input.addEventListener('blur', finishEditing)
        input.addEventListener('keydown', (e) => {
            if (e.key === 'Enter' && !isMultiline) {
                finishEditing()
            }
            if (e.key === 'Escape') {
                editingFinished = true
                element.style.display = 'inline'
                try {
                    input.remove()
                } catch (e) {
                    if (DEBUG_MODE) console.warn('Input element already removed:', e)
                }
            }
        })
    }

    // Resolve a node by a canonical field path, tolerating instance suffixes ("__N") and ordinal segments ("#k"),
    // and traversing through transparent wrappers when necessary.
    resolveNodeByPathLoose(document, fieldPath) {
        try {
            const pathParts = String(fieldPath).split('/')
            let currentNodes = document
            let targetNode = null

            const exactName = (n) => (n ?? '').toString()
            const normalizeName = (n) => exactName(n)
                .replace(/__\d+$/, '')   // drop instance suffix like __6
                .replace(/#\d+$/, '')    // drop ordinal path suffix like #1
            const segInfo = (part) => {
                const idx = part.indexOf('#')
                return idx >= 0 ? { base: part.slice(0, idx), ord: parseInt(part.slice(idx+1), 10) || 0 } : { base: part, ord: 0 }
            }
            const isTransparent = (node) => {
                try {
                    const nn = exactName(node.name)
                    const nt = exactName(node.node_type || node.type)
                    return node.is_hierarchy_transparent === true || !nn || nn.toLowerCase() === nt.toLowerCase()
                } catch (_) { return false }
            }
            const findMatches = (nodes, wantBase, wantOrd) => {
                const baseNorm = normalizeName(wantBase)
                const hasInstanceSuffix = /__\d+$/.test(String(wantBase))
                // 1) Exact name match first
                const exactMatches = nodes.filter(n => exactName(n.name) === wantBase)
                if (wantOrd === 0 && exactMatches.length > 0) return exactMatches[0]
                if (exactMatches.length > wantOrd) return exactMatches[wantOrd]
                // If the caller specified an explicit instance suffix (e.g., WeightRecord__5) but we didn't
                // find an exact match, do NOT fall back to normalized or type-based matching — that could
                // resolve to the wrong sibling. Force a miss so upstream logic can re-materialize or error.
                if (hasInstanceSuffix) return null
                // 2) Name normalized match (handles '#k' and '__N')
                const normMatches = nodes.filter(n => normalizeName(n.name) === baseNorm)
                if (normMatches.length > 0) return normMatches[wantOrd] || normMatches[0] || null
                // 3) Fallback: match by node_type when names differ (e.g., '-' vs 'WeightRecord')
                const typeMatches = nodes.filter(n => normalizeName(n.node_type || n.type) === baseNorm)
                if (typeMatches.length > 0) return typeMatches[wantOrd] || typeMatches[0] || null
                // 4) Fallback: match by _original_type parameter (templated list items)
                const origMatches = nodes.filter(n => {
                    try {
                        const ot = n.parameters && (n.parameters._original_type || n.parameters._template_type)
                        return ot && normalizeName(ot) === baseNorm
                    } catch(_) { return false }
                })
                if (origMatches.length > 0) return origMatches[wantOrd] || origMatches[0] || null
                return null
            }

            for (let i = 0; i < pathParts.length; i++) {
                const part = pathParts[i]
                const { base, ord } = segInfo(part)
                // Try direct among current level
                let nextNode = findMatches(currentNodes, base, ord)
                // If not found, walk across transparent wrappers (BFS up to a small depth)
                if (!nextNode) {
                    let frontier = currentNodes.slice()
                    let depth = 0
                    const maxDepth = 4
                    while (!nextNode && depth < maxDepth) {
                        const childrenOfTransparents = []
                        for (const n of frontier) {
                            if (isTransparent(n) && Array.isArray(n.children)) {
                                const candidate = findMatches(n.children, base, ord)
                                if (candidate) { nextNode = candidate; break }
                                childrenOfTransparents.push(...n.children)
                            }
                        }
                        frontier = childrenOfTransparents
                        depth++
                    }
                }

                if (!nextNode) return null
                targetNode = nextNode
                if (i < pathParts.length - 1) {
                    currentNodes = Array.isArray(targetNode.children) ? targetNode.children : []
                }
            }
            return targetNode
        } catch (_) { return null }
    }

    makeMarkdownFieldEditable(element, node) {
        // Get the raw markdown text from the node, not the rendered HTML
        const rawMarkdown = this.getNodeValue(node) || ''
        
        // Create a container for the markdown editor
        const editorContainer = document.createElement('div')
        editorContainer.className = 'markdown-editor-container'
        
        // Create toolbar
        const toolbar = document.createElement('div')
        toolbar.className = 'markdown-toolbar'
        
        // Mode toggle button
        const modeToggle = document.createElement('button')
        modeToggle.textContent = 'Preview'
        modeToggle.className = 'mode-toggle-btn'
        toolbar.appendChild(modeToggle)
        
        // Save button
        const saveBtn = document.createElement('button')
        saveBtn.textContent = 'Save'
        saveBtn.className = 'save-btn'
        toolbar.appendChild(saveBtn)
        
        // Cancel button
        const cancelBtn = document.createElement('button')
        cancelBtn.textContent = 'Cancel'
        cancelBtn.className = 'cancel-btn'
        toolbar.appendChild(cancelBtn)
        
        editorContainer.appendChild(toolbar)
        
        // Create textarea for editing
        const textarea = document.createElement('textarea')
        textarea.value = rawMarkdown
        textarea.className = 'markdown-editor'
        textarea.rows = 10
        textarea.placeholder = 'Enter markdown text...'
        editorContainer.appendChild(textarea)
        
        // Create preview div (initially hidden)
        const preview = document.createElement('div')
        preview.className = 'markdown-preview'
        preview.style.display = 'none'
        editorContainer.appendChild(preview)
        
        // Insert editor container
        element.style.display = 'none'
    element.parentNode.insertBefore(editorContainer, element.nextSibling)
        textarea.focus()
        
        let isPreviewMode = false
        
        // Mode toggle functionality
        modeToggle.addEventListener('click', () => {
            if (isPreviewMode) {
                // Switch to edit mode
                textarea.style.display = 'block'
                preview.style.display = 'none'
                modeToggle.textContent = 'Preview'
                isPreviewMode = false
                textarea.focus()
            } else {
                // Switch to preview mode
                preview.innerHTML = this.renderMarkdown(textarea.value)
                textarea.style.display = 'none'
                preview.style.display = 'block'
                modeToggle.textContent = 'Edit'
                isPreviewMode = true
            }
        })
        
    const finishEditing = async (save = true) => {
            if (save) {
                const newValue = textarea.value
                // Update the element with rendered markdown
                element.innerHTML = this.renderMarkdown(newValue)
                
                // Compute field path for selective updates and guarded marking
                let fieldPath = null
                try { fieldPath = this.buildNodePath(element).join('/') } catch(_) {}
                // Snapshot any pre-existing explicit value to support guarded restore on save
                let preExistingValueSnapshot = undefined
                let targetNode = null
                try {
                    if (fieldPath && window.app && window.app.currentDocument) {
                        targetNode = this.findNodeByPath(window.app.currentDocument, String(fieldPath).split('/'))
                        if (targetNode && targetNode.parameters && targetNode.parameters.value !== undefined) {
                            try { preExistingValueSnapshot = JSON.parse(JSON.stringify(targetNode.parameters.value)) } catch(_) { preExistingValueSnapshot = targetNode.parameters.value }
                        }
                    }
                } catch(_) { /* best-effort */ }
                
                // Update the node value in the document structure (prefer resolved target by path)
                try {
                    if (targetNode) this.updateNodeValue(targetNode, newValue)
                    else this.updateNodeValue(node, newValue)
                } catch(_) { this.updateNodeValue(node, newValue) }
                if (DEBUG_MODE) console.log('Markdown field updated:', node.name, newValue)

                // If this was a guarded edit, mark flags so serializer can drop/revert
                try {
                    const wasGuarded = element && element.getAttribute && element.getAttribute('data-guarded-edit') === '1'
                    if (wasGuarded && (targetNode || fieldPath)) {
                        const tgt = targetNode || (this.findNodeByPath(window.app.currentDocument, String(fieldPath).split('/')))
                        if (tgt) {
                            if (!tgt.parameters) tgt.parameters = {}
                            tgt.parameters._guarded_edit = { Boolean: true }
                            if (preExistingValueSnapshot === undefined) {
                                tgt.parameters._guarded_was_new_override = { Boolean: true }
                            } else {
                                try { tgt.parameters._guarded_original_value = JSON.parse(JSON.stringify(preExistingValueSnapshot)) } catch(_) { tgt.parameters._guarded_original_value = preExistingValueSnapshot }
                            }
                        }
                        try { element.removeAttribute('data-guarded-edit') } catch(_) {}
                    }
                } catch(_) { /* best-effort */ }
                
                // Mark document as modified
                if (window.app && window.app.markDocumentModified) {
                    window.app.markDocumentModified()
                }
                // Trigger reevaluation so formulas/computed params refresh
                if (window.app && window.app.reevaluateDocumentSelective) {
                    // Try to determine field path for selective update
                    try {
                        const fp = fieldPath || this.buildNodePath(element).join('/')
                        window.app.reevaluateDocumentSelective([fp])
                    } catch (e) {
                        if (DEBUG_MODE) console.warn('Failed to build field path, falling back to full update:', e)
                        window.app.reevaluateDocumentSelective([])
                    }
                }
        // Emit change event for actions
        try { await this.emitEvent(node, element, 'change') } catch(_) {}
        // Also bubble a change event to the nearest link-proxy container (if any),
        // so containers can react (e.g., ensure_in_list for phantom links)
        try {
            let p = element
            while (p && p !== document.body && !p.hasAttribute?.('data-link-proxy')) { p = p.parentElement }
            if (p && p.dataset && p.dataset.path) {
                await this.emitEvent(node, p, 'change')
            }
        } catch(_) {}
            }
            
            // Clean up
            element.style.display = 'block'
            editorContainer.remove()
        }
        
        // Event handlers
        saveBtn.addEventListener('click', () => finishEditing(true))
        cancelBtn.addEventListener('click', () => finishEditing(false))
        
        // Keyboard shortcuts
        textarea.addEventListener('keydown', (e) => {
            if (e.key === 'Escape') {
                finishEditing(false)
            }
            if (e.ctrlKey && e.key === 'Enter') {
                finishEditing(true)
            }
        })
    }

    // Emit an event to the backend action executor for a given node
    /**
     * If `element` sits inside a link proxy showing a phantom preview, turn that preview into
     * a real list entry and return the element's path within it.
     *
     * A phantom is a preview of what a list entry *would* look like for a key that has no
     * entry yet - a day with nothing logged, say. Its rendered paths carry a literal
     * `<phantom>` segment, which exists only in the DOM: the backend cannot resolve it, so an
     * action addressed that way fails. Editing a field already materializes first; an event
     * fired from a button inside the preview has to do the same, or the button does nothing
     * on exactly the days a user most wants to use it.
     *
     * Returns the real path as an array, or null when the element is not in a phantom.
     */
    async _materializePhantomForEvent(element) {
        try {
            if (!element) return null
            let container = element
            while (container && container !== document.body && !container.hasAttribute?.('data-link-phantom')) {
                container = container.parentElement
            }
            if (!container || !container.hasAttribute?.('data-link-phantom')) return null

            const meta = JSON.parse(container.getAttribute('data-link-phantom') || '{}')
            const containerPathArr = JSON.parse(container.dataset.path || '[]')
            const elementPathArr = (element.dataset && element.dataset.path)
                ? JSON.parse(element.dataset.path)
                : this.buildNodePath(element)

            // Element path is [...containerPath, '<phantom>', ...tail]; keep the tail so the
            // event lands on the same node inside the newly materialized entry.
            let metaWithTail = meta
            const idxAfterPhantom = containerPathArr.length + 1
            if (elementPathArr[containerPathArr.length] === '<phantom>' && elementPathArr.length > idxAfterPhantom) {
                const derivedTail = elementPathArr.slice(idxAfterPhantom)
                const existingTail = Array.isArray(meta.tailSegments) ? meta.tailSegments : []
                let i = 0
                while (i < derivedTail.length && i < existingTail.length && derivedTail[i] === existingTail[i]) i++
                metaWithTail = Object.assign({}, meta, { tailSegments: existingTail.concat(derivedTail.slice(i)) })
            }

            // Honour the container's phantom-materialize policy, as the edit path does.
            let position = 'append'
            try {
                const containerNode = this.findNodeByPath(window.app.currentDocument, containerPathArr)
                const policyRaw = this.getParameterValue(containerNode, 'phantom-materialize')
                const policy = (policyRaw ? String(policyRaw) : 'none').toLowerCase()
                if (policy.endsWith('on-edit')) {
                    position = policy.startsWith('prepend') ? 'prepend' : 'append'
                }
            } catch(_) { /* default to append */ }

            const realPath = await this._materializePhantomAndComputePath(metaWithTail, { position })
            if (!realPath) return null

            // The preview is now backed by a real entry, so stop advertising it as a phantom.
            try {
                container.removeAttribute('data-link-phantom')
                if (containerPathArr.length > 0) {
                    this.rerenderSubtree(window.app.currentDocument, containerPathArr)
                }
            } catch(_) { /* best-effort */ }

            return Array.isArray(realPath) ? realPath : String(realPath).split('/')
        } catch (err) {
            try { if (DEBUG_MODE) console.warn('[Overseer] phantom materialization for event failed', err) } catch(_) {}
            return null
        }
    }

    /**
     * Where a queued event's target can be found again after the document has moved under it.
     *
     * List entries are named by position - Task__1, Task__2 - so removing one renames every
     * entry after it. A press that was waiting its turn while that happened is holding a name
     * that now means a different entry. Keyed lists have something better to go on: the key is
     * a value on the entry, and it does not move.
     *
     * Returns null when there is nothing stable to hold on to, which is the honest answer for
     * an unkeyed list and means the caller should drop the event rather than guess.
     */
    _anchorForPath(path) {
        const doc = window.app && window.app.currentDocument
        if (!doc || !Array.isArray(path) || path.length < 2) return null
        for (let i = path.length - 1; i > 0; i--) {
            const parent = this.findNodeByPath(doc, path.slice(0, i))
            if (!parent) continue
            if (String(parent.node_type || parent.type || '').toLowerCase() !== 'list') continue
            const keyField = this.getParameterValue(parent, 'key')
            if (!keyField) return null
            const entry = this.findNodeByPath(doc, path.slice(0, i + 1))
            if (!entry) return null
            const field = (entry.children || []).find(c => c && c.name === String(keyField))
            if (!field) return null
            const value = this.getParameterValue(field, 'value')
            if (value === undefined || value === null) return null
            return {
                listPath: path.slice(0, i),
                keyField: String(keyField),
                keyValue: String(value),
                tail: path.slice(i + 1),
            }
        }
        return null
    }

    /** The path that anchor points at now, or null if the entry has gone. */
    _pathFromAnchor(anchor) {
        const doc = window.app && window.app.currentDocument
        const list = doc && this.findNodeByPath(doc, anchor.listPath)
        if (!list || !Array.isArray(list.children)) return null
        const entry = list.children.find((c) => {
            const field = (c.children || []).find(f => f && f.name === anchor.keyField)
            if (!field) return false
            const value = this.getParameterValue(field, 'value')
            return value !== undefined && value !== null && String(value) === anchor.keyValue
        })
        if (!entry) return null
        return [...anchor.listPath, entry.name, ...anchor.tail]
    }

    /**
     * Run one event at a time, against the document as it is when its turn comes.
     *
     * Every event is sent with the document's text, and that text is only replaced when the
     * answer arrives. Two presses made in quick succession were therefore both computed
     * against the document as it stood before either of them - and the second answer, applied
     * over the first, put back what the first had removed. Marking two tasks done in a row
     * left one of them open and the other closed, and which one depended on the timing.
     *
     * Waiting is not enough on its own: by the time a queued press runs, the entries it was
     * addressed against may have been renumbered. So the target is re-found by its key, and
     * an event whose target has genuinely gone is dropped rather than sent to whatever now
     * occupies that name.
     */
    async emitEvent(node, element, eventName) {
        if (!window.app || !window.app.currentDocument) return
        const path = (element && element.dataset && element.dataset.path)
            ? JSON.parse(element.dataset.path)
            : (node.__overseer_path || [node.name || node.node_type || node.type || 'root'])

        const busy = this._eventInFlight
        if (!busy) {
            return this._runEventExclusively(node, element, eventName, path, null)
        }
        // Held behind something already in flight, so the document is about to change under
        // this path. Remember what it points at now, while that is still true.
        const anchor = this._anchorForPath(path)
        return this._runEventExclusively(node, element, eventName, path, anchor, busy)
    }

    async _runEventExclusively(node, element, eventName, path, anchor, busy) {
        let release
        this._eventInFlight = new Promise((resolve) => { release = resolve })
        try {
            if (busy) {
                try { await busy } catch (_) { /* its failure is not this event's business */ }
                if (anchor) {
                    const fresh = this._pathFromAnchor(anchor)
                    if (!fresh) {
                        console.warn('[Overseer] dropping an event whose target is gone', anchor)
                        return
                    }
                    path = fresh
                }
            }
            return await this._emitEventNow(node, element, eventName, path)
        } finally {
            this._eventInFlight = null
            release()
        }
    }

    async _emitEventNow(node, element, eventName, pathOverride) {
        if (!window.app || !window.app.currentDocument) return
        let path = Array.isArray(pathOverride) ? pathOverride
            : (element && element.dataset && element.dataset.path)
            ? JSON.parse(element.dataset.path)
            : (node.__overseer_path || [node.name || node.node_type || node.type || 'root'])

        // Materialize before reading the document below, so the event runs against a tree
        // that actually contains the target.
        // Materializing changes the document here, so the text the app holds stops describing
        // it and this event has to carry the document itself.
        let materializedForThisEvent = false
        let nextText = null
        if (Array.isArray(path) && path.includes('<phantom>')) {
            const realPath = await this._materializePhantomForEvent(element)
            if (realPath) path = realPath
            materializedForThisEvent = true
        }
    try { if (DEBUG_MODE) console.debug('[Overseer] emitEvent', eventName, 'path=', path) } catch(_) {}

        // Don't make the trip when nothing will run.
        //
        // The backend answers an event by looking for a child `on <event>` block on the owner
        // node; finding none, it returns the document untouched. Asking anyway means uploading
        // the entire document - measured at ~5.5 s for 10.5 MB, since the IPC moves a couple
        // of MB per second - to be told that nothing happened. Every field edit emits a
        // 'change' event, so a document that handles none pays this on every edit.
        //
        // The check mirrors the backend's own rule, including its one implicit case: a mount
        // acts on 'load'/'unload' without declaring a handler.
        const handlesEvent = (n) => {
            const kids = n && Array.isArray(n.children) ? n.children : []
            return kids.some(c => c && (c.node_type || c.type) === 'on' && c.name === eventName)
        }
        const isImplicitMountEvent = node && (node.node_type || node.type) === 'mount'
            && (eventName === 'load' || eventName === 'unload')
        if (!isImplicitMountEvent && !handlesEvent(node)) {
            if (DEBUG_MODE) console.debug('[Overseer] no handler for', eventName, '- skipping round trip')
            return
        }
        // Guard: ensure nodes is an array (backend expects Vec<OverseerNode> root or serialized map)
        let nodesArg = window.app.currentDocument
        if (nodesArg && !Array.isArray(nodesArg)) {
            // Some earlier logic might have wrapped the document; attempt to unwrap
            if (nodesArg.children && Array.isArray(nodesArg.children)) {
                nodesArg = nodesArg.children
            } else {
                // Fallback: wrap single root into array
                nodesArg = [nodesArg]
            }
        }
        // Defensive: ensure no stray primitive sneaks into root document array
        if (Array.isArray(nodesArg)) {
            const invalids = []
            for (let i = 0; i < nodesArg.length; i++) {
                const n = nodesArg[i]
                if (!n || typeof n !== 'object' || Array.isArray(n)) invalids.push({ index: i, type: typeof n, value: n })
            }
            if (invalids.length > 0) {
                try { if (DEBUG_MODE) console.warn('[Overseer] Filtering invalid root nodes before event invoke', invalids) } catch(_) {}
                nodesArg = nodesArg.filter(n => n && typeof n === 'object' && !Array.isArray(n))
            }
        }
        // Normalize raw boolean parameter values into OverseerValue objects to satisfy serde expectations
        try {
            const wrapBooleanParams = (node) => {
                if (!node || typeof node !== 'object') return
                const p = node.parameters
                if (p && typeof p === 'object') {
                    for (const k of Object.keys(p)) {
                        if (p[k] === true) p[k] = { Boolean: true }
                        else if (p[k] === false) p[k] = { Boolean: false }
                        else if (k === 'value' && typeof p[k] === 'object' && p[k] !== null) {
                            // leave structured OverseerValue as-is
                        }
                    }
                }
                if (Array.isArray(node.children)) node.children.forEach(wrapBooleanParams)
            }
            if (Array.isArray(nodesArg)) nodesArg.forEach(wrapBooleanParams)
        } catch(errNorm) { try { console.warn('[Overseer] boolean normalization failed', errNorm) } catch(_) {} }
        let updated = null
        try {
            // Sanitize nodes: deep clone shallowly to strip any live references / accidental arrays in fields.
            //
            // Provenance must survive this. The backend serializer is snapshot-driven: it replays
            // each node's original source text, looked up in the SourceRegistry by `source_id`
            // (the snapshot itself is #[serde(skip)] and never crosses the IPC boundary). The
            // response to this call is adopted as the live document, so anything dropped here is
            // gone for every later save - taking comments, explicit type keywords and authored
            // formatting with it, since there is no longer a merge_comments path to rebuild them.
            const PROVENANCE_KEYS = [
                'source_id',
                'source_fingerprint',
                'param_order',
                'raw_value_literal',
                'authored_dash',
                'child_original_index',
                'leading_blank_lines',
                'template',
            ]
            const sanitizeNode = (n) => {
                if (!n || typeof n !== 'object') return null
                const copy = { name: n.name, node_type: n.node_type || n.type, parameters: {}, children: [], is_hierarchy_transparent: !!n.is_hierarchy_transparent }
                for (const key of PROVENANCE_KEYS) {
                    if (n[key] !== undefined) copy[key] = n[key]
                }
                if (n.parameters && typeof n.parameters === 'object') {
                    for (const [k,v] of Object.entries(n.parameters)) {
                        // Skip transient client-only helpers
                        if (k === '_injected_link_params') continue
                        copy.parameters[k] = v
                    }
                }
                if (Array.isArray(n.children)) copy.children = n.children.map(ch => sanitizeNode(ch)).filter(Boolean)
                return copy
            }
            // Send the document's text rather than the document.
            //
            // Uploading it measured ~5.5 s for 10.5 MB, since the IPC moves a couple of MB
            // per second - the same cost that was removed from the edit path. Rust owns the
            // document and rebuilds it from the text, which is two orders of magnitude
            // smaller. The text is unknown right after something restructured the document
            // client-side (materializing a phantom row, above), and then the document still
            // has to travel, because the text does not yet describe it.
            const knownText = (!materializedForThisEvent && typeof window.app._currentText === 'string')
                ? window.app._currentText
                : null
            if (knownText !== null) {
                // Ask for what changed. An event that moves one field - a day-navigation
                // button, say - costs a couple of nodes instead of the whole document, both
                // to send and to repaint.
                const update = await invoke('execute_overseer_event_update', {
                    content: knownText,
                    node_path: path,
                    nodePath: path,
                    event_name: eventName,
                    eventName
                }).catch(() => null)
                if (update && Array.isArray(update.changes)) {
                    window.app._currentText = typeof update.text === 'string' ? update.text : null
                    const touched = window.app.applyDocumentChanges(window.app.currentDocument, update.changes)
                    try {
                        this.repaintNodes(touched, window.app.currentDocument)
                    } catch (e) {
                        console.error('Render error (event repaint):', e)
                        try { this.renderDocument(window.app.currentDocument) } catch(_) {}
                    }
                    window.app.markDocumentModified && window.app.markDocumentModified()
                    try { window.app.startScheduler && window.app.startScheduler() } catch(_) {}
                    return
                }
                if (update && Array.isArray(update.nodes)) {
                    updated = update.nodes
                    nextText = typeof update.text === 'string' ? update.text : null
                }
            }
            if (knownText !== null && (updated === undefined || updated === null)) {
                const answer = await invoke('execute_overseer_event_with_text', {
                    content: knownText,
                    node_path: path,
                    nodePath: path,
                    event_name: eventName,
                    eventName
                }).catch((e) => {
                    if (DEBUG_MODE) console.warn('[Overseer] text-driven event failed; sending the document', e)
                    return null
                })
                if (answer && Array.isArray(answer.nodes)) {
                    updated = answer.nodes
                    nextText = typeof answer.text === 'string' ? answer.text : null
                }
            }
            if (updated === undefined || updated === null) {
                const sanitized = Array.isArray(nodesArg) ? nodesArg.map(n=>sanitizeNode(n)).filter(Boolean) : []
                updated = await invoke('execute_overseer_event', {
                    nodes: sanitized,
                    node_path: path,
                    nodePath: path, // provide camelCase variant for environments expecting it
                    event_name: eventName,
                    eventName // camelCase variant
                })
            }
        } catch(err) {
            // Attach additional context for debugging invalid args issues
            try { console.warn('[Overseer] execute_overseer_event failed', err, { eventName, path, nodesType: typeof nodesArg, sampleNode: nodesArg && nodesArg[0] && nodesArg[0].name }) } catch(_) {}
            throw err
        }
        // Only replace the document when the backend actually returned a document structure.
        const looksLikeDocArray = Array.isArray(updated) && updated.every(n => n && typeof n === 'object')
        const looksLikeDocObject = updated && typeof updated === 'object' && Array.isArray(updated.children)
        if (looksLikeDocArray || looksLikeDocObject) {
            const newDoc = looksLikeDocArray ? updated : updated.children
            // An event can restructure the document. When the backend returned the new text
            // with it, that text describes the result and the next interaction can use it;
            // otherwise the app has no accurate text and must rebuild it once.
            try { window.app._currentText = nextText } catch(_) {}
            const oldDoc = window.app.currentDocument
            // Tag any backend-driven changes under mutable=guarded so they remain UI-only until save
            try { this._tagGuardedChangesAfterBackendUpdate(oldDoc, newDoc) } catch(_) {}
            // Adopt using app’s preservation logic (formulas, flags), then render
            try { if (typeof window.app._applyResolvedDocumentWithFormulaPreservation === 'function') { window.app._applyResolvedDocumentWithFormulaPreservation(newDoc) } else { window.app.currentDocument = newDoc } } catch(_) { window.app.currentDocument = newDoc }
            window.app.renderer.renderDocument(window.app.currentDocument)
            window.app.markDocumentModified && window.app.markDocumentModified()
        } else {
            // Backend returned a status/boolean or unexpected shape; keep current document
            if (DEBUG_MODE) console.warn('[Overseer] emitEvent returned non-document value; preserving current document:', updated)
            // As a safety net, re-render the existing document if needed
            try { window.app.renderer.renderDocument(window.app.currentDocument) } catch (_) {}
        }
    // Reschedule timers based on the new document state
    try { window.app.startScheduler && window.app.startScheduler() } catch(_) {}
    }

    // Helper function to update a node's value in the document structure by path
    updateNodeValueByPath(document, fieldPath, newValue) {
        try {
            const pathParts = fieldPath.split('/')
            let currentNodes = document
            let targetNode = null
            
            const exactName = (n) => (n ?? '').toString()
            const normalizeName = (n) => exactName(n)
                .replace(/__\d+$/, '')   // drop instance suffix like __6
                .replace(/#\d+$/, '')    // drop ordinal path suffix like #1
            const segInfo = (part) => {
                const idx = part.indexOf('#')
                return idx >= 0 ? { base: part.slice(0, idx), ord: parseInt(part.slice(idx+1), 10) || 0 } : { base: part, ord: 0 }
            }
            const isTransparent = (node) => {
                try {
                    const nn = exactName(node.name)
                    const nt = exactName(node.node_type || node.type)
                    return node.is_hierarchy_transparent === true || !nn || nn.toLowerCase() === nt.toLowerCase()
                } catch (_) { return false }
            }
            const findMatches = (nodes, wantBase, wantOrd) => {
                const baseNorm = normalizeName(wantBase)
                const hasInstanceSuffix = /__\d+$/.test(String(wantBase))
                // 1) Exact name match first
                const exactMatches = nodes.filter(n => exactName(n.name) === wantBase)
                if (wantOrd === 0 && exactMatches.length > 0) return exactMatches[0]
                if (exactMatches.length > wantOrd) return exactMatches[wantOrd]
                // If an explicit instance suffix was provided but no exact match, do not degrade to
                // normalized or type-based matching. This avoids accidentally targeting another item.
                if (hasInstanceSuffix) return null
                // 2) Name normalized match (handles '#k' and '__N')
                const normMatches = nodes.filter(n => normalizeName(n.name) === baseNorm)
                if (normMatches.length > 0) return normMatches[wantOrd] || normMatches[0] || null
                // 3) Fallback: match by node_type when names differ (e.g., '-' vs 'WeightRecord')
                const typeMatches = nodes.filter(n => normalizeName(n.node_type || n.type) === baseNorm)
                if (typeMatches.length > 0) return typeMatches[wantOrd] || typeMatches[0] || null
                // 4) Fallback: match by _original_type parameter (templated list items)
                const origMatches = nodes.filter(n => {
                    try {
                        const ot = n.parameters && (n.parameters._original_type || n.parameters._template_type)
                        return ot && normalizeName(ot) === baseNorm
                    } catch(_) { return false }
                })
                if (origMatches.length > 0) return origMatches[wantOrd] || origMatches[0] || null
                return null
            }
            
            // Navigate to the target node
            for (let i = 0; i < pathParts.length; i++) {
                const part = pathParts[i]
                const { base, ord } = segInfo(part)

                // 1) Try direct match among current level
                let nextNode = findMatches(currentNodes, base, ord)

                // 2) If not found, try searching through any transparent wrappers without consuming extra path segments
                if (!nextNode) {
                    // BFS across transparent wrapper layers (depth-limited)
                    let frontier = currentNodes.slice()
                    let depth = 0
                    const maxDepth = 4
                    while (!nextNode && depth < maxDepth) {
                        const childrenOfTransparents = []
                        for (const n of frontier) {
                            if (isTransparent(n) && Array.isArray(n.children)) {
                                // Check among these children for a match first (at this depth)
                                const candidate = findMatches(n.children, base, ord)
                                if (candidate) { nextNode = candidate; break }
                                // Otherwise, continue to expand
                                childrenOfTransparents.push(...n.children)
                            }
                        }
                        frontier = childrenOfTransparents
                        depth++
                    }
                }

                if (!nextNode) {
                    if (DEBUG_MODE) console.warn('❌ Could not find node at path:', fieldPath, 'missing:', part)
                    return false
                }

                targetNode = nextNode

                // If not the last part, move to children for next iteration
                if (i < pathParts.length - 1) {
                    currentNodes = Array.isArray(targetNode.children) ? targetNode.children : []
                }
            }
            
            if (targetNode) {
                if (DEBUG_MODE) console.log('✅ Found target node:', targetNode.name, 'updating value to:', newValue)
                this.updateNodeValue(targetNode, newValue)
                return true
            } else {
                if (DEBUG_MODE) console.warn('❌ Target node not found at path:', fieldPath)
                return false
            }
        } catch (e) {
                console.warn('❌ Error updating node by path:', e)
            return false
        }
    }

    // Helper function to update a node's value in the document structure
    updateNodeValue(node, newValue) {
        // Update the node's parameters.value with the appropriate OverseerValue type
        if (!node.parameters) {
            node.parameters = {}
        }
        // If this field is inherited from a template (no explicit override yet), convert it to an override
        try {
            const isTemplateChild = node?.parameters && (node.parameters._template_node === true || Object.keys(node.parameters).some(k => String(k).startsWith('_template_')))
            const hasExplicitOverride = node?.parameters && (
                node.parameters._explicit_child_override === true ||
                (typeof node.parameters._explicit_child_override === 'object' && node.parameters._explicit_child_override?.Boolean === true) ||
                node.parameters._override_present === true ||
                (typeof node.parameters._override_present === 'object' && node.parameters._override_present?.Boolean === true)
            )
            if (isTemplateChild && !hasExplicitOverride) {
                // Remove template markers that would make resolver/serializer drop edits
                Object.keys(node.parameters).forEach(k => { if (k.startsWith('_template_')) delete node.parameters[k] })
                delete node.parameters._template_node
                // Use OverseerValue shape for booleans to match backend enum (externally tagged)
                node.parameters._override_present = { Boolean: true }
                node.parameters._explicit_child_override = { Boolean: true }
                // Ensure parent tracks explicit override list if available
                try {
                    // Find parent path and update document in place if possible
                    const path = this.buildNodePath(document.querySelector(`[data-path]`)) // fallback no-op
                    // We defer strict parent tracking; serializer already consults _explicit_overrides where present
                } catch (_) { /* no-op */ }
            }
        } catch (_) { /* ignore */ }
        
        // Handle boolean values (from checkboxes)
        if (typeof newValue === 'boolean') {
            node.parameters.value = { Boolean: newValue }
            return
        }
        // Normalize input
        const text = (newValue ?? '').toString()

        // Empty text maps to explicit Null sentinel
        if (text.trim() === '') {
            node.parameters.value = { Null: null }
            return
        }

        // If user entered a formula like $(...), store as Formula preserving the inner expression
        const formulaMatch = text.match(/^\s*\$\(([\s\S]*)\)\s*$/)
        if (formulaMatch) {
            const inner = formulaMatch[1]
            node.parameters.value = { Formula: inner }
            return
        }

        // Prefer node type when coercing values
        const nodeType = (node.node_type || node.type || '').toLowerCase()
    if (nodeType === 'int') {
            const intVal = parseInt(text, 10)
            if (!isNaN(intVal)) { node.parameters.value = { Integer: intVal }; return }
        }
        if (nodeType === 'float') {
            const floatVal = parseFloat(text)
            if (!isNaN(floatVal)) { node.parameters.value = { Float: floatVal }; return }
        }
        if (nodeType === 'bool' || nodeType === 'boolean') {
            if (text === 'true' || text === 'false') { node.parameters.value = { Boolean: text === 'true' }; return }
        }

        // Try to preserve the original type if possible
        const currentValue = node.parameters.value
        if (currentValue && typeof currentValue === 'object') {
            if (currentValue.Null !== undefined) { node.parameters.value = { String: text }; return }
            if (currentValue.Integer !== undefined) {
                const numValue = parseInt(text, 10)
                if (!isNaN(numValue)) { node.parameters.value = { Integer: numValue }; return }
            }
            if (currentValue.Float !== undefined) {
                const floatValue = parseFloat(text)
                if (!isNaN(floatValue)) { node.parameters.value = { Float: floatValue }; return }
            }
            if (currentValue.Boolean !== undefined) {
                if (text === 'true' || text === 'false') { node.parameters.value = { Boolean: text === 'true' }; return }
            }
        }

        // Default to String type
        node.parameters.value = { String: text }
    }

    renderMarkdown(text) {
        try {
            // Preprocess custom inline color syntax before passing to markdown parser.
            // Syntax: <color=#FF0000 | This text is red>
            // Allowed color value formats: #RRGGBB, #RGB, named CSS color (alphabetic), rgb(a)(), hsl(a)().
            // We sanitize by whitelisting acceptable patterns and discarding anything else (leaving raw text).
            const preprocessColorTags = (input) => {
                if (!input || typeof input !== 'string' || input.indexOf('<color=') === -1) return input
                return input.replace(/<color=([^|>]+)\|(.*?)>/gms, (match, rawColor, inner) => {
                    const color = String(rawColor).trim()
                    const content = String(inner).trim()
                    // Basic safe patterns
                    const isHex = /^#([0-9a-fA-F]{3}|[0-9a-fA-F]{6})$/.test(color)
                    const isNamed = /^[a-zA-Z]+$/.test(color)
                    const isRgb = /^rgb(a)?\(\s*[-+]?\d{1,3}\s*,\s*[-+]?\d{1,3}\s*,\s*[-+]?\d{1,3}(\s*,\s*(0|0?\.\d+|1(\.0+)?))?\s*\)$/.test(color)
                    const isHsl = /^hsl(a)?\(\s*[-+]?\d{1,3}\s*,\s*\d{1,3}%\s*,\s*\d{1,3}%(\s*,\s*(0|0?\.\d+|1(\.0+)?))?\s*\)$/.test(color)
                    if (!(isHex || isNamed || isRgb || isHsl)) {
                        return content // Unsafe or unsupported color format; strip tag but keep text
                    }
                    // Escape angle brackets in content minimally (marked will further sanitize if configured)
                    const esc = content.replace(/</g, '&lt;').replace(/>/g, '&gt;')
                    return `<span class=\"md-inline-color\" style=\"color:${color}\">${esc}</span>`
                })
            }
            const preprocessed = preprocessColorTags(text)
            // Use marked library for proper markdown rendering on preprocessed text
            return marked.parse(preprocessed);
        } catch (error) {
            console.warn('Markdown parsing error:', error);
            // Fallback to basic markdown rendering
            return text
                .replace(/\*\*(.*?)\*\*/g, '<strong>$1</strong>')
                .replace(/\*(.*?)\*/g, '<em>$1</em>')
                .replace(/\n/g, '<br>');
        }
    }

    /**
     * Attempt to update only specific fields in the DOM without full re-render
     * Returns true if successful, false if full re-render is needed
     */
    updateSelectiveFields(oldDocument, newDocument, changedFieldPaths, fieldChanges = []) {
        return profiled('  updateSelectiveFields', () => this._updateSelectiveFields(oldDocument, newDocument, changedFieldPaths, fieldChanges))
    }
    _updateSelectiveFields(oldDocument, newDocument, changedFieldPaths, fieldChanges = []) {
        try {
            if (DEBUG_MODE) console.log('🎯 Selective DOM update for paths:', changedFieldPaths)
            if (fieldChanges.length > 0) {
                if (DEBUG_MODE) console.log('💡 Using field change info for selective updates')
            }
            
            // Create a map of field changes for quick lookup
            const changeMap = new Map()
            for (const change of fieldChanges) {
                changeMap.set(change.path, change)
            }
            
            let updateCount = 0
            
            for (const fieldPath of changedFieldPaths) {
                if (DEBUG_MODE) console.log('🔍 Looking for DOM elements with path:', fieldPath)
                
                const changeInfo = changeMap.get(fieldPath)
                if (changeInfo) {
                    if (DEBUG_MODE) console.log('📝 Field change detected:', changeInfo)
                    
                    // Find all candidate elements with the same data-path and pick the deepest one
                    const all = Array.from(document.querySelectorAll('[data-path]'))
                    const candidates = []
                    for (const el of all) {
                        try {
                            const p = JSON.parse(el.dataset.path || '[]').join('/')
                            if (p === fieldPath) {
                                // compute DOM depth
                                let depth = 0, cur = el
                                while (cur && cur !== document.body) { depth++; cur = cur.parentElement }
                                candidates.push({ el, depth })
                            }
                        } catch (_) {}
                    }
                    if (candidates.length > 0) {
                        candidates.sort((a,b) => b.depth - a.depth)
                        const targetEl = candidates[0].el
                        const elementPath = JSON.parse(targetEl.dataset.path || '[]')
                        const newNode = this.findNodeByPath(newDocument, elementPath)
                        if (newNode) {
                            if (DEBUG_MODE) console.log('📝 Updating deepest element for changed field:', newNode.name)
                            if (this.updateSingleElement(targetEl, newNode, elementPath)) {
                                updateCount++
                                // Safety net: if this field lives under a list, consider re-rendering that list subtree.
                                // However, skip when the newDocument's list items appear as generic '-' nodes, which
                                // indicates a partially resolved structure from selective backend processing.
                                try {
                                    const listAncestorPath = this.findNearestAncestorOfTypePath(newDocument, elementPath, 'list')
                                    if (listAncestorPath) {
                                        const listNode = this.findNodeByPath(newDocument, listAncestorPath)
                                        const children = Array.isArray(listNode?.children) ? listNode.children : []
                                        const hasGenericDash = children.some(ch => (ch?.node_type||'').toLowerCase() === '-')
                                        const hasTemplatedInstances = children.some(ch => ch?.parameters && (ch.parameters._original_type || ch.parameters._from_template))
                                        // Only re-render when we have templated instances; avoid clobbering UI with generic '-' placeholders
                                        if (hasTemplatedInstances && !hasGenericDash) {
                                            if (DEBUG_MODE) console.log('🔁 Re-rendering ancestor list subtree at path:', listAncestorPath.join('/'))
                                            this.rerenderSubtree(newDocument, listAncestorPath)
                                        } else {
                                            if (DEBUG_MODE) console.log('⏭️ Skipping list subtree re-render due to generic/partial children')
                                        }
                                    }
                                } catch (e) { if (DEBUG_MODE) console.warn('List subtree re-render skipped:', e) }
                            }
                        } else {
                            if (DEBUG_MODE) console.log('❌ Node not found for path:', elementPath, '— applying DOM-only fallback using changeInfo')
                            // DOM-only fallback: update visible text using changeInfo when backend provided partial structure
                            try {
                                const holder = targetEl.querySelector('.field-value, .text-content, .overseer-list-value') || targetEl
                                const nv = (changeInfo.newValue == null) ? '' : String(changeInfo.newValue)
                                if (holder) {
                                    // If markdown-enabled, avoid innerHTML changes here; treat as plain text
                                    if (holder.classList && holder.classList.contains('text-content') && holder.classList.contains('markdown-enabled')) {
                                        holder.textContent = nv
                                    } else {
                                        holder.textContent = nv
                                    }
                                    updateCount++
                                }
                            } catch (_) { /* ignore */ }
                        }
                    }
                } else {
                    // Fallback to old comparison-based approach if no change info
                    if (DEBUG_MODE) console.log('⚠️ No change info available, using comparison approach for:', fieldPath)
                    this.updateFieldByComparison(oldDocument, newDocument, fieldPath)
                    updateCount++ // Assume it worked for now
                }
            }
            
            // Additionally, refresh all link-proxy containers because their computed targets
            // may depend on interpolated fields (e.g., $(../selected_date)). This ensures
            // dynamic link bindings update without requiring a full document render.
            try {
                const linkEls = Array.from(document.querySelectorAll('[data-link-proxy]'))
                if (linkEls.length > 0 && changedFieldPaths && changedFieldPaths.length > 0) {
                    if (DEBUG_MODE) console.log(`🔗 Considering refresh for ${linkEls.length} link proxy container(s) due to changes:`, changedFieldPaths)
                    for (const el of linkEls) {
                        try {
                            const p = JSON.parse(el.dataset.path || '[]')
                            if (!Array.isArray(p) || p.length === 0) continue
                            const linkPath = p.join('/')
                            let targetPathArr = null
                            try { targetPathArr = JSON.parse(el.getAttribute('data-link-target-path') || 'null') } catch(_) { targetPathArr = null }
                            const targetPathStr = Array.isArray(targetPathArr) ? targetPathArr.join('/') : null
                            // If selected_date (or any interpolated field) changed and affects this link (heuristic: link has $( ) pattern in its raw param), force refresh.
                            let forceDueToInterpolation = false
                            try {
                                const node = this.findNodeByPath(newDocument, p)
                                const rawLink = node && node.parameters && (node.parameters.link?.String || node.parameters.link)
                                if (rawLink && /\$\([^)]*\)/.test(String(rawLink))) {
                                    // If any changed field path shares the same ancestor (parent of proxy) assume interpolation may differ
                                    const parentPath = p.slice(0, -1).join('/')
                                    forceDueToInterpolation = changedFieldPaths.some(cf => cf.startsWith(parentPath + '/'))
                                }
                            } catch(_) {}
                            // Skip only if the change occurred inside the proxy's own subtree (avoid overwriting live edit) AND not forced
                            const changedInsideProxy = changedFieldPaths.some(cf => cf.startsWith(linkPath + '/'))
                            if (changedInsideProxy && !forceDueToInterpolation) {
                                if (DEBUG_MODE) console.log('⏭️ Skipping link proxy refresh (internal change) for', linkPath)
                                continue
                            }
                            if (forceDueToInterpolation || !changedInsideProxy) {
                                if (DEBUG_MODE) console.log('🔁 Refreshing link proxy', linkPath, 'forceDueToInterpolation=', forceDueToInterpolation)
                                this.rerenderSubtree(newDocument, p)
                            }
                        } catch (_) { /* ignore individual failures */ }
                    }
                }
            } catch (_) { /* best-effort only */ }

            if (DEBUG_MODE) console.log(`✅ Selective update completed: ${updateCount} elements updated`)
            return updateCount > 0
            
        } catch (error) {
            console.error('Error in selective DOM update:', error)
            return false
        }
    }

    // Find the nearest ancestor path (including self if matches) whose node_type equals typeName
    findNearestAncestorOfTypePath(documentArray, pathArray, typeName) {
        try {
            // Walk up from deepest to root
            for (let i = pathArray.length; i >= 1; i--) {
                const ancestorPath = pathArray.slice(0, i)
                const node = this.findNodeByPath(documentArray, ancestorPath)
                if (!node) continue
                const ty = (node.node_type || node.type || '').toLowerCase()
                if (ty === String(typeName).toLowerCase()) return ancestorPath
            }
        } catch (_) {}
        return null
    }

    // Replace a rendered subtree at a given path with a freshly rendered one from the provided document
    rerenderSubtree(documentArray, pathArray) {
        try {
            // Determine the node at this path to infer expected container class
            const node = this.findNodeByPath(documentArray, pathArray)
            if (!node) return false
            const nodeType = (node.node_type || node.type || '').toLowerCase()
            const expectedClass = (() => {
                switch (nodeType) {
                    case 'list': return 'overseer-list'
                    case 'list_item':
                    case '-': return 'overseer-list-item'
                    case 'div': return 'overseer-div'
                    default: return null // fall back to any element with matching data-path
                }
            })()

            // Gather all elements whose dataset.path matches exactly
            const all = Array.from(document.querySelectorAll('[data-path]'))
            const matches = []
            for (const cand of all) {
                try {
                    const p = JSON.parse(cand.dataset.path || '[]')
                    if (Array.isArray(p) && p.length === pathArray.length && p.every((v, i) => v === pathArray[i])) {
                        matches.push(cand)
                    }
                } catch (_) { /* ignore */ }
            }
            if (matches.length === 0) return false

            // Prefer elements that look like the expected container class (avoids transparent descendants)
            let candidates = matches
            if (expectedClass) {
                const typed = matches.filter(el => el.classList && el.classList.contains(expectedClass))
                if (typed.length > 0) candidates = typed
            }

            // Choose the shallowest element (closest to the root) to represent the subtree root
            let el = null
            let bestDepth = Number.POSITIVE_INFINITY
            for (const cand of candidates) {
                let depth = 0, cur = cand
                while (cur && cur !== document.body) { depth++; cur = cur.parentElement }
                if (depth < bestDepth) { bestDepth = depth; el = cand }
            }
            if (!el || !el.parentElement) return false
            const parent = el.parentElement
            const idx = Array.prototype.indexOf.call(parent.children, el)

            // Create a temporary wrapper and render into it so dataset.path is correct
            const wrapper = document.createElement('div')
            // Inherit background from current parent to keep look stable during render
            const bg = parent ? (getComputedStyle(parent).backgroundColor || null) : null
            const inherited = { backgroundColor: bg }
            this.renderNode(node, wrapper, inherited, pathArray)
            const fresh = wrapper.firstElementChild
            if (fresh) {
                parent.replaceChild(fresh, parent.children[idx])
                // What was just put on the page is new elements, which know nothing about any
                // filter over them, or about the table they landed in. Without this a filtered
                // list quietly refills the moment anything else nearby changes - the same shape
                // as the tab that lost its content on a repaint - and a repainted cell drops out
                // of its column.
                try { this.arrangeTables() } catch (_) { /* never break a repaint over a view */ }
                try { this.applyFilters() } catch (_) { /* never break a repaint over a view */ }
                return true
            }
        } catch (e) {
            if (DEBUG_MODE) console.warn('Failed to re-render subtree:', e)
        }
        return false
    }
    
    /**
     * Update DOM for cascade fields that were changed by backend processing
     */
    updateDocumentForCascadeFields(oldDocument, newDocument, userChangedFields, cascadeFields = null) {
        return profiled('  updateDocumentForCascadeFields', () => this._updateDocumentForCascadeFields(oldDocument, newDocument, userChangedFields, cascadeFields))
    }
    _updateDocumentForCascadeFields(oldDocument, newDocument, userChangedFields, cascadeFields = null) {
        if (DEBUG_MODE) console.log('🔄 Updating DOM for cascade fields after backend processing')
        // Use provided cascade fields if available, otherwise compute them
        let fieldsToUpdate = cascadeFields
        if (!fieldsToUpdate) {
            // Find all fields that changed between old and new documents
            const allChangedFields = this.findAllChangedFields(oldDocument, newDocument, '')
            // Filter out user-changed fields to get only cascade fields
            fieldsToUpdate = allChangedFields.filter(field => !userChangedFields.includes(field))
        }
    if (DEBUG_MODE) console.log('🎯 Cascade fields to update:', fieldsToUpdate)
        // Update DOM for each cascade field
        for (let fieldPath of fieldsToUpdate) {
            // If the change points to a nested property like '/value', repaint the node element itself
            if (fieldPath.endsWith('/value')) {
                fieldPath = fieldPath.slice(0, -('/value'.length))
            }
            try {
                this.updateSingleFieldInDOM(oldDocument, newDocument, fieldPath)
            } catch (e) {
                if (DEBUG_MODE) console.warn(`Failed to update cascade field ${fieldPath}:`, e)
            }
        }
    }

    /**
     * Recursively collect changed field paths
     */
    collectChangedFields(node1, node2, currentPath, changedFields) {
        if (!node1 || !node2) return
        // Normalize base path so it always includes the full chain from the root.
        const basePath = currentPath || node1.name || ''

        // Check computed parameters for changes
        if (node1.parameters && node2.parameters) {
            for (const [key, value1] of Object.entries(node1.parameters)) {
                if (key.startsWith('_computed_')) {
                    const value2 = node2.parameters[key]
                    if (!this.valuesEqual(value1, value2)) {
                        const fieldName = key.replace('_computed_', '')
                        const fieldPath = fieldName === 'value' ? basePath : `${basePath}/${fieldName}`
                        changedFields.push(fieldPath)
                        if (DEBUG_MODE) console.log(`📝 Detected cascade change: ${fieldPath}`)
                    }
                }
            }
            // Also detect plain value changes (when computed absent but raw differs)
            try {
                const v1 = node1.parameters.value
                const v2 = node2.parameters.value
                const eq = (a,b) => JSON.stringify(a) === JSON.stringify(b)
                if (!eq(v1, v2)) {
                    changedFields.push(basePath)
                    if (DEBUG_MODE) console.log(`📝 Detected raw value change: ${basePath}`)
                }
            } catch(_) {}
        }

        // Recursively check children (propagating full path prefix)
        if (node1.children && node2.children) {
            for (let i = 0; i < Math.min(node1.children.length, node2.children.length); i++) {
                const child1 = node1.children[i]
                const child2 = node2.children[i]
                if (child1 && child2 && child1.name === child2.name) {
                    const childPath = basePath ? `${basePath}/${child1.name}` : child1.name
                    this.collectChangedFields(child1, child2, childPath, changedFields)
                }
            }
        }
    }

    /**
     * Update a single field in the DOM based on document comparison
     */
    updateSingleFieldInDOM(oldDocument, newDocument, fieldPath) {
    if (DEBUG_MODE) console.log(`🔄 Updating single field in DOM: ${fieldPath}`)
        
        // Find the DOM element for this field using the same logic as selective updates
        const elements = document.querySelectorAll(`[data-path]`)
        
        for (const element of elements) {
            try {
                const elementPathArr = JSON.parse(element.dataset.path || '[]')
                const elementPath = elementPathArr.join('/')
                // Match exact path or parent-of-field (e.g., element 'g' for field 'g/value')
                if (elementPath === fieldPath || fieldPath.startsWith(elementPath + '/')) {
                    const newNode = this.findNodeByPath(newDocument, elementPathArr)
                    if (newNode) {
                        if (DEBUG_MODE) console.log(`📝 Updating cascade field via node re-render: ${fieldPath} -> element ${elementPath}`)
                        this.updateSingleElement(element, newNode, elementPathArr)
                    }
                }
            } catch (e) {
                if (DEBUG_MODE) console.warn(`Failed to update element for ${fieldPath}:`, e)
            }
        }
    }

    /**
     * Get computed value from document for a specific field path
     */
    getComputedValueFromDocument(documentArray, fieldPath) {
        // Handle array document structure
        if (Array.isArray(documentArray)) {
            // Find the node with the matching name
            const targetNode = documentArray.find(node => node.name === fieldPath)
            if (targetNode && targetNode.parameters) {
                // Check for computed value first, then regular value
                if (targetNode.parameters._computed_value) {
                    return targetNode.parameters._computed_value
                }
                if (targetNode.parameters.value) {
                    return targetNode.parameters.value
                }
            }
        }
        
        return undefined
    }

    /**
     * Update element's display value
     */
    updateElementDisplayValue(element, newValue) {
        if (!element) return

        // Extract display value from OverseerValue
        let displayValue = newValue
        if (typeof newValue === 'object' && newValue !== null) {
            if (newValue.Integer !== undefined) displayValue = newValue.Integer
            else if (newValue.Float !== undefined) displayValue = newValue.Float
            else if (newValue.String !== undefined) displayValue = newValue.String
            else if (newValue.Boolean !== undefined) displayValue = newValue.Boolean
        }

        element.textContent = displayValue
    if (DEBUG_MODE) console.log(`✅ Updated element display to: ${displayValue}`)
    }

    /**
     * Compare two values for equality
     */
    valuesEqual(val1, val2) {
        if (val1 === val2) return true
        if (!val1 || !val2) return false
        
        if (typeof val1 === 'object' && typeof val2 === 'object') {
            return JSON.stringify(val1) === JSON.stringify(val2)
        }
        
        return false
    }

    /**
     * Fallback method for updating fields by comparing old vs new documents
     */
    updateFieldByComparison(oldDocument, newDocument, fieldPath) {
        // Find all elements that might match this field path
        const elements = document.querySelectorAll(`[data-path]`)
        
        for (const element of elements) {
            try {
                const elementPath = JSON.parse(element.dataset.path || '[]')
                const elementPathStr = elementPath.join('/')
                
                // Check if this element's path matches or is a parent of the changed field
                if (elementPathStr === fieldPath || fieldPath.startsWith(elementPathStr + '/')) {
                    if (DEBUG_MODE) console.log('🎯 Found matching element for path:', elementPathStr)
                    
                    // Find the corresponding node in the new document
                    const newNode = this.findNodeByPath(newDocument, elementPath)
                    const oldNode = this.findNodeByPath(oldDocument, elementPath)
                    
                    if (newNode && oldNode) {
                        // Check if the node actually changed
                        if (DEBUG_MODE) console.log('🔍 Comparing nodes:', {
                            oldValue: this.getNodeValue(oldNode),
                            newValue: this.getNodeValue(newNode),
                            oldComputed: oldNode.parameters?._computed_value,
                            newComputed: newNode.parameters?._computed_value
                        })
                        
                        if (this.nodeHasChanged(oldNode, newNode)) {
                            if (DEBUG_MODE) console.log('📝 Updating element for changed node:', newNode.name)
                            this.updateSingleElement(element, newNode, elementPath)
                        } else {
                            if (DEBUG_MODE) console.log('⏭️ Node unchanged, skipping:', newNode.name)
                        }
                    } else {
                        if (DEBUG_MODE) console.log('❌ Node not found:', { newNode: !!newNode, oldNode: !!oldNode, path: elementPath })
                    }
                }
            } catch (e) {
                if (DEBUG_MODE) console.warn('Error processing element for selective update:', e)
            }
        }
    }

    /**
     * Find a node in the document by its path array
     */
    findNodeByPath(document, pathArray) {
        const matchAt = (nodes, baseName, index) => {
            const matches = nodes.filter(child => child.name === baseName)
            return index < matches.length ? matches[index] : null
        }

        // Unnamed wrapper divs are hierarchy-transparent: they contribute no segment to a
        // rendered path, so resolution has to look through them too. Without this, fields
        // grouped inside such a wrapper (a common layout idiom) resolve to null, which in
        // turn makes the mutability walk fall back to the default and blocks editing.
        const isTransparent = (node) => {
            if (!node) return false
            if (node.is_hierarchy_transparent === true) return true
            const name = String(node.name || '').trim()
            const type = String(node.node_type || node.type || '').trim()
            return !name || name.toLowerCase() === type.toLowerCase()
        }

        const matchThroughTransparent = (nodes, baseName, index) => {
            const direct = matchAt(nodes, baseName, index)
            if (direct) return direct
            // Breadth-first across transparent wrappers only, bounded like the other resolvers.
            let frontier = nodes.slice()
            for (let depth = 0; depth < 4 && frontier.length; depth++) {
                const next = []
                for (const n of frontier) {
                    if (!isTransparent(n) || !Array.isArray(n.children)) continue
                    const candidate = matchAt(n.children, baseName, index)
                    if (candidate) return candidate
                    next.push(...n.children)
                }
                frontier = next
            }
            return null
        }

        let current = { children: document }

        for (const pathSegment of pathArray) {
            if (!current.children) return null

            // Handle array-style names with ordinals (e.g., "item#1")
            const [baseName, ordinal] = pathSegment.includes('#') ?
                pathSegment.split('#') : [pathSegment, '0']

            const next = matchThroughTransparent(current.children, baseName, parseInt(ordinal, 10))
            if (!next) return null
            current = next
        }

        return current
    }

    /**
     * Check if a node has actually changed between old and new versions
     */
    nodeHasChanged(oldNode, newNode) {
        // Compare key properties that would affect rendering
        if (oldNode.node_type !== newNode.node_type) {
            if (DEBUG_MODE) console.log('🔄 Node type changed:', oldNode.node_type, '->', newNode.node_type)
            return true
        }
        
        // Compare the actual values using getNodeValue
        const oldValue = this.getNodeValue(oldNode)
        const newValue = this.getNodeValue(newNode)
        
        if (JSON.stringify(oldValue) !== JSON.stringify(newValue)) {
            if (DEBUG_MODE) console.log('🔄 Node value changed:', oldValue, '->', newValue)
            return true
        }
        
        // Compare computed values if they exist
        const oldComputed = oldNode.parameters?._computed_value
        const newComputed = newNode.parameters?._computed_value
        
        if (JSON.stringify(oldComputed) !== JSON.stringify(newComputed)) {
            if (DEBUG_MODE) console.log('🔄 Computed value changed:', oldComputed, '->', newComputed)
            return true
        }
        
    if (DEBUG_MODE) console.log('🔄 No relevant changes detected in node:', oldNode.name)
        return false
    }

    /**
     * Update a single DOM element with new node data
     */
    updateSingleElement(element, newNode, pathArray) {
        try {
            // Handle different node types
            const nodeType = (newNode.node_type || newNode.type || '').toLowerCase()
            
            switch (nodeType) {
                case '-':
                case 'list_item': {
                    // Simple list entries render a span.overseer-list-value inside the container
                    let holder = element.querySelector('.overseer-list-value')
                    const val = this.getNodeValue(newNode)
                    if (!holder) {
                        // For typed list items rendered via field components, try common holders
                        holder = element.querySelector('.field-value') || element.querySelector('.text-content')
                    }
                    if (holder) {
                        const txt = (val === null || val === undefined) ? '' : String(val)
                        if (holder.textContent !== txt) holder.textContent = txt
                        if (DEBUG_MODE) console.log('📝 Updated list item value:', txt)
                        break
                    }
                    // As a last resort, don't clobber the whole container; create the span
                    try {
                        const span = document.createElement('span')
                        span.className = 'overseer-list-value'
                        span.textContent = (val === null || val === undefined) ? '' : String(val)
                        element.appendChild(span)
                        if (DEBUG_MODE) console.log('📝 Inserted new list value holder for selective update')
                    } catch (_) { /* no-op */ }
                    break
                }
                case 'string':
                case 'text':
                    this.updateTextElement(element, newNode)
                    break
                    
                case 'int':
                case 'float':
                    this.updateNumericElement(element, newNode)
                    break
                case 'date':
                case 'timestamp':
                    this.updateTextElement(element, newNode)
                    break
                case 'bool':
                    this.updateCheckboxElement(element, newNode)
                    break
                case 'button':
                    // No visual incremental update needed
                    return true
                case 'checkbox':
                    this.updateCheckboxElement(element, newNode)
                    break
                case 'div':
                    // Treat generic div as a structural container. We can't trivially patch inner values
                    // because children may have changed (new computed values). Re-render just this subtree.
                    try {
                        if (DEBUG_MODE) console.log('🔁 Re-rendering div container subtree for selective update:', pathArray.join('/'))
                        this.rerenderSubtree([...(Array.isArray(newNode)?newNode:[newNode])], pathArray)
                        return true
                    } catch(e) {
                        if (DEBUG_MODE) console.warn('Div selective subtree re-render failed, falling back:', e)
                        return false
                    }
                    break
                case 'list':
                case 'tab':
                    // Re-render container subtree for lists and tabs
                    try {
                        if (DEBUG_MODE) console.log(`🔁 Re-rendering ${nodeType} container subtree for selective update:`, pathArray.join('/'))
                        // Render directly with provided node to avoid path resolution mismatches
                        const parent = element.parentElement
                        if (!parent) return false
                        const idx = Array.prototype.indexOf.call(parent.children, element)
                        const wrapper = document.createElement('div')
                        const bg = parent ? (getComputedStyle(parent).backgroundColor || null) : null
                        this.renderNode(newNode, wrapper, { backgroundColor: bg }, pathArray)
                        const fresh = wrapper.firstElementChild
                        if (fresh) {
                            parent.replaceChild(fresh, parent.children[idx])
                            return true
                        }
                    } catch (e) {
                        if (DEBUG_MODE) console.warn(`${nodeType} selective subtree re-render failed, falling back:`, e)
                    }
                    return false
                default:
                    // Unknown/custom component (e.g., WeightRecord). Re-render subtree like a container.
                    try {
                        if (DEBUG_MODE) console.log('🔁 Re-rendering custom container subtree for selective update:', pathArray.join('/'), 'type=', nodeType)
                        const parent = element.parentElement
                        if (!parent) return false
                        const idx = Array.prototype.indexOf.call(parent.children, element)
                        const wrapper = document.createElement('div')
                        const bg = parent ? (getComputedStyle(parent).backgroundColor || null) : null
                        this.renderNode(newNode, wrapper, { backgroundColor: bg }, pathArray)
                        const fresh = wrapper.firstElementChild
                        if (fresh) {
                            parent.replaceChild(fresh, parent.children[idx])
                            return true
                        }
                    } catch (e) {
                        if (DEBUG_MODE) console.warn('Custom container selective subtree re-render failed, falling back:', e)
                    }
                    return false
            }
            
            return true
        } catch (error) {
            if (DEBUG_MODE) console.warn('Failed to update single element:', error)
            return false
        }
    }

    /**
     * Update a text/string element
     */
    updateTextElement(element, newNode) {
        // Only update dedicated content holders to avoid corrupting container markup
        // Prefer markdown container when present
        const textElement = element.querySelector('.text-content') || element.querySelector('.field-content, .field-value')
        if (!textElement) {
            if (DEBUG_MODE) console.log('⏭️ No inner text holder found; skipping container text update')
            return
        }
        const computedValue = this.getNodeValue(newNode)
        // If this is a markdown-enabled block, update innerHTML; otherwise textContent
        if (textElement.classList.contains('text-content') && textElement.classList.contains('markdown-enabled')) {
            const html = this.renderMarkdown(String(computedValue ?? ''))
            if (textElement.innerHTML !== html) {
                textElement.innerHTML = html
                if (DEBUG_MODE) console.log('📝 Updated markdown element')
            }
        } else {
            if (textElement.textContent !== computedValue) {
                textElement.textContent = computedValue
                if (DEBUG_MODE) console.log('📝 Updated text element:', computedValue)
            }
        }
    }

    /**
     * Update a numeric input element
     */
    updateNumericElement(element, newNode) {
        const input = element.querySelector('input[type="number"], input[type="text"]')
        if (input) {
            const computedValue = this.getNodeValue(newNode)
            if (input.value !== computedValue) {
                input.value = computedValue
                if (DEBUG_MODE) console.log('📝 Updated numeric element:', computedValue)
            }
        } else {
            // Only update if there's a known text holder; otherwise skip
            this.updateTextElement(element, newNode)
        }
    }

    /**
     * Update a checkbox element
     */
    updateCheckboxElement(element, newNode) {
        const checkbox = element.querySelector('input[type="checkbox"]')
        if (checkbox) {
            const value = this.getNodeValue(newNode)
            const isChecked = value === true || value === 'true'
            if (checkbox.checked !== isChecked) {
                checkbox.checked = isChecked
                if (DEBUG_MODE) console.log('📝 Updated checkbox element:', isChecked)
            }
        }
    }
}
