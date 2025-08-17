import { invoke } from '@tauri-apps/api/tauri'

// Set this to false to disable all debug UI/status output
const DEBUG_MODE = false;
import { open, save } from '@tauri-apps/api/dialog'
import { appWindow } from '@tauri-apps/api/window'
import { OverseerRenderer } from './renderer.js'
import { FileManager } from './file-manager.js'

class OverseerApp {
    constructor() {
        this.currentFile = null
        this.currentDocument = null
        this.isDocumentModified = false
        this.fileManager = new FileManager()
    this.renderer = new OverseerRenderer()
    this._scheduler = { id: null, periodMs: 1000, cachedNextMs: null }
    // Keep the raw original text for comment/whitespace merge on save
    this._originalText = null
        
        // Make app instance available globally for renderer
        window.app = this
        
        this.initializeEventListeners()
        this.showWelcomeScreen()
    }

    // Normalize in-memory document before sending to Rust: convert certain raw booleans
    // in parameters to OverseerValue-shaped objects expected by Serde, e.g., { Boolean: true }.
    // We only touch known internal flags we might have set from the UI: _override_present, _explicit_child_override.
    normalizeDocumentForSerialization(doc) {
        const visit = (node) => {
            if (!node || typeof node !== 'object') return
            const p = node.parameters
            if (p && typeof p === 'object') {
                const fix = (k) => {
                    if (p[k] === true) p[k] = { Boolean: true }
                    if (p[k] === false) p[k] = { Boolean: false }
                }
                fix('_override_present')
                fix('_explicit_child_override')
            }
            if (Array.isArray(node.children)) node.children.forEach(visit)
        }
        if (Array.isArray(doc)) doc.forEach(visit)
        else visit(doc)
        return doc
    }

    initializeEventListeners() {
        // File operations
        document.getElementById('open-file-btn').addEventListener('click', () => this.openFile())
        document.getElementById('new-file-btn').addEventListener('click', () => this.newFile())
        document.getElementById('save-file-btn').addEventListener('click', () => this.saveFile())
    document.getElementById('reload-file-btn').addEventListener('click', () => this.reloadFile())
        
        // Welcome screen
        document.getElementById('welcome-open-btn').addEventListener('click', () => this.openFile())
        document.getElementById('welcome-new-btn').addEventListener('click', () => this.newFile())
        
        // Error screen
        document.getElementById('error-back-btn').addEventListener('click', () => this.showWelcomeScreen())

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
                            console.log(`DEBUG: Found chart node ${node.name} at depth ${depth}`)
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
            this.currentDocument = overseerDocument
            this._originalText = content
            this.isDocumentModified = false

            // Update UI - remove direct file path update since updateTitle handles it now
            document.getElementById('save-file-btn').disabled = false
            document.getElementById('reload-file-btn').disabled = false
            this.updateTitle()

            // Add visible debug info before rendering
            const contentDisplay = document.getElementById('content-display')
            if (DEBUG_MODE) {
                contentDisplay.innerHTML = `<div style="background: yellow; padding: 10px; margin: 10px;">
                    <h3>DEBUG: About to render document</h3>
                    <p>Document type: ${typeof overseerDocument}</p>
                    <p>Is array: ${Array.isArray(overseerDocument)}</p>
                    <p>Length: ${overseerDocument?.length || 'N/A'}</p>
                    <p>Content preview: ${JSON.stringify(overseerDocument).substring(0, 200)}...</p>
                </div>`
            } else {
                contentDisplay.innerHTML = '';
            }

            if (DEBUG_MODE) this.setStatus('DEBUG: About to call renderer...')

            // Render the document
            this.renderer.renderDocument(overseerDocument)

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
        if (!this.currentFile || !this.currentDocument) {
            this.setStatus('No file to save', '', 'warning')
            return
        }

        try {
            this.setStatus('Saving file...', this.currentFile, 'info')
            
            // Serialize the current document state to Overseer DSL format
            const content = await invoke('serialize_overseer_nodes', { 
                nodes: this.normalizeDocumentForSerialization(this.currentDocument) 
            })

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
        console.log('🔧 Selectively updating document fields:', changedFieldPaths)
        
        for (const fieldPath of changedFieldPaths) {
            try {
                // Find the field in both documents and update the current one
                const newValue = this.getFieldValueByPath(resolvedDocument, fieldPath)
                if (newValue !== undefined) {
                    this.setFieldValueByPath(currentDocument, fieldPath, newValue)
                    console.log('🔧 Updated field:', fieldPath, 'to:', newValue)
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

    async reevaluateDocumentSelective(changedFieldPaths = [], fieldChanges = []) {
        try {
            if (!this.currentDocument) return { domOnly: false }
            
            console.log('🔄 Selective update triggered for fields:', changedFieldPaths)
            if (fieldChanges.length > 0) {
                console.log('📝 Field changes:', fieldChanges)
            }

            // Check if we can handle this as a pure DOM-only update (no backend needed)
            const canHandleDOMOnly = this.canHandleAsDOMOnlyUpdate(fieldChanges)
            
            if (canHandleDOMOnly) {
                console.log('🚀 Handling as DOM-only update (no backend call needed)')
                // Just do the DOM update directly without any backend processing
                if (changedFieldPaths.length > 0) {
                    console.log('🎯 Attempting DOM-only update for specific fields')
                    
                    // Use the current document as both old and new for DOM updates
                    const selectiveUpdateSuccessful = this.renderer.updateSelectiveFields(
                        this.currentDocument, this.currentDocument, changedFieldPaths, fieldChanges
                    )
                    
                    if (selectiveUpdateSuccessful) {
                        console.log('✅ DOM-only update completed successfully (charts completely untouched)')
                        return { domOnly: true, success: true }
                    } else {
                        console.log('⚠️ DOM-only update failed, falling back to backend processing')
                    }
                }
            }
            
            // Store the old document state before backend processing
            const oldDocument = JSON.parse(JSON.stringify(this.currentDocument))
            
            // Serialize current nodes to DSL
            const content = await invoke('serialize_overseer_nodes', { nodes: this.normalizeDocumentForSerialization(this.currentDocument) })
            console.log('📤 Serialized content being sent to backend:', content.substring(0, 500))
            // Parse + resolve + evaluate on backend with selective updates
            const resolved = await invoke('parse_overseer_content_selective', { content, changedFields: changedFieldPaths })
            
            // Instead of full re-render, do selective DOM updates if we have specific changed fields
            if (changedFieldPaths.length > 0) {
                console.log('🎯 Attempting selective DOM update for specific fields')
                
                // Try selective rendering for the changed fields using field change info
                let selectiveUpdateSuccessful = false
                try {
                    selectiveUpdateSuccessful = this.renderer.updateSelectiveFields(oldDocument, resolved, changedFieldPaths, fieldChanges)
                } catch (e) {
                    console.warn('Selective DOM update failed:', e)
                }
                
                if (selectiveUpdateSuccessful) {
                    // For now, assume no cascading changes since we can't easily get cascade count from backend
                    // This will prevent document updates when only simple field changes occur
                    console.log('✅ No cascading changes detected - document object unchanged (prevents chart refresh)')
                    
                    console.log('✅ Selective update completed successfully')
                    return { domOnly: false, success: true }
                } else {
                    console.log('🔄 Falling back to full re-render')
                    this.currentDocument = resolved
                    this.renderer.renderDocument(this.currentDocument)
                    return { domOnly: false, success: true }
                }
            } else {
                // No specific fields, do full re-render (for periodic updates)
                this.currentDocument = resolved
                this.renderer.renderDocument(this.currentDocument)
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
                this.renderer.renderDocument(this.currentDocument)
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
        document.getElementById('error-message').textContent = `${title}: ${error}`
        this.showScreen('error-screen')
        this.setStatus('Error occurred', error.message, 'error')
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
                // With dependency tracking, periodic full refreshes are no longer needed
                // Selective updates will handle formula dependencies when fields actually change
                // Keep minimal timer support for any remaining time-based formulas like days_since()
                const periodicRefreshMs = 300000 // Reduced to 5 minutes
                if (!nextMs) {
                    this._scheduler.id = setTimeout(async () => {
                        try {
                            if (!this.currentDocument) return
                            // Skip reevaluation if document has been modified to preserve user changes
                            if (!this.isDocumentModified) {
                                // Use selective update with empty change list for minimal time-based formula refresh
                                await this.reevaluateDocumentSelective([])
                            }
                        } finally {
                            scheduleNext()
                        }
                    }, periodicRefreshMs)
                    return
                }
        const now = Date.now()
        let delay = nextMs - now
            // Only schedule for future; if due/past, process almost immediately (debounced)
            if (delay < 0) delay = 0
            // Add small debounce to let system settle
            const baseDebounce = 500
            delay += baseDebounce
    this._scheduler.id = setTimeout(async () => {
                try {
            if (!this.currentDocument) return
                    const updated = await invoke('scheduler_tick', { nodes: this.currentDocument })
                    if (updated && !docsEqual(updated, this.currentDocument)) {
                        this.currentDocument = updated
                        this.renderer.renderDocument(updated)
                        // Invalidate cached next due after a state change
                        this._scheduler.cachedNextMs = null
                    }
                } catch (e) {
                    if (DEBUG_MODE) console.warn('scheduler tick error:', e)
                } finally {
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
    new OverseerApp()
})
