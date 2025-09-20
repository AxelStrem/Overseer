// Debug UI is disabled unless explicitly enabled via ?debug=1 (localStorage flag ignored)
const DEBUG_MODE = (() => {
    try {
        const qs = typeof window !== 'undefined' && window.location && typeof window.location.search === 'string' ? window.location.search : ''
        return !!(qs && qs.includes('debug=1'))
    } catch { return false }
})();

// Import marked for markdown rendering
import { marked } from 'marked';
import { invoke } from '@tauri-apps/api/tauri'

// Import Chart.js for chart visualization
import {
    Chart,
    CategoryScale,
    LinearScale,
    PointElement,
    LineElement,
    LineController,
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
    Title,
    Tooltip,
    Legend
);

export class OverseerRenderer {
    constructor() {
        this.contentDisplay = document.getElementById('content-display')
        this.tabContainer = document.getElementById('tab-container')
        // Track live intervals so we can clear them on each full re-render
        this._liveIntervals = new Set()
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
            // Ensure required schema fields exist on the new item
            if (newItem.is_hierarchy_transparent === undefined) newItem.is_hierarchy_transparent = (tmpl && typeof tmpl.is_hierarchy_transparent === 'boolean') ? tmpl.is_hierarchy_transparent : false
            // Mark as originating from a template to help selective UI rerenders detect templated instances
            try { newItem.parameters = Object.assign({}, newItem.parameters || {}, { _from_template: true }) } catch (_) {}
            // Assign a unique instance name similar to backend logic (T__N)
            const ordinal = listNode.children.length + 1
            newItem.name = `${tmplName || newItem.name}__${ordinal}`
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
            // Insert into list honoring requested position
            const pos = (options && typeof options.position === 'string') ? options.position.toLowerCase() : 'append'
            if (pos === 'prepend') {
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
            // Find the actual index of the newly inserted item (works for both append and prepend)
            const idxNew = siblings.indexOf(newItem)
            const itemBaseName = newItem.name
            const itemOrd = siblings.slice(0, idxNew).filter(c => c && c.name === itemBaseName).length
            const itemSeg = itemOrd > 0 ? `${itemBaseName}#${itemOrd}` : itemBaseName
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
            return realPathArr.join('/')
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

    shouldRenderChild(parentNode, childNode) {
        const parentType = (parentNode?.node_type || parentNode?.type || '').toLowerCase()
        const childName = childNode?.name
        const childType = (childNode?.node_type || childNode?.type || '').toLowerCase()

    // Timer nodes are visual now; don't filter them out
        // Hide any event handler containers and action statements anywhere
        if (this.isEventHandlerName(childName) || this.isActionName(childName)) return false
        
        // Hide plot nodes - they are configuration for chart nodes, not visual elements
        if (childType === 'plot') return false
        
        // Respect hidden=true on child nodes
        try {
            const hid = this.getParameterValue(childNode, 'hidden')
            if (hid === true || String(hid).toLowerCase() === 'true') return false
        } catch(_) {}
        // For buttons specifically, do not render any children other than explicit visual content (none today)
        if (parentType === 'button') return false
        return true
    }

    renderDocument(overseerDocument) {
        if (DEBUG_MODE) {
            console.log('Rendering document:', overseerDocument)
            console.log('Document type:', typeof overseerDocument)
            console.log('Document is array:', Array.isArray(overseerDocument))
            console.log('Document length:', overseerDocument?.length)
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
        this.contentDisplay.innerHTML = ''
        this.tabContainer.innerHTML = ''

        // Optional lightweight debug info (avoid dumping full document JSON)
        if (DEBUG_MODE) {
            const debugInfo = document.createElement('div')
            debugInfo.style.cssText = 'background: #f0f0f0; padding: 6px 10px; margin: 8px; border: 1px solid #ccc; font-family: monospace; color: #000;'
            const rootCount = Array.isArray(overseerDocument) ? overseerDocument.length : 1
            debugInfo.textContent = `DEBUG: roots=${rootCount} type=${typeof overseerDocument}`
            this.contentDisplay.appendChild(debugInfo)
        }

        if (!overseerDocument) {
            console.error('Document is null or undefined')
            this.contentDisplay.innerHTML += '<p>No document provided</p>'
            return
        }

        if (Array.isArray(overseerDocument)) {
            if (DEBUG_MODE) console.log('Processing array document with', overseerDocument.length, 'nodes')

            if (overseerDocument.length === 0) {
                this.contentDisplay.innerHTML += '<p>Document is empty (no nodes parsed)</p>'
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
            console.warn('Unexpected document format:', overseerDocument)
            this.contentDisplay.innerHTML += '<p>Unexpected document format</p>'
        }

    if (DEBUG_MODE) console.log('Content display after rendering:', this.contentDisplay.innerHTML)
    }

    renderNode(node, container, inheritedStyles = {}, path = []) {
    if (DEBUG_MODE) console.log('renderNode called with:', node, 'container:', container)

        if (!node || typeof node !== 'object') {
            console.warn('Invalid node:', node)
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

        const element = this.createNodeElement(node)
    if (DEBUG_MODE) console.log('Created element:', element)

        if (element) {
            // Attach path metadata for event handling (DOM-only; do not mutate node)
            try {
                if (element.dataset) {
                    element.dataset.path = JSON.stringify(Array.isArray(path) ? path : [])
                }
            } catch (_) { /* no-op */ }

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
                            // Missing-key phantom: render a preview and tag the container
                            this._linkDepth = (this._linkDepth || 0) + 1
                            if (this._linkDepth <= 6) {
                                try { this.renderEventControls(node, element) } catch(_) {}
                                try { element.setAttribute('data-link-proxy', '1') } catch(_) {}
                                // Clear any previous target-path; phantom does not have a concrete target yet
                                try { element.removeAttribute('data-link-target-path') } catch(_) {}
                                try { element.setAttribute('data-link-phantom', JSON.stringify(phantomMeta)) } catch(_) {}
                                // Policy: create on access if requested
                                try {
                                    const policyRaw = this.getParameterValue(node, 'phantom-materialize')
                                    const policy = (policyRaw ? String(policyRaw) : 'none').toLowerCase()
                                    if (policy.endsWith('on-access')) {
                                        const isPrepend = policy.startsWith('prepend')
                                        // Kick off materialization without blocking render
                                        this._materializePhantomAndComputePath(Object.assign({}, phantomMeta), { position: isPrepend ? 'prepend' : 'append' })
                                            .then((realPath) => {
                                                if (!realPath) return
                                                try { element.removeAttribute('data-link-phantom') } catch(_) {}
                                                const pathArr = String(realPath).split('/')
                                                const nodeReal = this.findNodeByPath(window.app.currentDocument, pathArr)
                                                if (nodeReal) {
                                                    // Replace contents with the real node subtree
                                                    try { element.innerHTML = '' } catch(_) {}
                                                    this.renderNode(nodeReal, element, nextInherited, pathArr)
                                                }
                                            })
                                            .catch(() => {/* ignore */})
                                    }
                                } catch(_) { /* ignore policy errors; fall back to preview */ }
                                // Apply per-link overrides to phantom preview as well
                                let phantomToRender = phantomPreviewNode
                                try {
                                    const overrideSpecs = (node.children || []).filter(ch => ch && !this.isEventHandlerName(ch.name) && !this.isActionName(ch.name))
                                    if (overrideSpecs.length > 0) {
                                        const clone = JSON.parse(JSON.stringify(phantomPreviewNode))
                                        const applyOverrideRecursive = (tNode, oNode) => {
                                            if (!tNode || !oNode) return
                                            const kids = Array.isArray(tNode.children) ? tNode.children : []
                                            const exact = kids.find(c => c && c.name === oNode.name)
                                            let targetMatch = exact || (tNode.name === oNode.name ? tNode : null)
                                            const mergeParams = (target, src) => {
                                                if (!src || !target) return
                                                const sp = src.parameters || {}
                                                if (!target.parameters) target.parameters = {}
                                                for (const k of Object.keys(sp)) target.parameters[k] = sp[k]
                                            }
                                            if (targetMatch) {
                                                mergeParams(targetMatch, oNode)
                                                const oKids = Array.isArray(oNode.children) ? oNode.children : []
                                                for (const ok of oKids) applyOverrideRecursive(targetMatch, ok)
                                            }
                                        }
                                        for (const ov of overrideSpecs) applyOverrideRecursive(clone, ov)
                                        phantomToRender = clone
                                    }
                                } catch(_) { /* best-effort only */ }
                                // Render preview under a synthetic path; edits will be intercepted
                                const syntheticPath = path.concat(['<phantom>'])
                                this.renderNode(phantomToRender, element, nextInherited, syntheticPath)
                                this._linkDepth -= 1
                                return
                            } else {
                                if (DEBUG_MODE) console.warn('Link depth exceeded; skipping nested phantom rendering')
                            }
                            this._linkDepth -= 1
                        } else if (targetNode && targetPath && Array.isArray(targetPath)) {
                            // Guard against runaway recursion in case of cycles
                            this._linkDepth = (this._linkDepth || 0) + 1
                            if (this._linkDepth <= 6) {
                                // Clear any stale phantom flag when binding to a real target
                                try { element.removeAttribute('data-link-phantom') } catch(_) {}
                                // Even when acting as a link proxy, expose any event controls (e.g., on click -> button)
                                try { this.renderEventControls(node, element) } catch(_) {}
                                // Mark this container as a link proxy to enable event bubbling on edit
                                try { element.setAttribute('data-link-proxy', '1') } catch(_) {}
                                // Record the concrete target path this proxy is rendering, to aid selective updates
                                try { element.setAttribute('data-link-target-path', JSON.stringify(targetPath)) } catch(_) {}
                                // Apply per-link child overrides by cloning the target and merging override params
                                let toRender = targetNode
                                try {
                                    const overrideSpecs = (node.children || []).filter(ch => ch && !this.isEventHandlerName(ch.name) && !this.isActionName(ch.name))
                                    if (overrideSpecs.length > 0) {
                                        const clone = JSON.parse(JSON.stringify(targetNode))
                                        const applyOverrideRecursive = (tNode, oNode) => {
                                            if (!tNode || !oNode) return
                                            // Try to match by name among immediate children; if not found and names equal, apply to self
                                            const pickChild = (parent, name) => {
                                                const kids = Array.isArray(parent.children) ? parent.children : []
                                                const exact = kids.find(c => c && c.name === name)
                                                return exact || null
                                            }
                                            // Merge parameters from override node into target match
                                            const mergeParams = (target, src) => {
                                                if (!src || !target) return
                                                const sp = src.parameters || {}
                                                if (!target.parameters) target.parameters = {}
                                                for (const k of Object.keys(sp)) {
                                                    // Copy all params; rely on author to avoid conflicting link overrides
                                                    target.parameters[k] = sp[k]
                                                }
                                            }
                                            // Find target child by override name
                                            let targetMatch = pickChild(tNode, oNode.name)
                                            if (!targetMatch && (tNode.name === oNode.name)) targetMatch = tNode
                                            if (targetMatch) {
                                                mergeParams(targetMatch, oNode)
                                                // Recurse for nested overrides
                                                const oKids = Array.isArray(oNode.children) ? oNode.children : []
                                                for (const ok of oKids) applyOverrideRecursive(targetMatch, ok)
                                            }
                                        }
                                        for (const ov of overrideSpecs) applyOverrideRecursive(clone, ov)
                                        toRender = clone
                                    }
                                } catch(_) { /* best-effort only */ }
                                // Render the (possibly overridden) target subtree inside this container
                                this.renderNode(toRender, element, nextInherited, targetPath.slice())
                                // Do not render this node's own children for a link-proxy container
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
                } else {
                    if (DEBUG_MODE) console.log('No children for node:', node)
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
            console.log('[DEBUG] createNodeElement:', { nodeType, node });
            if (node.children && Array.isArray(node.children)) {
                console.log(`[DEBUG] Node ${nodeType} has ${node.children.length} children:`, node.children.map(c => ({ name: c.name, type: c.node_type, parameters: c.parameters })));
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
        const tabButton = document.createElement('button')
        tabButton.className = 'tab-button'
    // Bug 8: Tabs should use a 'label' parameter instead of exposing node name
    tabButton.textContent = this.getParameterValue(node, 'label') || node.name || 'Tab'
        
        const tabContent = document.createElement('div')
        tabContent.className = 'tab-content'
        tabContent.style.display = 'none'
        
        // Add tab button to tab container
        this.tabContainer.appendChild(tabButton)
        
        // Tab click handler
        tabButton.addEventListener('click', () => {
            // Hide all tab contents
            document.querySelectorAll('.tab-content').forEach(content => {
                content.style.display = 'none'
            })
            document.querySelectorAll('.tab-button').forEach(btn => {
                btn.classList.remove('active')
            })
            
            // Show this tab's content
            tabContent.style.display = 'block'
            tabButton.classList.add('active')
        })
        
        // Make first tab active by default
        if (this.tabContainer.children.length === 1) {
            tabButton.classList.add('active')
            tabContent.style.display = 'block'
        }
        
        this.applyNodeStyles(tabContent, node)
        return tabContent
    }

    createDivElement(node) {
        const div = document.createElement('div')
        div.className = 'overseer-div'
        
        if (node.name) {
            div.setAttribute('data-name', node.name)
        }
    // legacy div hidden handling removed in favor of global hidden check
        
        // Apply layout (use effective layout calculated by resolver, or fall back to explicit parameter)
        const layout = this.getEffectiveLayout(node)
        div.classList.add(`layout-${layout}`)
        
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
        
    // Apply spacing and margins  
    this.applyLayoutStyles(list, node)
    this.applyNodeStyles(list, node)
        return list
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
        
        // Make it editable on double-click
        value.addEventListener('dblclick', () => {
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
        
        // Make it editable on double-click
        value.addEventListener('dblclick', () => {
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
                    return n.toFixed(p)
                }
                // No precision specified; render integers without decimals
                if (Number.isInteger(n)) return String(n)
                return String(n)
            }
            // Fallback to string
            return String(v)
        }
        value.textContent = `${pref}${fmtNumber(rawVal)}${suf}`
        
        // Make it editable on double-click
        value.addEventListener('dblclick', () => {
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
        const labelText = node.parameters && node.parameters.label ? node.parameters.label : null;
        if (labelText) {
            const label = document.createElement('label')
            label.textContent = labelText
            container.appendChild(label)
        }
        
        const checkbox = document.createElement('input')
        checkbox.type = 'checkbox'
        checkbox.checked = this.getNodeValue(node) === 'true' || this.getNodeValue(node) === true

        container.appendChild(checkbox)

        // Handle changes
        checkbox.addEventListener('change', () => {
            this.updateNodeValue(node, checkbox.checked)
            if (window.app && window.app.markDocumentModified) {
                window.app.markDocumentModified()
            }
            if (window.app && window.app.reevaluateDocumentSelective) {
                // Try to determine field path for selective update
                try {
                    const fieldPath = this.buildNodePath(container).join('/')
                    window.app.reevaluateDocumentSelective([fieldPath])
                } catch (e) {
                    console.warn('Failed to build field path, falling back to full update:', e)
                    window.app.reevaluateDocumentSelective([])
                }
            }
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

        // Handle checkbox changes with default events: toggle, check, uncheck
        checkbox.addEventListener('change', async () => {
            if (DEBUG_MODE) console.log('Checkbox changed:', node.name, checkbox.checked)
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
                    window.app.reevaluateDocumentSelective([fieldPath])
                } catch (e) {
                    console.warn('Failed to build field path, falling back to full update:', e)
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
        
        // Size handling - support width/height parameters
        const rawW = this.getParameterValue(node, 'width')
        const rawH = this.getParameterValue(node, 'height')
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
        
        // Get plot children
        const plotsAll = (node.children || []).filter(c => (c.node_type||'').toLowerCase() === 'plot')
        
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

            // Ensure label spacing is pleasant
            const label = container.querySelector('label')
            if (label) {
                if (!label.style.marginBottom) label.style.marginBottom = '4px'
                if (!label.style.display) label.style.display = 'block'
            }

            // Value element defaults
            const valueEl = container.querySelector('.field-value') || container.querySelector('.text-content')
            if (valueEl) {
                // Avoid collapsing to 0 height when empty
                if (!valueEl.style.minHeight) valueEl.style.minHeight = '20px'
                // Keep inline-block so borders/padding wrap text nicely
                if (!valueEl.style.display) valueEl.style.display = 'inline-block'
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
                if (!container.style.padding) container.style.padding = '4px 6px'
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
                console.warn('Failed to build field path before editing:', e)
            }
            
            const newValue = input.value
            // Keep showing the previous computed value if a formula was entered/edited
            const prevDisplay = element.textContent
            const isFormulaInput = typeof newValue === 'string' && /\$\([\s\S]*\)/.test(newValue.trim())
            element.textContent = isFormulaInput ? prevDisplay : newValue
            element.style.display = 'inline'
            
            // Safely remove input element
            try {
                input.remove()
            } catch (e) {
                console.warn('Input element already removed:', e)
            }
            
            // If this edit is under a phantom link preview, materialize the item first and recompute a real path
            try {
                let p = element
                while (p && p !== document.body && !p.hasAttribute?.('data-link-phantom')) { p = p.parentElement }
                if (p && p.hasAttribute && p.hasAttribute('data-link-phantom')) {
                    const meta = JSON.parse(p.getAttribute('data-link-phantom') || '{}')
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
                    if (realPath) { fieldPath = realPath }
                    try { p.removeAttribute('data-link-phantom') } catch(_) {}
                }
            } catch(_) {}

            // Update the node value in the document structure (using real path if computed)
            // Instead of using the local node reference, find and update the node in the main document
            let skipElementEvent = false
            if (fieldPath && window.app && window.app.currentDocument) {
                if (DEBUG_MODE) console.log('🔧 Updating node in main document at path:', fieldPath, 'with value:', newValue)
                const success = this.updateNodeValueByPath(window.app.currentDocument, fieldPath, newValue)
                if (!success) {
                    console.warn('⚠️ Failed to update node by path, attempting loose path resolution')
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
                                    console.warn('⚠️ Could not resolve target node via any resolver; falling back to local node update')
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
                } else {
                    console.warn('No field path available, falling back to full update')
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
                    console.warn('Input element already removed:', e)
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
                // 1) Exact name match first
                const exactMatches = nodes.filter(n => exactName(n.name) === wantBase)
                if (wantOrd === 0 && exactMatches.length > 0) return exactMatches[0]
                if (exactMatches.length > wantOrd) return exactMatches[wantOrd]
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
                
                // Update the node value in the document structure
                this.updateNodeValue(node, newValue)
                if (DEBUG_MODE) console.log('Markdown field updated:', node.name, newValue)
                
                // Mark document as modified
                if (window.app && window.app.markDocumentModified) {
                    window.app.markDocumentModified()
                }
                // Trigger reevaluation so formulas/computed params refresh
                if (window.app && window.app.reevaluateDocumentSelective) {
                    // Try to determine field path for selective update
                    try {
                        const fieldPath = this.buildNodePath(element).join('/')
                        window.app.reevaluateDocumentSelective([fieldPath])
                    } catch (e) {
                        console.warn('Failed to build field path, falling back to full update:', e)
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
    async emitEvent(node, element, eventName) {
        if (!window.app || !window.app.currentDocument) return
        const path = (element && element.dataset && element.dataset.path)
            ? JSON.parse(element.dataset.path)
            : (node.__overseer_path || [node.name || node.node_type || node.type || 'root'])
    try { if (DEBUG_MODE) console.debug('[Overseer] emitEvent', eventName, 'path=', path) } catch(_) {}
        const updated = await invoke('execute_overseer_event', {
            nodes: window.app.currentDocument,
            nodePath: path,
            eventName
        })
        // Only replace the document when the backend actually returned a document structure.
        const looksLikeDocArray = Array.isArray(updated) && updated.every(n => n && typeof n === 'object')
        const looksLikeDocObject = updated && typeof updated === 'object' && Array.isArray(updated.children)
        if (looksLikeDocArray || looksLikeDocObject) {
            window.app.currentDocument = looksLikeDocArray ? updated : updated.children
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
                // 1) Exact name match first
                const exactMatches = nodes.filter(n => exactName(n.name) === wantBase)
                if (wantOrd === 0 && exactMatches.length > 0) return exactMatches[0]
                if (exactMatches.length > wantOrd) return exactMatches[wantOrd]
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
                    console.warn('❌ Could not find node at path:', fieldPath, 'missing:', part)
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
                console.warn('❌ Target node not found at path:', fieldPath)
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
            // Use marked library for proper markdown rendering
            return marked.parse(text);
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
                            // Skip re-rendering a proxy if the changed field is inside that proxy's subtree.
                            // This prevents overwriting the just-updated DOM with a stale render.
                            // Also skip if the changed field is inside the proxy's target subtree.
                            let targetPathArr = null
                            try { targetPathArr = JSON.parse(el.getAttribute('data-link-target-path') || 'null') } catch(_) { targetPathArr = null }
                            const targetPathStr = Array.isArray(targetPathArr) ? targetPathArr.join('/') : null
                            const containsChanged = changedFieldPaths.some(cf => cf.startsWith(linkPath + '/'))
                                || (targetPathStr ? changedFieldPaths.some(cf => cf.startsWith(targetPathStr + '/')) : false)
                            if (containsChanged) {
                                if (DEBUG_MODE) console.log('⏭️ Skipping link proxy refresh for', linkPath, 'because it contains changed field(s)')
                                continue
                            }
                            this.rerenderSubtree(newDocument, p)
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
                console.warn(`Failed to update cascade field ${fieldPath}:`, e)
            }
        }
    }

    /**
     * Find all fields that have different computed values between two documents
     */
    findAllChangedFields(node1, node2, currentPath) {
        const changedFields = []
        // Support both node objects and top-level document arrays
        const isArr1 = Array.isArray(node1)
        const isArr2 = Array.isArray(node2)
        if (isArr1 && isArr2) {
            const len = Math.min(node1.length || 0, node2.length || 0)
            for (let i = 0; i < len; i++) {
                const a = node1[i]
                const b = node2[i]
                if (!a || !b) continue
                const childPath = currentPath ? `${currentPath}/${a.name}` : (a.name || '')
                this.collectChangedFields(a, b, childPath, changedFields)
            }
        } else {
            this.collectChangedFields(node1, node2, currentPath, changedFields)
        }
        return changedFields
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
                console.warn(`Failed to update element for ${fieldPath}:`, e)
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
                console.warn('Error processing element for selective update:', e)
            }
        }
    }

    /**
     * Find a node in the document by its path array
     */
    findNodeByPath(document, pathArray) {
        let current = { children: document }
        
        for (const pathSegment of pathArray) {
            if (!current.children) return null
            
            // Handle array-style names with ordinals (e.g., "item#1")
            const [baseName, ordinal] = pathSegment.includes('#') ? 
                pathSegment.split('#') : [pathSegment, '0']
            
            const matches = current.children.filter(child => child.name === baseName)
            const index = parseInt(ordinal, 10)
            
            if (index >= matches.length) return null
            current = matches[index]
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
                default:
                    console.warn('Selective update not implemented for node type:', nodeType)
                    return false
            }
            
            return true
        } catch (error) {
            console.warn('Failed to update single element:', error)
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
