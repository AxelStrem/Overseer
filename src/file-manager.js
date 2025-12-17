export class FileManager {
    constructor() {
        this.recentFiles = this.loadRecentFiles()
    }

    loadRecentFiles() {
        try {
            const stored = localStorage.getItem('overseer-recent-files')
            return stored ? JSON.parse(stored) : []
        } catch (error) {
            console.warn('Failed to load recent files:', error)
            return []
        }
    }

    saveRecentFiles() {
        try {
            localStorage.setItem('overseer-recent-files', JSON.stringify(this.recentFiles))
        } catch (error) {
            console.warn('Failed to save recent files:', error)
        }
    }

    addRecentFile(filePath) {
        // Remove if already exists
        this.recentFiles = this.recentFiles.filter(file => file.path !== filePath)
        
        // Add to beginning
        this.recentFiles.unshift({
            path: filePath,
            name: this.getFileName(filePath),
            lastOpened: new Date().toISOString()
        })
        
        // Keep only last 10 files
        this.recentFiles = this.recentFiles.slice(0, 10)
        
        this.saveRecentFiles()
    }

    getRecentFiles() {
        return this.recentFiles
    }

    clearRecentFiles() {
        this.recentFiles = []
        this.saveRecentFiles()
    }

    getFileName(filePath) {
        return filePath.split(/[\\/]/).pop() || filePath
    }

    validateFilePath(filePath) {
        return filePath && typeof filePath === 'string' && filePath.endsWith('.os')
    }
}
