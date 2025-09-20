import { invoke } from '@tauri-apps/api/tauri'

// Debug is opt-in only via ?debug=1; localStorage flag is ignored to avoid accidental noise
const DEBUG_MODE = (() => {
    try {
        const qs = typeof window !== 'undefined' && window.location && typeof window.location.search === 'string' ? window.location.search : ''
        return !!(qs && qs.includes('debug=1'))
    } catch { return false }
})();

// Quiet console noise in non-debug mode while preserving warnings/errors
try {
    if (!DEBUG_MODE) {
        console.debug = () => {}
        const _origLog = console.log.bind(console)
        console.log = () => {}
    }
} catch {}
import { open, save } from '@tauri-apps/api/dialog'
import { appWindow } from '@tauri-apps/api/window'
import { OverseerRenderer } from './renderer.js'
import { FileManager } from './file-manager.js'

// Shallow document equality check used to avoid unnecessary re-renders after scheduler ticks
// If serialization fails, assume different to be safe and apply update.
function docsEqual(a, b) {
    try {
        return JSON.stringify(a) === JSON.stringify(b)
    } catch (_) {
        return false
    }
}

export class OverseerApp {
    constructor() {
        this.currentFile = null
        this.currentDocument = null
        this.isDocumentModified = false
        this.fileManager = new FileManager()
    this.renderer = new OverseerRenderer()
    this._scheduler = { id: null, periodMs: 1000, cachedNextMs: null, inFlight: false, lastTickAt: 0 }
    // Keep the raw original text for comment/whitespace merge on save
    this._originalText = null
    // Track recent user edits to guard against stale backend overwrites during selective refresh
    this._pendingUserEdits = new Map() // path -> { value: OverseerValue|string|number|boolean, ts: ms }
        
        // Make app instance available globally for renderer
        window.app = this
        
        this.initializeEventListeners()
        this.showWelcomeScreen()
    }
    
    // Adopt a new document while preserving the original root array reference if possible so
    // external holders of app.currentDocument (e.g. test harness variables) observe updates.
    _adoptDocumentPreserveRoot(newDoc){
        try{
            if (Array.isArray(this.currentDocument) && Array.isArray(newDoc) && this.currentDocument !== newDoc) {
                this.currentDocument.length = 0
                for(const n of newDoc) this.currentDocument.push(n)
                return
            }
        }catch(_){/* non-fatal */}
        this.currentDocument = newDoc
    }

    // Normalize in-memory document before sending to Rust: convert certain raw booleans
    // in parameters to OverseerValue-shaped objects expected by Serde, e.g., { Boolean: true }.
    // We touch known internal flags that can be set client-side.
    normalizeDocumentForSerialization(doc) {
        const visit = (node) => {
            if (!node || typeof node !== 'object') return
            // Ensure required schema property exists for all nodes
            if (typeof node.is_hierarchy_transparent !== 'boolean') node.is_hierarchy_transparent = false
            const p = node.parameters
            if (p && typeof p === 'object') {
                const fix = (k) => {
                    if (p[k] === true) p[k] = { Boolean: true }
                    if (p[k] === false) p[k] = { Boolean: false }
                }
                fix('_override_present')
                fix('_explicit_child_override')
                // Flags introduced by renderer for UI/rerender hints
                fix('_from_template')
            }
            if (Array.isArray(node.children)) node.children.forEach(visit)
        }
        if (Array.isArray(doc)) doc.forEach(visit)
        else visit(doc)
        return doc
    }

    initializeEventListeners() {
        // File operations
    const openBtn = document.getElementById('open-file-btn')
    if (openBtn) openBtn.addEventListener('click', () => this.openFile())
    const newBtn = document.getElementById('new-file-btn')
    if (newBtn) newBtn.addEventListener('click', () => this.newFile())
    const saveBtn = document.getElementById('save-file-btn')
    if (saveBtn) saveBtn.addEventListener('click', () => this.saveFile())
    const reloadBtn = document.getElementById('reload-file-btn')
    if (reloadBtn) reloadBtn.addEventListener('click', () => this.reloadFile())
        
        // Welcome screen
    const welcomeOpen = document.getElementById('welcome-open-btn')
    if (welcomeOpen) welcomeOpen.addEventListener('click', () => this.openFile())
    const welcomeNew = document.getElementById('welcome-new-btn')
    if (welcomeNew) welcomeNew.addEventListener('click', () => this.newFile())
        
        // Error screen
    const errorBack = document.getElementById('error-back-btn')
    if (errorBack) errorBack.addEventListener('click', () => this.showWelcomeScreen())

        // Keyboard shortcuts
        document.addEventListener('keydown', (e) => {
            if (e.ctrlKey || e.metaKey) {
                switch (e.key) {
                    case 'o':
                        e.preventDefault()
                        this.openFile()
                        break
                    case 's':
                        e.preventDefault()
                        this.saveFile()
                        break
                    case 'n':
                        e.preventDefault()
                        this.newFile()
                        break
                }
            }
        })
    }

    async openFile() {
        try {
            const selected = await open({
                filters: [
                    {
                        name: 'Overseer Files',
                        extensions: ['os']
                    },
                    {
                        name: 'All Files',
                        extensions: ['*']
                    }
                ]
            })

            if (selected) {
                await this.loadFile(selected)
            }
        } catch (error) {
            this.showError('Failed to open file', error)
        }
    }

    async loadFile(filePath) {
        try {
            this.setStatus('Loading file...', filePath, 'info')

            if (DEBUG_MODE) this.setStatus('DEBUG: Starting file load...')

            // Read file content
            const content = await invoke('load_overseer_file', { path: filePath })
            if (DEBUG_MODE) this.setStatus(`DEBUG: File content loaded, length: ${content?.length || 'unknown'}`)

            // Parse the content
            const overseerDocument = await invoke('parse_overseer_content', { content })
            if (DEBUG_MODE) this.setStatus(`DEBUG: Document parsed, type: ${typeof overseerDocument}, length: ${overseerDocument?.length || 'unknown'}`)
            
            // Debug: Check if any chart nodes have computed series
            if (DEBUG_MODE && overseerDocument) {
                const checkNodes = (nodes, depth = 0) => {
                    for (const node of nodes) {
                        if (node.node_type === 'chart') {
                        if (DEBUG_MODE) this.setStatus('DEBUG: Starting file load...') 
                            for (const child of node.children || []) {
                                if (child.node_type === 'plot') {
                                    const computedSeries = child.parameters?._computed_series
                                    console.log(`DEBUG: Plot ${child.name} _computed_series:`, 
                                        computedSeries ? (typeof computedSeries === 'string' ? computedSeries.substring(0, 100) + '...' : computedSeries) : 'null')
                                }
                            }
                        }
                        if (node.children) {
                            checkNodes(node.children, depth + 1)
                        }
                    }
                }
                checkNodes(overseerDocument)
            }

            this.currentFile = filePath
            this._adoptDocumentPreserveRoot(overseerDocument)
            this._originalText = content
            this.isDocumentModified = false

            // Update UI - remove direct file path update since updateTitle handles it now
            document.getElementById('save-file-btn').disabled = false
            document.getElementById('reload-file-btn').disabled = false
            this.updateTitle()

            // Clear any previous content; avoid injecting bulky debug blocks into the document area
            const contentDisplay = document.getElementById('content-display')
            contentDisplay.innerHTML = ''

            if (DEBUG_MODE) this.setStatus('DEBUG: About to call renderer...')
                        this._adoptDocumentPreserveRoot(overseerDocument) 
            // Render the document
            try {
                this.renderer.renderDocument(this.currentDocument)
            } catch (e) {
                console.error('Render error on initial load:', e)
                this.showError('Render error', e)
                return
            }

            if (DEBUG_MODE) this.setStatus('DEBUG: Renderer called, switching to editor screen...')
            this.showEditorScreen()
            
            // Final status update handled by updateTitle()
            if (DEBUG_MODE) this.setStatus('File loaded successfully - DEBUG VERSION')
            
            // Start periodic scheduler tick
            this.startScheduler()
            
        } catch (error) {
            if (DEBUG_MODE) this.setStatus(`DEBUG: Error occurred - ${error.message}`)
            this.showError('Failed to load file', error)
        }
    }

    async newFile() {
        try {
            const selected = await save({
                filters: [
                    {
                        name: 'Overseer Files',
                        extensions: ['os']
                    }
                ]
            })

            if (selected) {
                // Create a basic template
                const template = `// New Overseer file
tab Main {
    div (hidden=true) {
        div Item {
            string Name = ""
            text Description = ""
            int Priority = 0
            date Created = today()
        }
    }
    
    list Data (entry=../Item) {
        // Add your items here
    }
}`

                await invoke('save_overseer_file', { 
                    path: selected, 
                    content: template 
                })

                await this.loadFile(selected)
            }
        } catch (error) {
            this.showError('Failed to create new file', error)
        }
    }

    async saveFile() {
        if (!this.currentDocument) {
            this.setStatus('No document to save', '', 'warning')
            return
        }

        try {
            this.setStatus('Saving file...', this.currentFile, 'info')
            // Best-effort: merge recent user edits (within a short TTL) into currentDocument
            try {
                const now = Date.now()
                const ttlMs = 2000
                if (this._pendingUserEdits && this._pendingUserEdits.size > 0) {
                    for (const [p, rec] of this._pendingUserEdits.entries()) {
                        if (!rec) continue
                        if ((now - (rec.ts||0)) > ttlMs) { this._pendingUserEdits.delete(p); continue }
                        let n = this.getNodeByPath(this.currentDocument, p)
                        if (!n && this.renderer && typeof this.renderer.resolveNodeByPathLoose === 'function') {
                            try { n = this.renderer.resolveNodeByPathLoose(this.currentDocument, p) } catch(_) { n = null }
                        }
                        if (n) {
                            const ov = this.coerceToOverseerValue(n, rec.value)
                            if (!n.parameters) n.parameters = {}
                            n.parameters.value = ov
                        }
                    }
                }
            } catch(_) { /* non-fatal safeguard */ }
            
            // Serialize the current document state to Overseer DSL format (always serialize to keep state in sync for tests)
            const content = await invoke('serialize_overseer_nodes', { 
                nodes: this.normalizeDocumentForSerialization(this.currentDocument) 
            })

            // If no file path is set yet, treat this as a dry-run serialization only
            if (!this.currentFile) {
                this.setStatus('Document serialized (no file selected)', '', 'info')
                // Do not attempt disk writes without a target path
                this.isDocumentModified = false
                this.updateTitle()
                return
            }

            // If we have original raw text, use the merge-sav e API to preserve comments/whitespace
            if (this._originalText != null) {
                await invoke('save_overseer_file_with_original', {
                    path: this.currentFile,
                    regenerated: content,
                    original: this._originalText
                })
                // After saving, refresh our original baseline from disk to keep merges stable
                try {
                    this._originalText = await invoke('load_overseer_file', { path: this.currentFile })
                } catch (_) { /* non-fatal */ }
            } else {
                // Fallback if no baseline is available (e.g., new unsaved file in memory)
                await invoke('save_overseer_file', { 
                    path: this.currentFile, 
                    content 
                })
                // Try to set baseline now
                try { this._originalText = await invoke('load_overseer_file', { path: this.currentFile }) } catch(_) {}
            }
            
            // Mark document as saved
            this.isDocumentModified = false
            this.updateTitle() // This will update status to show "All changes saved"
            
        } catch (error) {
            this.setStatus('Failed to save file', error.message, 'error')
            this.showError('Failed to save file', error)
        }
    }

    /**
     * Update only specific fields in the current document instead of replacing the entire document
     */
    updateDocumentSelectively(currentDocument, resolvedDocument, changedFieldPaths) {
    if (DEBUG_MODE) console.log('🔧 Selectively updating document fields:', changedFieldPaths)
        
        for (const fieldPath of changedFieldPaths) {
            try {
                // Find the field in both documents and update the current one
                const newValue = this.getFieldValueByPath(resolvedDocument, fieldPath)
                if (newValue !== undefined) {
                    this.setFieldValueByPath(currentDocument, fieldPath, newValue)
                    if (DEBUG_MODE) console.log('🔧 Updated field:', fieldPath, 'to:', newValue)
                }
            } catch (error) {
                console.warn('⚠️ Failed to update field selectively:', fieldPath, error)
            }
        }
    }

    /**
     * Get a field value by path from a document
     */
    getFieldValueByPath(document, fieldPath) {
        const pathParts = fieldPath.split('/')
        let current = { children: document }
        
        for (const part of pathParts) {
            if (current.children) {
                current = current.children.find(child => child.name === part)
                if (!current) return undefined
            } else {
                return undefined
            }
        }
        
        return current.parameters?.value
    }

    /**
     * Set a field value by path in a document
     */
    setFieldValueByPath(document, fieldPath, newValue) {
        const pathParts = fieldPath.split('/')
        let current = { children: document }
        
        for (const part of pathParts) {
            if (current.children) {
                current = current.children.find(child => child.name === part)
                if (!current) return false
            } else {
                return false
            }
        }
        
        if (current.parameters) {
            current.parameters.value = newValue
            return true
        }
        return false
    }

    // Robust node resolver by canonical field path, tolerant to instance suffixes ("__N"),
    // ordinal segments ("#k"), and transparent wrappers.
    getNodeByPath(document, fieldPath) {
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
                if (!Array.isArray(currentNodes)) return null
                // 1) Try direct among current level
                let nextNode = findMatches(currentNodes, base, ord)
                // 2) If not found, walk across transparent wrappers (BFS up to a small depth)
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
        } catch(_) { return null }
    }

    // Coerce a plain JS value/string to an OverseerValue based on node_type when possible
    coerceToOverseerValue(node, raw) {
        const asString = (v) => (v == null) ? '' : String(v)
        const t = (node?.node_type || node?.type || '').toLowerCase()
        if (typeof raw === 'object' && raw !== null && (raw.Integer!=null || raw.Float!=null || raw.Boolean!=null || raw.String!=null || raw.Null!==undefined || raw.Timestamp!=null || raw.Date!=null || raw.Formula!=null)) {
            return raw // Already shaped
        }
        if (raw == null || (typeof raw === 'string' && raw.trim() === '')) {
            return { Null: null }
        }
        if (t === 'int') {
            const n = parseInt(asString(raw), 10)
            if (!Number.isNaN(n)) return { Integer: n }
        }
        if (t === 'float') {
            const n = parseFloat(asString(raw))
            if (!Number.isNaN(n)) return { Float: n }
        }
        if (t === 'bool' || t === 'boolean') {
            if (raw === true || raw === false) return { Boolean: !!raw }
            const s = asString(raw).toLowerCase()
            if (s === 'true' || s === 'false') return { Boolean: s === 'true' }
        }
        // Default to String
        return { String: asString(raw) }
    }

    /**
     * Check if field changes can be handled as DOM-only updates without backend processing
     */
    canHandleAsDOMOnlyUpdate(fieldChanges) {
        for (const change of fieldChanges) {
            // Check if the new value contains any formulas (starts with $)
            if (typeof change.newValue === 'string' && change.newValue.includes('$')) {
                return false // Contains formulas, needs backend processing
            }
            
            // For numeric values, we need to check if other fields might depend on this field
            // For now, be conservative: only handle simple string fields that are clearly labels/headers
            if (typeof change.newValue !== 'string') {
                return false // Non-string values might be referenced by formulas
            }
            
            // Check if this looks like a header/label field (starts with # or contains "header" in path)
            const isHeaderField = change.path.toLowerCase().includes('header') || 
                                 (typeof change.newValue === 'string' && change.newValue.startsWith('#'))
            
            if (!isHeaderField) {
                return false // Non-header string fields might still be referenced by formulas
            }
        }
        
        return true // All changes are simple header/label strings
    }

    /**
     * Detect if there are cascade changes by comparing old and new documents
     */
    detectCascadeChanges(oldDocument, newDocument, userChangedFields) {
        // Simple comparison: check if any fields other than user-changed fields have different values
        const allChangedFields = this.findChangedFieldsBetweenDocuments(oldDocument, newDocument)
        const cascadeFields = allChangedFields.filter(field => !userChangedFields.includes(field))
        return cascadeFields.length > 0
    }

    // Heuristic: if any remaining Formula in document references one of the changed field base names, we may need a full recompute
    formulasReferenceChangedNames(doc, changedFieldPaths) {
        try {
            const names = new Set(
                (changedFieldPaths || []).map(p => {
                    const seg = (p || '').split('/').pop()
                    return seg || ''
                }).filter(Boolean)
            )
            if (names.size === 0) return false
            const nameRegexes = Array.from(names).map(n => new RegExp(`(^|[^A-Za-z0-9_])${n}([^A-Za-z0-9_]|$)`))
            let found = false
            const visit = (n) => {
                if (!n || typeof n !== 'object') return
                const p = n.parameters || {}
                const val = p.value
                const comp = p._computed_value
                const checkVal = (vv) => {
                    if (vv && typeof vv === 'object' && vv.Formula) {
                        const s = String(vv.Formula)
                        for (const rx of nameRegexes) { if (rx.test(s)) { found = true; return } }
                    }
                }
                checkVal(val)
                checkVal(comp)
                if (Array.isArray(n.children)) for (const c of n.children) { if (found) break; visit(c) }
            }
            if (Array.isArray(doc)) { for (const r of doc) { if (found) break; visit(r) } }
            else visit(doc)
            return found
        } catch(_) { return false }
    }

    /**
     * Scan resolved doc for aggregate formula fields (sum / map-sum patterns) that reference any of the base names
     * in changedFieldPaths. Return their paths so we can eagerly include them in changedFieldPaths for selective DOM update.
     */
    findAggregateDependents(doc, changedFieldPaths) {
        try {
            const changedNames = new Set(
                (changedFieldPaths||[]).map(p => (p||'').split('/').pop()).filter(Boolean)
            )
            if (changedNames.size === 0) return []
            const aggPaths = new Set()
            const aggRegex = /(\.sum\s*\(|\.sum\s*$|sum\s*\(|\.reduce\s*\(|\.count\s*\(|\.map\(|\bmap\b.*\.sum\s*\()/i
            // Precompute changed path segments for structural detection (e.g., edits inside list 'L')
            const changedPathSegments = (changedFieldPaths||[]).map(p => p.split('/'))
            const visit = (node, pathArr) => {
                if (!node || typeof node !== 'object') return
                const p = node.parameters || {}
                const addIfAgg = (vv) => {
                    if (!vv || typeof vv !== 'object' || !vv.Formula) return
                    const s = String(vv.Formula)
                    if (!aggRegex.test(s)) return
                    let direct = false
                    for (const nm of changedNames) {
                        const rx = new RegExp(`(^|[^A-Za-z0-9_])${nm}([^A-Za-z0-9_]|$)`) // word-boundary-ish
                        if (rx.test(s)) { direct = true; break }
                    }
                    if (direct) { aggPaths.add(pathArr.join('/')); return }
                    // Structural heuristic: if formula references a list token (e.g., 'L.map') and any changed path is inside that list
                    // then treat aggregate as dependent.
                    const listRefMatch = s.match(/\b([A-Za-z0-9_]+)\s*\.map\b|\b([A-Za-z0-9_]+)\s*\.sum\b/)
                    if (listRefMatch) {
                        const listName = listRefMatch[1] || listRefMatch[2]
                        if (listName) {
                            for (const segs of changedPathSegments) {
                                // if path contains listName as a segment (not just last) assume edit inside list
                                if (segs.includes(listName)) { aggPaths.add(pathArr.join('/')); break }
                            }
                        }
                    }
                }
                addIfAgg(p.value)
                addIfAgg(p._computed_value)
                if (Array.isArray(node.children)) {
                    for (const ch of node.children) {
                        const childPath = pathArr.concat([ch.name])
                        visit(ch, childPath)
                    }
                }
            }
            if (Array.isArray(doc)) {
                for (const r of doc) visit(r, [r.name])
            } else if (doc) {
                visit(doc, [doc.name])
            }
            // Remove any that are already directly edited
            return Array.from(aggPaths).filter(p => !changedFieldPaths.includes(p))
        } catch(_) { return [] }
    }

    /**
     * Find all fields that have different values between two documents
     */
    findChangedFieldsBetweenDocuments(doc1, doc2) {
        const changedFields = []
        
        // Handle array documents (list of top-level nodes)
        if (Array.isArray(doc1) && Array.isArray(doc2)) {
            // Compare each top-level node
            for (let i = 0; i < Math.max(doc1.length, doc2.length); i++) {
                const node1 = doc1[i]
                const node2 = doc2[i]
                if (node1 && node2) {
                    this.compareDocumentFields(node1, node2, '', changedFields)
                }
            }
        } else {
            // Handle single document object
            this.compareDocumentFields(doc1, doc2, '', changedFields)
        }
        
        return changedFields
    }

    /**
     * Recursively compare document fields and collect paths of changed fields
     */
    compareDocumentFields(node1, node2, currentPath, changedFields) {
        if (!node1 || !node2) return
        // Establish the full path for this node (root-aware). currentPath, when present, should already
        // represent the full path to this node. If empty, seed with this node's name.
        const basePath = currentPath || node1.name || ''

        // Compare all parameter values, not just computed ones
        if (node1.parameters && node2.parameters) {
            for (const [key, value1] of Object.entries(node1.parameters)) {
                const value2 = node2.parameters[key]
                if (!this.valuesEqual(value1, value2)) {
                    if (key === 'value') {
                        changedFields.push(basePath)
                        if (DEBUG_MODE) console.log(`🔍 Field value changed: ${basePath} from`, value1, 'to', value2)
                    } else if (key.startsWith('_computed_')) {
                        const paramName = key.replace('_computed_', '')
                        if (paramName === 'value') {
                            changedFields.push(basePath)
                        } else {
                            changedFields.push(`${basePath}/${paramName}`)
                        }
                        if (DEBUG_MODE) console.log(`🔍 Field computed value changed: ${basePath}/${paramName} from`, value1, 'to', value2)
                    } else if (key === '_computed_value') {
                        // Edge case: some nodes (notably aggregates) retain their original Formula in parameters.value
                        // and only surface numeric updates through _computed_value. If the underlying value is a Formula
                        // and _computed_value changed while raw 'value' object remained identical, we still want to treat
                        // this as a change for selective DOM updates.
                        try {
                            const rawVal1 = node1.parameters.value
                            const rawVal2 = node2.parameters.value
                            const isFormula = (v) => v && typeof v === 'object' && v.Formula
                            if (isFormula(rawVal1) && isFormula(rawVal2) && this.valuesEqual(rawVal1, rawVal2)) {
                                changedFields.push(basePath)
                                if (DEBUG_MODE) console.log('🔍 Aggregate computed change (formula value unchanged):', basePath)
                            }
                        } catch(_) {}
                    }
                }
            }
        }

        // Recursively check children (maintaining full path prefix)
        if (node1.children && node2.children) {
            for (let i = 0; i < Math.max(node1.children.length, node2.children.length); i++) {
                const child1 = node1.children[i]
                const child2 = node2.children[i]
                if (child1 && child2) {
                    const childPath = basePath ? `${basePath}/${child1.name}` : child1.name
                    this.compareDocumentFields(child1, child2, childPath, changedFields)
                }
            }
        }
    }

    /**
     * Compare two values for equality
     */
    valuesEqual(val1, val2) {
        if (val1 === val2) return true
        if (!val1 || !val2) return false
        
        // Handle OverseerValue objects
        if (typeof val1 === 'object' && typeof val2 === 'object') {
            return JSON.stringify(val1) === JSON.stringify(val2)
        }
        
        return false
    }

    async reevaluateDocumentSelective(changedFieldPaths = [], fieldChanges = []) {
        try {
            if (!this.currentDocument) return { domOnly: false }
            
            if (DEBUG_MODE) console.log('🔄 Selective update triggered for fields:', changedFieldPaths)
            if (fieldChanges.length > 0) {
                if (DEBUG_MODE) console.log('📝 Field changes:', fieldChanges)
            }

            // Eagerly apply user edits directly to the in-memory document so any backend (or test stub)
            // operating on the serialized form sees the fresh values even if later preservation logic
            // attempts to restore prior state. This guards against cases where stale values survive
            // into the selective recompute (observed in aggregate test where B edit was not reflected).
            try {
                for (const ch of fieldChanges || []) {
                    if (!ch || !ch.path) continue
                    let n = this.getNodeByPath(this.currentDocument, ch.path)
                    if (!n && this.renderer && typeof this.renderer.resolveNodeByPathLoose === 'function') {
                        try { n = this.renderer.resolveNodeByPathLoose(this.currentDocument, ch.path) } catch(_) { n = null }
                    }
                    if (n) {
                        const ov = this.coerceToOverseerValue(n, ch.newValue)
                        if (!n.parameters) n.parameters = {}
                        // If existing value is a Formula and incoming change is a primitive (number/string), treat as computed update
                        const existingVal = n.parameters.value
                        const isFormula = (v) => v && typeof v === 'object' && v.Formula
                        const isPrimitiveUpdate = ov && typeof ov === 'object' && (('Integer' in ov) || ('Float' in ov) || ('String' in ov) || ('Boolean' in ov))
                        if (isFormula(existingVal) && isPrimitiveUpdate && !ch._allowFormulaOverwrite) {
                            n.parameters._computed_value = ov
                        } else {
                            n.parameters.value = ov
                        }
                    }
                }
            } catch(_) { /* non-fatal eager apply */ }

            // Cache recent user edits (even when the DOM update is pure) for merge-on-resolve
            try {
                const now = Date.now()
                for (const ch of (fieldChanges || [])) {
                    if (!ch || !ch.path) continue
                    this._pendingUserEdits.set(ch.path, { value: ch.newValue, ts: now })
                }
            } catch(_) {}

            // Check if we can handle this as a pure DOM-only update (no backend needed)
            let canHandleDOMOnly = this.canHandleAsDOMOnlyUpdate(fieldChanges)
            // Guard: if any aggregate formulas (sum/map pipelines) could be affected indirectly by this edit
            // (e.g. edit to a list item primitive feeding another item's computed field feeding an aggregate),
            // force backend selective path. We detect:
            // 1) Any formula referencing changed field names directly (handled later, but we short‑circuit here)
            // 2) Any aggregate formula referencing the list identifier for which a descendant field changed
            try {
                if (canHandleDOMOnly) {
                    const aggPattern = /(\.sum\s*\(|\.sum\s*$|\.map\s*\(|\.reduce\s*\(|\.count\s*\()/i
                    const changedPaths = Array.isArray(changedFieldPaths) ? changedFieldPaths : []
                    // Pre-extract list names from changed paths (second segment after root, or any segment preceding a template instance)
                    const changedListNames = new Set()
                    for (const p of changedPaths) {
                        if (!p) continue
                        const segs = p.split('/')
                        for (let i=0;i<segs.length;i++) {
                            const seg = segs[i]
                            if (!seg) continue
                            // Heuristic: treat any segment whose next segment appears to be a template instance or item as a list name
                            if (i < segs.length - 1 && /__\d+$/.test(segs[i+1])) changedListNames.add(seg)
                        }
                        // Also if path explicitly contains a known list node (named 'L') include it
                        if (segs.includes('L')) changedListNames.add('L')
                    }
                    if (changedListNames.size > 0) {
                        const visit = (n) => {
                            if (!n || typeof n !== 'object') return
                            const p = n.parameters || {}
                            const val = p.value
                            const check = (vv) => {
                                if (!vv || typeof vv !== 'object' || !vv.Formula) return false
                                const s = String(vv.Formula)
                                if (!aggPattern.test(s)) return false
                                for (const ln of changedListNames) {
                                    // look for list reference token like 'L.' or ' L ' or '(L.' inside formula
                                    const rx = new RegExp(`(^|[^A-Za-z0-9_])${ln}[^A-Za-z0-9_]`)
                                    if (rx.test(s)) return true
                                }
                                return false
                            }
                            if (check(val)) { canHandleDOMOnly = false; return }
                            if (Array.isArray(n.children) && canHandleDOMOnly) {
                                for (const c of n.children) { if (!canHandleDOMOnly) break; visit(c) }
                            }
                        }
                        if (Array.isArray(this.currentDocument)) {
                            for (const r of this.currentDocument) { if (!canHandleDOMOnly) break; visit(r) }
                        } else { visit(this.currentDocument) }
                    }
                }
            } catch(_) { /* non-fatal heuristic */ }
            
            if (canHandleDOMOnly) {
                if (DEBUG_MODE) console.log('🚀 Handling as DOM-only update (no backend call needed)')
                // Just do the DOM update directly without any backend processing
                if (changedFieldPaths.length > 0) {
                    if (DEBUG_MODE) console.log('🎯 Attempting DOM-only update for specific fields')
                    
                    // Use the current document as both old and new for DOM updates
                    const selectiveUpdateSuccessful = this.renderer.updateSelectiveFields(
                        this.currentDocument, this.currentDocument, changedFieldPaths, fieldChanges
                    )
                    
                    if (selectiveUpdateSuccessful) {
                        if (DEBUG_MODE) console.log('✅ DOM-only update completed successfully (charts completely untouched)')
                        return { domOnly: true, success: true }
                    } else {
                        if (DEBUG_MODE) console.log('⚠️ DOM-only update failed, falling back to backend processing')
                    }
                }
            }
            
            // Store the old document state before backend processing
            const oldDocument = JSON.parse(JSON.stringify(this.currentDocument))
            
            // Serialize current nodes to DSL
            const content = await invoke('serialize_overseer_nodes', { nodes: this.normalizeDocumentForSerialization(this.currentDocument) })
            if (DEBUG_MODE) console.log('📤 Serialized content being sent to backend:', content.substring(0, 500))
            // Parse + resolve + evaluate on backend with selective updates
            // Build a map of changed field values (as OverseerValue-shaped objects) to send to backend
            const changedValuesMap = (() => {
                const map = {}
                try {
                    for (const ch of fieldChanges || []) {
                        if (!ch || !ch.path) continue
                        // Find node to know its type, then coerce value
                        let n = this.getNodeByPath(this.currentDocument, ch.path)
                        if (!n && this.renderer && typeof this.renderer.resolveNodeByPathLoose === 'function') {
                            try { n = this.renderer.resolveNodeByPathLoose(this.currentDocument, ch.path) } catch(_) { n = null }
                        }
                        if (n) {
                            map[ch.path] = this.coerceToOverseerValue(n, ch.newValue)
                        }
                    }
                } catch(_) {}
                return map
            })()
            const resolved = await invoke('parse_overseer_content_selective', { content, changedFields: changedFieldPaths, changedFieldValues: changedValuesMap })
            try {
                // Temporary instrumentation for aggregate debugging: surface total node params
                const findTotal = (doc) => {
                    try {
                        if (Array.isArray(doc)) {
                            for (const r of doc) { if (r && r.name === 'main') {
                                const t = (r.children||[]).find(c=>c && c.name==='total')
                                if (t) return t
                            }}
                        }
                    } catch(_) {}
                    return null
                }
                const tNode = findTotal(resolved)
                if (tNode && tNode.parameters) {
                    console.warn('[AGG TRACE] post-selective resolved total params:', JSON.stringify(tNode.parameters))
                }
            } catch(_) { /* ignore instrumentation errors */ }

            // Proactively augment changedFieldPaths with aggregate dependents whose formulas reference any of the changed base fields.
            // This helps ensure totals like $(L.map(|x| x/C).sum()) update immediately in the selective branch rather than relying
            // solely on later cascade diff detection (which in some nested transparent wrapper cases may miss a direct repaint).
            try {
                const addedAggs = this.findAggregateDependents(resolved, changedFieldPaths)
                if (addedAggs.length > 0) {
                    const before = changedFieldPaths.slice()
                    changedFieldPaths = Array.from(new Set([...changedFieldPaths, ...addedAggs]))
                    if (DEBUG_MODE) console.log('➕ Added aggregate dependents to changedFieldPaths:', { before, addedAggs, after: changedFieldPaths })
                    // Inject synthetic fieldChanges entries so selective DOM update path treats them like direct edits
                    try {
                        for (const ap of addedAggs) {
                            if (!fieldChanges.some(fc => fc.path === ap)) {
                                let aggNode = this.getNodeByPath(resolved, ap)
                                if (!aggNode && this.renderer && typeof this.renderer.resolveNodeByPathLoose === 'function') {
                                    try { aggNode = this.renderer.resolveNodeByPathLoose(resolved, ap) } catch(_) { aggNode = null }
                                }
                                let newDisplay = ''
                                if (aggNode) {
                                    try { newDisplay = this.renderer.getNodeValue(aggNode) } catch(_) {}
                                    // Fallback: extract integer/string primitives
                                    if (newDisplay == null) {
                                        const p = aggNode.parameters || {}
                                        const cv = p._computed_value || p.value
                                        if (cv && typeof cv === 'object') {
                                            if ('Integer' in cv) newDisplay = cv.Integer
                                            else if ('Float' in cv) newDisplay = cv.Float
                                            else if ('String' in cv) newDisplay = cv.String
                                        }
                                    }
                                }
                                // Mark as synthetic aggregate-driven change so we never overwrite the formula itself
                                fieldChanges.push({ path: ap, oldValue: '', newValue: String(newDisplay), _aggregateSynthetic: true })
                            }
                        }
                    } catch(_) { /* non-fatal */ }
                }
            } catch(_) { /* non-fatal */ }

            // Helper: merge recent user edits into a resolved document (TTL ~2s)
            const mergeRecentUserEdits = (doc) => {
                try {
                    const now = Date.now()
                    const ttlMs = 2000
                    if (!this._pendingUserEdits || this._pendingUserEdits.size === 0) return
                    for (const [p, rec] of this._pendingUserEdits.entries()) {
                        if (!rec) continue
                        if ((now - (rec.ts||0)) > ttlMs) { this._pendingUserEdits.delete(p); continue }
                        let n = this.getNodeByPath(doc, p)
                        if (!n && this.renderer && typeof this.renderer.resolveNodeByPathLoose === 'function') {
                            try { n = this.renderer.resolveNodeByPathLoose(doc, p) } catch(_) { n = null }
                        }
                        if (n) {
                            const ov = this.coerceToOverseerValue(n, rec.value)
                            if (!n.parameters) n.parameters = {}
                            n.parameters.value = ov
                        }
                    }
                } catch(_) { /* best-effort */ }
            }
            
            // Instead of full re-render, do selective DOM updates if we have specific changed fields
            if (changedFieldPaths.length > 0) {
                if (DEBUG_MODE) console.log('🎯 Attempting selective DOM update for specific fields')
                
                // Before DOM updates, merge the user edits into the resolved doc so renderer sees new values
                try {
                    if (Array.isArray(fieldChanges) && fieldChanges.length > 0) {
                        for (const ch of fieldChanges) {
                            if (!ch || !ch.path) continue
                            let n = this.getNodeByPath(resolved, ch.path)
                            if (!n && this.renderer && typeof this.renderer.resolveNodeByPathLoose === 'function') {
                                try { n = this.renderer.resolveNodeByPathLoose(resolved, ch.path) } catch(_) { n = null }
                            }
                            if (n) {
                                const ov = this.coerceToOverseerValue(n, ch.newValue)
                                if (!n.parameters) n.parameters = {}
                                const existingVal = n.parameters.value
                                const isFormula = (v) => v && typeof v === 'object' && v.Formula
                                const isPrimitiveUpdate = ov && typeof ov === 'object' && (('Integer' in ov) || ('Float' in ov) || ('String' in ov) || ('Boolean' in ov))
                                if ((ch._aggregateSynthetic || (isFormula(existingVal) && isPrimitiveUpdate && !ch._allowFormulaOverwrite))) {
                                    n.parameters._computed_value = ov
                                } else {
                                    n.parameters.value = ov
                                }
                            }
                        }
                    }
                } catch(_) { /* best-effort pre-merge for DOM update */ }

                // Try selective rendering for the changed fields using field change info
                let selectiveUpdateSuccessful = false
                try {
                    selectiveUpdateSuccessful = this.renderer.updateSelectiveFields(oldDocument, resolved, changedFieldPaths, fieldChanges)
                } catch (e) {
                    console.warn('Selective DOM update failed:', e)
                }
                
                if (selectiveUpdateSuccessful) {
                    // If any aggregate nodes are explicitly in changedFieldPaths, force full re-render for correctness.
                    try {
                        const aggRegex = /(\.sum\s*\(|\.sum\s*$|sum\s*\(|\.reduce\s*\(|\.count\s*\(|\.map\(|map\b.*\.sum\s*\()/i
                        let hasAgg = false
                        for (const pth of changedFieldPaths) {
                            if (hasAgg) break
                            let n = this.getNodeByPath(resolved, pth)
                            if (!n && this.renderer && typeof this.renderer.resolveNodeByPathLoose === 'function') {
                                try { n = this.renderer.resolveNodeByPathLoose(resolved, pth) } catch(_) { n = null }
                            }
                            if (!n) continue
                            const val = n.parameters?.value
                            const comp = n.parameters?._computed_value
                            const check = (vv) => vv && typeof vv === 'object' && vv.Formula && aggRegex.test(String(vv.Formula))
                            if (check(val) || check(comp)) { hasAgg = true; break }
                        }
                        if (hasAgg) {
                            if (DEBUG_MODE) console.log('♻️  Forcing full re-render due to aggregate field(s) in changedFieldPaths')
                            this.currentDocument = resolved
                            try { this.renderer.renderDocument(this.currentDocument) } catch(e) { console.error('Render error (agg force):', e); this.showError('Render error', e) }
                            return { domOnly: false, success: true }
                        }
                    } catch(_) { /* non-fatal */ }
                    // Always refresh DOM for all fields whose values changed between old and new docs.
                    // This covers cases where callers pass dependents in changedFieldPaths (so cascade detection would skip them).
                    try {
                        const allChangedFields = this.findChangedFieldsBetweenDocuments(oldDocument, resolved)
                        if (Array.isArray(allChangedFields) && allChangedFields.length > 0) {
                            this.renderer.updateDocumentForCascadeFields(oldDocument, resolved, [], allChangedFields)
                            // If there are computed/formula-driven cascade updates (e.g. aggregate totals) not explicitly in the
                            // original changedFieldPaths, the existing targeted DOM patch logic may still miss them when the raw
                            // 'value' parameter (a Formula object) is unchanged and only _computed_value updated. To guarantee
                            // correctness for aggregates like total = $(L.map(|x| x/C).sum()), do a one-time full re-render
                            // when we detect additional changed fields outside the user edits.
                            const extraCascade = allChangedFields.filter(f => !changedFieldPaths.includes(f))
                            if (extraCascade.length > 0) {
                                if (DEBUG_MODE) console.log('♻️  Performing full document re-render due to cascade fields:', extraCascade)
                                this.currentDocument = resolved
                                try { this.renderer.renderDocument(this.currentDocument) } catch (e) { console.error('Render error (aggregate cascade re-render):', e); this.showError('Render error', e) }
                                return { domOnly: false, success: true }
                            }
                        }
                    } catch (e) {
                        if (DEBUG_MODE) console.warn('Best-effort dependent field refresh failed:', e)
                    }
                    // The backend has processed cascade dependencies, so we need to update DOM for 
                    // both the user-changed fields AND any cascade fields that were updated
                    
                    // Detect potential cascade changes and formula references
                    const hasCascadeChanges = this.detectCascadeChanges(this.currentDocument, resolved, changedFieldPaths)
                    const hasFormulaRefs = this.formulasReferenceChangedNames(resolved, changedFieldPaths)

                    // If formulas reference changed fields, prefer full resolve immediately to recompute dependents
                    if (hasFormulaRefs) {
                        if (DEBUG_MODE) console.log('ℹ️ Formula references to changed fields detected; performing full resolve fallback')
                        try {
                            const fullContent = await invoke('serialize_overseer_nodes', { nodes: this.normalizeDocumentForSerialization(resolved) })
                            const fullResolved = await invoke('parse_overseer_content', { content: fullContent })
                            // Merge edits into full resolve result as well
                            try {
                                if (Array.isArray(fieldChanges) && fieldChanges.length > 0) {
                                    for (const ch of fieldChanges) {
                                        let n = this.getNodeByPath(fullResolved, ch.path)
                                        if (!n && this.renderer && typeof this.renderer.resolveNodeByPathLoose === 'function') {
                                            try { n = this.renderer.resolveNodeByPathLoose(fullResolved, ch.path) } catch(_) { n = null }
                                        }
                                        if (n) {
                                            const ov = this.coerceToOverseerValue(n, ch.newValue)
                                            if (!n.parameters) n.parameters = {}
                                            n.parameters.value = ov
                                        }
                                    }
                                }
                                mergeRecentUserEdits(fullResolved)
                            } catch(_) {}
                            this.currentDocument = fullResolved
                            if (DEBUG_MODE) console.log('🔁 Applied full resolve fallback due to formula references')
                            try { this.renderer.renderDocument(this.currentDocument) } catch (e) { console.error('Render error (full resolve fallback):', e); this.showError('Render error', e) }
                            return { domOnly: false, success: true }
                        } catch (e) {
                            if (DEBUG_MODE) console.warn('Full resolve fallback failed, continuing with selective result:', e)
                        }
                    } else if (hasCascadeChanges) {
                        if (DEBUG_MODE) console.log('🔄 Cascade changes detected, updating DOM for affected fields')
                        // Get the actual cascade fields that were detected
                        const allChangedFields = this.findChangedFieldsBetweenDocuments(this.currentDocument, resolved)
                        const cascadeFields = allChangedFields.filter(field => !changedFieldPaths.includes(field))
                        
                        // Re-run selective DOM update to include cascade fields
                        try {
                            this.renderer.updateDocumentForCascadeFields(this.currentDocument, resolved, changedFieldPaths, cascadeFields)
                        } catch (e) {
                            console.warn('Failed to update cascade fields in DOM:', e)
                        }
                    } else {
                        if (DEBUG_MODE) console.log('ℹ️ No cascade changes detected after selective update')
                        // Fallback: some dependencies might not be captured in selective mode; do a full resolve
                        try {
                            const fullContent = await invoke('serialize_overseer_nodes', { nodes: this.normalizeDocumentForSerialization(resolved) })
                            const fullResolved = await invoke('parse_overseer_content', { content: fullContent })
                            // Merge edits into full resolve result to avoid losing user changes
                            try {
                                if (Array.isArray(fieldChanges) && fieldChanges.length > 0) {
                                    for (const ch of fieldChanges) {
                                        let n = this.getNodeByPath(fullResolved, ch.path)
                                        if (!n && this.renderer && typeof this.renderer.resolveNodeByPathLoose === 'function') {
                                            try { n = this.renderer.resolveNodeByPathLoose(fullResolved, ch.path) } catch(_) { n = null }
                                        }
                                        if (n) {
                                            const ov = this.coerceToOverseerValue(n, ch.newValue)
                                            if (!n.parameters) n.parameters = {}
                                            n.parameters.value = ov
                                        }
                                    }
                                }
                                mergeRecentUserEdits(fullResolved)
                            } catch(_) {}
                            this.currentDocument = fullResolved
                            if (DEBUG_MODE) console.log('🔁 Applied full resolve fallback to refresh dependent formulas')
                            try { this.renderer.renderDocument(this.currentDocument) } catch (e) { console.error('Render error (full resolve fallback):', e); this.showError('Render error', e) }
                            return { domOnly: false, success: true }
                        } catch (e) {
                            if (DEBUG_MODE) console.warn('Full resolve fallback failed, continuing with selective result:', e)
                        }
                    }
                    
                    // Merge user-changed field values into the resolved document to avoid losing edits
                    try {
                        if (Array.isArray(fieldChanges) && fieldChanges.length > 0) {
                            for (const ch of fieldChanges) {
                                let n = this.getNodeByPath(resolved, ch.path)
                                if (!n && this.renderer && typeof this.renderer.resolveNodeByPathLoose === 'function') {
                                    try { n = this.renderer.resolveNodeByPathLoose(resolved, ch.path) } catch(_) { n = null }
                                }
                                if (n) {
                                    const ov = this.coerceToOverseerValue(n, ch.newValue)
                                    if (!n.parameters) n.parameters = {}
                                    const existingVal = n.parameters.value
                                    const isFormula = (v) => v && typeof v === 'object' && v.Formula
                                    const isPrimitiveUpdate = ov && typeof ov === 'object' && (('Integer' in ov) || ('Float' in ov) || ('String' in ov) || ('Boolean' in ov))
                                    if ((ch._aggregateSynthetic || (isFormula(existingVal) && isPrimitiveUpdate && !ch._allowFormulaOverwrite))) {
                                        n.parameters._computed_value = ov
                                    } else {
                                        n.parameters.value = ov
                                    }
                                }
                            }
                        }
                        // Also merge any other very recent user edits (guard against immediate follow-up refresh)
                        mergeRecentUserEdits(resolved)
                    } catch (_) { /* best-effort merge */ }
                    // Update the current document with the merged resolved result
                    this.currentDocument = resolved
                    
                    if (DEBUG_MODE) console.log('✅ Selective update completed successfully')
                    // Post-selective safety net: Some aggregate nodes may have only _computed_value updated while their
                    // raw Formula in parameters.value stays the same (e.g., total = $(L.map(|x| x/C).sum())). In rare
                    // nested transparent wrapper cases the earlier diff logic could still miss repainting if path
                    // resolution failed or element not found. Perform a lightweight scan: compare old vs resolved for
                    // any node whose parameters.value is a Formula and whose _computed_value differs. If its path is
                    // NOT in changedFieldPaths, inject an immediate DOM refresh of that single element (or fallback
                    // to full re-render if refresh fails).
                    try {
                        const aggCandidates = []
                        const walk = (a, b, pathArr=[]) => {
                            if (!a || !b) return
                            try {
                                const av = a.parameters && a.parameters.value
                                const bv = b.parameters && b.parameters.value
                                const ac = a.parameters && a.parameters._computed_value
                                const bc = b.parameters && b.parameters._computed_value
                                const isFormula = (v) => v && typeof v === 'object' && v.Formula
                                const valEq = this.valuesEqual(av, bv)
                                const compEq = this.valuesEqual(ac, bc)
                                if (isFormula(av) && isFormula(bv) && valEq && !compEq) {
                                    const p = pathArr.join('/')
                                    if (p && !changedFieldPaths.includes(p)) aggCandidates.push(p)
                                }
                            } catch(_) {}
                            const ach = Array.isArray(a.children) ? a.children : []
                            const bch = Array.isArray(b.children) ? b.children : []
                            for (let i=0;i<Math.min(ach.length, bch.length);i++) {
                                const an = ach[i]; const bn = bch[i]
                                if (!an || !bn) continue
                                walk(an, bn, pathArr.concat([an.name]))
                            }
                        }
                        // Support root array
                        const rootsA = Array.isArray(oldDocument) ? oldDocument : [oldDocument]
                        const rootsB = Array.isArray(resolved) ? resolved : [resolved]
                        for (let i=0;i<Math.min(rootsA.length, rootsB.length); i++) {
                            const ra = rootsA[i]; const rb = rootsB[i]
                            if (!ra || !rb) continue
                            walk(ra, rb, [ra.name])
                        }
                        if (aggCandidates.length > 0) {
                            if (DEBUG_MODE) console.log('🛠  Post-scan repainting aggregate candidates:', aggCandidates)
                            for (const p of aggCandidates) {
                                try {
                                    // Attempt single-field DOM refresh by treating as cascade update
                                    this.renderer.updateDocumentForCascadeFields(oldDocument, resolved, [], [p])
                                } catch(_) {}
                            }
                            // After targeted repaint, verify DOM reflects new computed values; if any still stale, force full re-render.
                            try {
                                let stale = false
                                for (const p of aggCandidates) {
                                    if (stale) break
                                    const pathArr = p.split('/')
                                    const el = document.querySelector(`[data-path='${JSON.stringify(pathArr)}']`)
                                    if (!el) continue
                                    const node = this.getNodeByPath(resolved, p)
                                    const valObj = node?.parameters?._computed_value || node?.parameters?.value
                                    let expected = ''
                                    if (valObj && typeof valObj === 'object') {
                                        if ('Integer' in valObj) expected = String(valObj.Integer)
                                        else if ('Float' in valObj) expected = String(valObj.Float)
                                        else if ('String' in valObj) expected = String(valObj.String)
                                    }
                                    const displayHolder = el.querySelector('.field-value, .text-content') || el
                                    const got = (displayHolder.textContent||'').trim()
                                    if (expected && got !== expected) {
                                        stale = true
                                    }
                                }
                                if (stale) {
                                    if (DEBUG_MODE) console.log('♻️  Forcing full re-render due to stale aggregate display after targeted repaint.')
                                    this.currentDocument = resolved
                                    try { this.renderer.renderDocument(this.currentDocument) } catch(e) { console.error('Render error (agg stale fallback):', e); this.showError('Render error', e) }
                                    return { domOnly: false, success: true }
                                }
                            } catch(_) { /* non-fatal */ }
                        }
                    } catch(_) { /* non-fatal */ }
                    return { domOnly: false, success: true }
                } else {
                    if (DEBUG_MODE) console.log('🔄 Falling back to full re-render')
                    // For fallback full re-render path, also preserve user edits by merging them first
                    try {
                        if (Array.isArray(fieldChanges) && fieldChanges.length > 0) {
                            for (const ch of fieldChanges) {
                                let n = this.getNodeByPath(resolved, ch.path)
                                if (!n && this.renderer && typeof this.renderer.resolveNodeByPathLoose === 'function') {
                                    try { n = this.renderer.resolveNodeByPathLoose(resolved, ch.path) } catch(_) { n = null }
                                }
                                if (n) {
                                    const ov = this.coerceToOverseerValue(n, ch.newValue)
                                    if (!n.parameters) n.parameters = {}
                                    const existingVal = n.parameters.value
                                    const isFormula = (v) => v && typeof v === 'object' && v.Formula
                                    const isPrimitiveUpdate = ov && typeof ov === 'object' && (('Integer' in ov) || ('Float' in ov) || ('String' in ov) || ('Boolean' in ov))
                                    if ((ch._aggregateSynthetic || (isFormula(existingVal) && isPrimitiveUpdate && !ch._allowFormulaOverwrite))) {
                                        n.parameters._computed_value = ov
                                    } else {
                                        n.parameters.value = ov
                                    }
                                }
                            }
                        }
                        mergeRecentUserEdits(resolved)
                    } catch(_) {}
                    this.currentDocument = resolved
                    try { this.renderer.renderDocument(this.currentDocument) } catch (e) { console.error('Render error (fallback re-render):', e); this.showError('Render error', e) }
                    // After re-rendering the selective result, if formulas still reference changed fields,
                    // do a full resolve fallback to ensure dependent values are recomputed
                    try {
                        if (this.formulasReferenceChangedNames(resolved, changedFieldPaths)) {
                            if (DEBUG_MODE) console.log('ℹ️ Post-fallback formulas still reference changed fields; performing full resolve')
                            const fullContent = await invoke('serialize_overseer_nodes', { nodes: this.normalizeDocumentForSerialization(resolved) })
                            const fullResolved = await invoke('parse_overseer_content', { content: fullContent })
                            // Merge edits into full resolve result
                            try {
                                if (Array.isArray(fieldChanges) && fieldChanges.length > 0) {
                                    for (const ch of fieldChanges) {
                                        let n = this.getNodeByPath(fullResolved, ch.path)
                                        if (!n && this.renderer && typeof this.renderer.resolveNodeByPathLoose === 'function') {
                                            try { n = this.renderer.resolveNodeByPathLoose(fullResolved, ch.path) } catch(_) { n = null }
                                        }
                                        if (n) {
                                            const ov = this.coerceToOverseerValue(n, ch.newValue)
                                            if (!n.parameters) n.parameters = {}
                                            n.parameters.value = ov
                                        }
                                    }
                                }
                                mergeRecentUserEdits(fullResolved)
                            } catch(_) {}
                            this.currentDocument = fullResolved
                            try { this.renderer.renderDocument(this.currentDocument) } catch (e) { console.error('Render error (full resolve after fallback):', e); this.showError('Render error', e) }
                        }
                    } catch (_) { /* non-fatal */ }
                    return { domOnly: false, success: true }
                }
            } else {
                // No specific fields; prefer an in-place DOM update of cascade fields to preserve element identity
                try {
                    if (Array.isArray(fieldChanges) && fieldChanges.length > 0) {
                        for (const ch of fieldChanges) {
                            let n = this.getNodeByPath(resolved, ch.path)
                            if (!n && this.renderer && typeof this.renderer.resolveNodeByPathLoose === 'function') {
                                try { n = this.renderer.resolveNodeByPathLoose(resolved, ch.path) } catch(_) { n = null }
                            }
                            if (n) {
                                const ov = this.coerceToOverseerValue(n, ch.newValue)
                                if (!n.parameters) n.parameters = {}
                                    const existingVal = n.parameters.value
                                    const isFormula = (v) => v && typeof v === 'object' && v.Formula
                                    const isPrimitiveUpdate = ov && typeof ov === 'object' && (('Integer' in ov) || ('Float' in ov) || ('String' in ov) || ('Boolean' in ov))
                                    if ((ch._aggregateSynthetic || (isFormula(existingVal) && isPrimitiveUpdate && !ch._allowFormulaOverwrite))) {
                                        n.parameters._computed_value = ov
                                    } else {
                                        n.parameters.value = ov
                                    }
                            }
                        }
                    }
                    mergeRecentUserEdits(resolved)
                } catch(_) {}
                // Compute all changed fields and update DOM for them (cascade refresh)
                try {
                    const allChangedFields = this.findChangedFieldsBetweenDocuments(this.currentDocument, resolved)
                    if (Array.isArray(allChangedFields) && allChangedFields.length > 0) {
                        this.renderer.updateDocumentForCascadeFields(this.currentDocument, resolved, [], allChangedFields)
                    }
                } catch (e) {
                    console.warn('Cascade-only update failed; falling back to full re-render:', e)
                    try { this.renderer.renderDocument(resolved) } catch (e2) { console.error('Render error (cascade fallback):', e2); this.showError('Render error', e2) }
                }
                // Adopt the resolved document after DOM refresh
                this.currentDocument = resolved
                return { domOnly: false, success: true }
            }
        } catch (error) {
            // If selective update fails, fall back to full reevaluation
            console.warn('❌ Selective reevaluation failed, falling back to full update:', error)
            // Fallback: do full resolution without selective DOM updates
            try {
                const content = await invoke('serialize_overseer_nodes', { nodes: this.normalizeDocumentForSerialization(this.currentDocument) })
                const resolved = await invoke('parse_overseer_content', { content })
                this.currentDocument = resolved
                try { this.renderer.renderDocument(this.currentDocument) } catch (e) { console.error('Render error (fallback full update):', e); this.showError('Render error', e) }
                return { domOnly: false, success: true }
            } catch (fallbackError) {
                console.error('❌ Fallback update also failed:', fallbackError)
                return { domOnly: false, success: false }
            }
        }
    }

    markDocumentModified() {
        if (!this.isDocumentModified) {
            this.isDocumentModified = true
            this.updateTitle()
            
            // Enable save button if it was disabled
            document.getElementById('save-file-btn').disabled = false
        }
    }

    async reloadFile() {
        if (!this.currentFile) {
            this.setStatus('No file to reload', '', 'warning')
            return
        }

        // If there are unsaved changes, confirm with the user
        if (this.isDocumentModified) {
            const confirmDiscard = confirm('Discard unsaved changes and reload from disk?')
            if (!confirmDiscard) {
                return
            }
        }

    await this.loadFile(this.currentFile)
    // Update baseline from disk
    try { this._originalText = await invoke('load_overseer_file', { path: this.currentFile }) } catch(_) {}
    }

    updateTitle() {
        const filePathElement = document.getElementById('file-path')
        
        if (this.currentFile) {
            const fileName = this.currentFile.split('\\').pop() || this.currentFile.split('/').pop()
            const modifiedMarker = this.isDocumentModified ? ' *' : ''
            
            // Update window title
            document.title = `${fileName}${modifiedMarker} - Overseer`
            
            // Update file path display in toolbar
            filePathElement.textContent = `${this.currentFile}${modifiedMarker}`
            filePathElement.style.display = 'block'
            
            // Update status bar with current file info
            if (this.isDocumentModified) {
                this.setStatus(`Editing: ${fileName}`, 'Unsaved changes', 'warning')
            } else {
                this.setStatus(`Editing: ${fileName}`, 'All changes saved', 'success')
            }
        } else {
            document.title = 'Overseer'
            filePathElement.textContent = ''
            filePathElement.style.display = 'none'
            this.setStatus('Ready', 'No file open')
        }
    }

    showWelcomeScreen() {
        this.showScreen('welcome-screen')
        this.setStatus('Ready', 'Open or create a new Overseer file to get started')
    }

    showEditorScreen() {
        this.showScreen('editor-screen')
    }

    showError(title, error) {
        console.error(title, error)
        const msg = error && error.message ? error.message : (typeof error === 'string' ? error : JSON.stringify(error))
        document.getElementById('error-message').textContent = `${title}: ${msg}`
        this.showScreen('error-screen')
        this.setStatus('Error occurred', msg, 'error')
    }

    showScreen(screenId) {
        // Hide all screens
        document.querySelectorAll('.screen').forEach(screen => {
            screen.classList.remove('active')
        })
        
        // Show the target screen
        document.getElementById(screenId).classList.add('active')
    }

    startScheduler() {
        this.stopScheduler()
        if (!this.currentDocument) return

        const docsEqual = (a, b) => {
            try { return JSON.stringify(a) === JSON.stringify(b) } catch (_) { return false }
        }

        // Scan currentDocument for active timers and compute the next due time
        const extractAtMs = (params) => {
            if (!params) return null
            const comp = params._computed_at
            const raw = params.at
            const toMs = (s) => {
                if (!s || typeof s !== 'string') return null
                const ms = Date.parse(s)
                return isNaN(ms) ? null : ms
            }
            // Prefer computed
            if (comp !== undefined && comp !== null) {
                if (typeof comp === 'string') return toMs(comp)
                if (typeof comp === 'object') {
                    if (comp.Timestamp) return toMs(comp.Timestamp)
                    if (comp.String) return toMs(comp.String)
                    if (comp.Date) return toMs(`${comp.Date}T00:00:00Z`)
                }
            }
            // Fallback to raw param
            if (raw !== undefined && raw !== null) {
                if (typeof raw === 'string') return toMs(raw)
                if (typeof raw === 'object') {
                    if (raw.Timestamp) return toMs(raw.Timestamp)
                    if (raw.String) return toMs(raw.String)
                    if (raw.Date) return toMs(`${raw.Date}T00:00:00Z`)
                }
            }
            return null
        }

    const findNextDue = (doc) => {
            let nextTs = null
            const walk = (nodes, ctxPath=[]) => {
                for (const n of nodes || []) {
                    const t = (n.node_type || n.type || '').toLowerCase()
                    if (t === 'timer') {
                        const params = n.parameters || {}
                        const active = (params.active && (params.active.Boolean === true || params.active === true)) || false
                        if (!active) { /* skip */ }
                        else {
                            const ms = extractAtMs(params)
                            if (ms !== null) {
                                if (nextTs === null || ms < nextTs) nextTs = ms
                            }
                        }
                    }
                    if (Array.isArray(n.children) && n.children.length) walk(n.children, ctxPath.concat(n.name||n.node_type||n.type||''))
                }
            }
            walk(doc)
            return nextTs
        }

    const scheduleNext = async () => {
            if (!this.currentDocument) return
        // Prefer backend calculation to stay consistent with formula evaluation.
        // While actively editing, avoid spamming the backend — reuse a cached value when available.
        let nextMs = null
        try { nextMs = await invoke('get_next_timer_due_ms', { nodes: this.currentDocument }) } catch(_) {}
        if (nextMs == null) nextMs = findNextDue(this.currentDocument)
                // With dependency tracking and live UI timers, periodic full refreshes are no longer needed.
                // If there are no timers due, stay idle (no background refresh to avoid flicker/scroll resets).
                if (!nextMs) { if (DEBUG_MODE) console.log('[SCHED] no timers; idle (no periodic refresh)'); return }
        const now = Date.now()
        if (DEBUG_MODE) console.log('[SCHED] next due ms from backend:', nextMs)
        let delay = nextMs - now
            // Only schedule for future; if due/past, process almost immediately (debounced)
            if (delay < 0) delay = 0
            // Add small debounce to let system settle
            const baseDebounce = 700
            delay += baseDebounce
        // Enforce a minimal gap between ticks to avoid storms
    const minGap = 500
        if (this._scheduler && this._scheduler.lastTickAt) {
            const sinceLast = now - this._scheduler.lastTickAt
            if (sinceLast < minGap) delay += (minGap - sinceLast)
        }
    if (DEBUG_MODE) console.log('[SCHED] scheduling tick in', delay, 'ms')
    this._scheduler.id = setTimeout(async () => {
                try {
            if (!this.currentDocument) return
                    // Skip if a previous tick is still running
                    if (this._scheduler && this._scheduler.inFlight) { if (DEBUG_MODE) console.log('[SCHED] tick skipped (in flight)'); return }
                    if (this._scheduler) this._scheduler.inFlight = true
                    if (DEBUG_MODE) console.log('[SCHED] tick invoking backend')
                    const updated = await invoke('scheduler_tick', { nodes: this.currentDocument })
                    // Accept only shapes that look like a document
                    const looksLikeDocArray = Array.isArray(updated) && updated.every(n => n && typeof n === 'object')
                    const looksLikeDocObject = updated && typeof updated === 'object' && Array.isArray(updated.children)
                    if (looksLikeDocArray || looksLikeDocObject) {
                        const nextDoc = looksLikeDocArray ? updated : updated.children
                        // Only re-render if there are actual changes to visible document
                        if (!docsEqual(this.currentDocument, nextDoc)) {
                            this.currentDocument = nextDoc
                            try {
                                this.renderer.renderDocument(this.currentDocument)
                            } catch (e) {
                                console.error('Render error during scheduler tick:', e)
                                this.showError('Render error', e)
                                return
                            }
                        }
                        // Invalidate cached next due after a state change
                        this._scheduler.cachedNextMs = null
                    } else if (updated != null) {
                        if (DEBUG_MODE) console.warn('[SCHED] scheduler_tick returned non-document value; ignoring:', updated)
                    }
                } catch (e) {
                    const msg = e?.message || (typeof e === 'string' ? e : JSON.stringify(e))
                    console.warn('scheduler tick error:', e)
                    try { this.setStatus('Scheduler error', msg, 'error') } catch {}
                } finally {
                    if (this._scheduler) {
                        this._scheduler.inFlight = false
                        this._scheduler.lastTickAt = Date.now()
                    }
                    // Schedule again for the next due timer if any
            scheduleNext()
                }
            }, delay)
        }

        // Kick off scheduling
    scheduleNext()

        // Re-schedule lifecycle with window focus/blur
        window.addEventListener('blur', () => this.stopScheduler(), { once: true })
        window.addEventListener('focus', () => this.startScheduler(), { once: true })
    }

    stopScheduler() {
        if (this._scheduler && this._scheduler.id) {
            clearTimeout(this._scheduler.id)
            this._scheduler.id = null
        }
        if (this._scheduler) {
            this._scheduler.cachedNextMs = null
        }
    }

    setStatus(message, info = '', type = 'info') {
        const statusMessage = document.getElementById('status-message')
        const statusInfo = document.getElementById('status-info')
        
        statusMessage.textContent = message
        statusInfo.textContent = info
        
        // Clear previous status classes
        statusMessage.classList.remove('error', 'success', 'warning')
        
        // Add appropriate status class
        if (type === 'error') {
            statusMessage.classList.add('error')
        } else if (type === 'success') {
            statusMessage.classList.add('success')
        } else if (type === 'warning') {
            statusMessage.classList.add('warning')
        }
    }
}

// Initialize the app when the DOM is loaded
document.addEventListener('DOMContentLoaded', () => {
    const app = new OverseerApp()

    // Dev-only convenience: auto-enable debug UI and auto-open a file if configured
    try {
        const isDev = (typeof window !== 'undefined') && (import.meta && import.meta.env && import.meta.env.DEV)
    // Do not force-enable debug in dev; user can opt-in via ?debug=1

        // Determine autofile from query, localStorage, or dev default
        const qs = (() => {
            try { return new URLSearchParams(window.location.search) } catch { return new URLSearchParams('') }
        })()
        const autoFile = qs.get('autofile')
        // Only honor explicit query param; ignore any stale localStorage-based auto-loads
        // Guard against relative paths which may not resolve reliably across environments
        const looksAbsolute = (p) => typeof p === 'string' && (/^[a-zA-Z]:\\/.test(p) || /^\\\\/.test(p) || /^\//.test(p))
        if (autoFile && looksAbsolute(autoFile)) {
            app.loadFile(autoFile)
        }
    } catch {}
    
    // Global error surfacing to avoid silent crashes
    try {
        window.addEventListener('error', (e) => {
            const msg = e?.error?.message || e?.message || 'Unknown error'
            console.error('Global error:', e?.error || e)
            try { app.setStatus('Runtime error', msg, 'error') } catch {}
        })
        window.addEventListener('unhandledrejection', (e) => {
            const reason = e?.reason
            const msg = (reason && (reason.message || (typeof reason === 'string' ? reason : JSON.stringify(reason)))) || 'Unhandled promise rejection'
            console.error('Unhandled promise rejection:', reason)
            try { app.setStatus('Unhandled error', msg, 'error') } catch {}
        })
    } catch {}
})
