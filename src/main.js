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
    this._scheduler = { id: null, periodMs: 1000 }
        
        // Make app instance available globally for renderer
        window.app = this
        
        this.initializeEventListeners()
        this.showWelcomeScreen()
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

            this.currentFile = filePath
            this.currentDocument = overseerDocument
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
                nodes: this.currentDocument 
            })
            
            // Save the serialized content
            await invoke('save_overseer_file', { 
                path: this.currentFile, 
                content 
            })
            
            // Mark document as saved
            this.isDocumentModified = false
            this.updateTitle() // This will update status to show "All changes saved"
            
        } catch (error) {
            this.setStatus('Failed to save file', error.message, 'error')
            this.showError('Failed to save file', error)
        }
    }

    async reevaluateDocument() {
        try {
            if (!this.currentDocument) return
            // Serialize current nodes to DSL
            const content = await invoke('serialize_overseer_nodes', { nodes: this.currentDocument })
            // Parse + resolve + evaluate on backend
            const resolved = await invoke('parse_overseer_content', { content })
            // Replace current document and re-render
            this.currentDocument = resolved
            this.renderer.renderDocument(this.currentDocument)
        } catch (error) {
            // Non-fatal: log and keep current view
            console.warn('Reevaluation failed:', error)
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
        // Prefer backend calculation to stay consistent with formula evaluation
        let nextMs = null
        try { nextMs = await invoke('get_next_timer_due_ms', { nodes: this.currentDocument }) } catch(_) {}
        if (nextMs == null) nextMs = findNextDue(this.currentDocument)
        if (!nextMs) return
        const now = Date.now()
        let delay = nextMs - now
            // Only schedule for future; if due/past, process almost immediately (debounced)
            if (delay < 0) delay = 0
            // Add small debounce to let system settle
            delay += 500
        this._scheduler.id = setTimeout(async () => {
                try {
                    if (!this.currentDocument) return
                    const updated = await invoke('scheduler_tick', { nodes: this.currentDocument })
                    if (updated && !docsEqual(updated, this.currentDocument)) {
                        this.currentDocument = updated
                        this.renderer.renderDocument(updated)
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
            clearInterval(this._scheduler.id)
            this._scheduler.id = null
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
