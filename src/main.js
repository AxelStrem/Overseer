import { invoke } from '@tauri-apps/api/tauri'
import { open, save } from '@tauri-apps/api/dialog'
import { appWindow } from '@tauri-apps/api/window'
import { OverseerRenderer } from './renderer.js'
import { FileManager } from './file-manager.js'

class OverseerApp {
    constructor() {
        this.currentFile = null
        this.currentDocument = null
        this.fileManager = new FileManager()
        this.renderer = new OverseerRenderer()
        
        this.initializeEventListeners()
        this.showWelcomeScreen()
    }

    initializeEventListeners() {
        // File operations
        document.getElementById('open-file-btn').addEventListener('click', () => this.openFile())
        document.getElementById('new-file-btn').addEventListener('click', () => this.newFile())
        document.getElementById('save-file-btn').addEventListener('click', () => this.saveFile())
        
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
            this.setStatus('Loading file...')
            
            // Add visible debugging to the status bar
            this.setStatus('DEBUG: Starting file load...')
            
            // Read file content
            const content = await invoke('load_overseer_file', { path: filePath })
            this.setStatus(`DEBUG: File content loaded, length: ${content?.length || 'unknown'}`)
            
            // Parse the content
            const overseerDocument = await invoke('parse_overseer_content', { content })
            this.setStatus(`DEBUG: Document parsed, type: ${typeof overseerDocument}, length: ${overseerDocument?.length || 'unknown'}`)
            
            this.currentFile = filePath
            this.currentDocument = overseerDocument
            
            // Update UI
            document.getElementById('file-path').textContent = filePath
            document.getElementById('save-file-btn').disabled = false
            
            // Add visible debug info before rendering
            const contentDisplay = document.getElementById('content-display')
            contentDisplay.innerHTML = `<div style="background: yellow; padding: 10px; margin: 10px;">
                <h3>DEBUG: About to render document</h3>
                <p>Document type: ${typeof overseerDocument}</p>
                <p>Is array: ${Array.isArray(overseerDocument)}</p>
                <p>Length: ${overseerDocument?.length || 'N/A'}</p>
                <p>Content preview: ${JSON.stringify(overseerDocument).substring(0, 200)}...</p>
            </div>`
            
            this.setStatus('DEBUG: About to call renderer...')
            
            // Render the document
            this.renderer.renderDocument(overseerDocument)
            
            this.setStatus('DEBUG: Renderer called, switching to editor screen...')
            this.showEditorScreen()
            this.setStatus('File loaded successfully - DEBUG VERSION')
            
        } catch (error) {
            this.setStatus(`DEBUG: Error occurred - ${error.message}`)
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
        if (!this.currentFile) {
            return
        }

        try {
            this.setStatus('Saving file...')
            
            // For now, we'll save the original content
            // Later this will save the modified document
            const content = await invoke('load_overseer_file', { path: this.currentFile })
            await invoke('save_overseer_file', { 
                path: this.currentFile, 
                content 
            })
            
            this.setStatus('File saved successfully')
        } catch (error) {
            this.showError('Failed to save file', error)
        }
    }

    showWelcomeScreen() {
        this.showScreen('welcome-screen')
        this.setStatus('Ready')
    }

    showEditorScreen() {
        this.showScreen('editor-screen')
    }

    showError(title, error) {
        console.error(title, error)
        document.getElementById('error-message').textContent = `${title}: ${error}`
        this.showScreen('error-screen')
        this.setStatus('Error')
    }

    showScreen(screenId) {
        // Hide all screens
        document.querySelectorAll('.screen').forEach(screen => {
            screen.classList.remove('active')
        })
        
        // Show the target screen
        document.getElementById(screenId).classList.add('active')
    }

    setStatus(message, info = '') {
        document.getElementById('status-message').textContent = message
        document.getElementById('status-info').textContent = info
    }
}

// Initialize the app when the DOM is loaded
document.addEventListener('DOMContentLoaded', () => {
    new OverseerApp()
})
