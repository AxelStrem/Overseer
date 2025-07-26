// Set this to false to disable all debug info in the rendered UI
const DEBUG_MODE = false;

export class OverseerRenderer {
    constructor() {
        this.contentDisplay = document.getElementById('content-display')
        this.tabContainer = document.getElementById('tab-container')
    }

    renderDocument(overseerDocument) {
        console.log('Rendering document:', overseerDocument)
        console.log('Document type:', typeof overseerDocument)
        console.log('Document is array:', Array.isArray(overseerDocument))
        console.log('Document length:', overseerDocument?.length)
        
        // Clear previous content
        this.contentDisplay.innerHTML = ''
        this.tabContainer.innerHTML = ''

        // Add visible debugging info
        if (DEBUG_MODE) {
            const debugInfo = document.createElement('div')
            debugInfo.style.cssText = 'background: #f0f0f0; padding: 10px; margin: 10px; border: 1px solid #ccc; font-family: monospace; white-space: pre-wrap; color: #000;'
            debugInfo.textContent = `DEBUG INFO:\nDocument type: ${typeof overseerDocument}\nIs array: ${Array.isArray(overseerDocument)}\nDocument length: ${overseerDocument?.length || 'N/A'}\nDocument content: ${JSON.stringify(overseerDocument, null, 2)}`
            this.contentDisplay.appendChild(debugInfo)
        }

        if (!overseerDocument) {
            console.error('Document is null or undefined')
            this.contentDisplay.innerHTML += '<p>No document provided</p>'
            return
        }

        if (Array.isArray(overseerDocument)) {
            console.log('Processing array document with', overseerDocument.length, 'nodes')
            
            if (overseerDocument.length === 0) {
                this.contentDisplay.innerHTML += '<p>Document is empty (no nodes parsed)</p>'
                return
            }
            
            // Document is an array of root nodes
            for (let i = 0; i < overseerDocument.length; i++) {
                console.log(`Rendering node ${i}:`, overseerDocument[i])
                this.renderNode(overseerDocument[i], this.contentDisplay)
            }
        } else if (overseerDocument && typeof overseerDocument === 'object') {
            console.log('Processing single root node:', overseerDocument)
            // Single root node
            this.renderNode(overseerDocument, this.contentDisplay)
        } else {
            console.warn('Unexpected document format:', overseerDocument)
            this.contentDisplay.innerHTML += '<p>Unexpected document format</p>'
        }
        
        console.log('Content display after rendering:', this.contentDisplay.innerHTML)
    }

    renderNode(node, container) {
        console.log('renderNode called with:', node, 'container:', container)
        
        if (!node || typeof node !== 'object') {
            console.warn('Invalid node:', node)
            return
        }

        const element = this.createNodeElement(node)
        console.log('Created element:', element)
        
        if (element) {
            container.appendChild(element)
            console.log('Appended element to container')
            
            // Render children
            if (node.children && Array.isArray(node.children)) {
                console.log('Rendering', node.children.length, 'children for node:', node)
                for (const child of node.children) {
                    this.renderNode(child, element)
                }
            } else {
                console.log('No children for node:', node)
            }
        } else {
            console.warn('Failed to create element for node:', node)
        }
    }

    createNodeElement(node) {
        // Handle both possible node structures
        let nodeType = node.node_type || node.type || node.name || 'div'

        console.log('Creating element for node type:', nodeType, 'from node:', node)
        
        switch (nodeType.toLowerCase()) {
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
            case 'bool':
                return this.createBooleanElement(node)
            case 'button':
                return this.createButtonElement(node)
            case 'checkbox':
                return this.createCheckboxElement(node)
            case 'chart':
                return this.createChartElement(node)
            default:
                console.warn('Unknown node type:', nodeType)
                return this.createDivElement(node)
        }
    }

    createTabElement(node) {
        const tabButton = document.createElement('button')
        tabButton.className = 'tab-button'
        tabButton.textContent = node.name || 'Tab'
        
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
        
        // Check if this is a hidden div (template)
        if (node.parameters && node.parameters.hidden === 'true') {
            div.style.display = 'none'
        }
        
        this.applyNodeStyles(div, node)
        return div
    }

    createListElement(node) {
        const list = document.createElement('div')
        list.className = 'overseer-list'
        
        // Don't show list name as header - we just want the list contents
        // Lists should be transparent containers for their items
        
        // Apply list layout
        const layout = node.parameters?.layout || 'vertical'
        list.classList.add(`layout-${layout}`)
        
        this.applyNodeStyles(list, node)
        return list
    }

    createListItemElement(node) {
        const listItem = document.createElement('div')
        listItem.className = 'overseer-list-item'
        
        const value = this.getNodeValue(node)
        if (value) {
            listItem.textContent = value
        }
        
        this.applyNodeStyles(listItem, node)
        return listItem
    }

    createStringElement(node) {
        const container = document.createElement('div')
        container.className = 'overseer-field string-field'
        
        if (node.name) {
            const label = document.createElement('label')
            label.textContent = node.name
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
        this.applyNodeStyles(container, node)
        return container
    }

    createTextElement(node) {
        const container = document.createElement('div')
        container.className = 'overseer-field text-field'
        
        if (node.name) {
            const label = document.createElement('label')
            label.textContent = node.name
            container.appendChild(label)
        }
        
        const value = document.createElement('div')
        value.className = 'field-value text-content'
        
        // Support markdown rendering (basic for now)
        const textContent = this.getNodeValue(node) || ''
        value.innerHTML = this.renderMarkdown(textContent)
        
        // Make it editable on double-click
        value.addEventListener('dblclick', () => {
            this.makeFieldEditable(value, node, true)
        })
        
        container.appendChild(value)
        this.applyNodeStyles(container, node)
        return container
    }

    createNumberElement(node) {
        const container = document.createElement('div')
        container.className = 'overseer-field number-field'
        
        if (node.name) {
            const label = document.createElement('label')
            label.textContent = node.name
            container.appendChild(label)
        }
        
        const value = document.createElement('span')
        value.className = 'field-value'
        value.textContent = this.getNodeValue(node) || '0'
        
        // Make it editable on double-click
        value.addEventListener('dblclick', () => {
            this.makeFieldEditable(value, node)
        })
        
        container.appendChild(value)
        this.applyNodeStyles(container, node)
        return container
    }

    createDateElement(node) {
        const container = document.createElement('div')
        container.className = 'overseer-field date-field'
        
        if (node.name) {
            const label = document.createElement('label')
            label.textContent = node.name
            container.appendChild(label)
        }
        
        const value = document.createElement('span')
        value.className = 'field-value'
        value.textContent = this.getNodeValue(node) || ''
        
        container.appendChild(value)
        this.applyNodeStyles(container, node)
        return container
    }

    createBooleanElement(node) {
        const container = document.createElement('div')
        container.className = 'overseer-field boolean-field'
        
        if (node.name) {
            const label = document.createElement('label')
            label.textContent = node.name
            container.appendChild(label)
        }
        
        const checkbox = document.createElement('input')
        checkbox.type = 'checkbox'
        checkbox.checked = this.getNodeValue(node) === 'true' || this.getNodeValue(node) === true
        
        container.appendChild(checkbox)
        this.applyNodeStyles(container, node)
        return container
    }

    createButtonElement(node) {
        const button = document.createElement('button')
        button.className = 'overseer-button'
        button.textContent = node.name || 'Button'
        
        // TODO: Add action handling
        button.addEventListener('click', () => {
            console.log('Button clicked:', node.name)
        })
        
        this.applyNodeStyles(button, node)
        return button
    }

    createCheckboxElement(node) {
        const container = document.createElement('div')
        container.className = 'overseer-field checkbox-field'
        
        const checkbox = document.createElement('input')
        checkbox.type = 'checkbox'
        checkbox.checked = this.getNodeValue(node) === 'true' || this.getNodeValue(node) === true

        if (node.name) {
            const label = document.createElement('label')
            label.appendChild(checkbox)
            label.appendChild(document.createTextNode(node.name))
            container.appendChild(label)
        } else {
            // Just the checkbox without any label for unnamed or internal checkboxes
            container.appendChild(checkbox)
        }
        
        // TODO: Add action handling
        checkbox.addEventListener('change', () => {
            console.log('Checkbox changed:', node.name, checkbox.checked)
        })
        
        this.applyNodeStyles(container, node)
        return container
    }

    createChartElement(node) {
        const container = document.createElement('div')
        container.className = 'overseer-chart'
        
        const canvas = document.createElement('canvas')
        container.appendChild(canvas)
        
        // TODO: Implement chart rendering with Chart.js
        canvas.width = 400
        canvas.height = 200
        
        const ctx = canvas.getContext('2d')
        ctx.fillStyle = '#f0f0f0'
        ctx.fillRect(0, 0, 400, 200)
        ctx.fillStyle = '#333'
        ctx.font = '16px Arial'
        ctx.fillText('Chart: ' + (node.name || 'Unnamed'), 10, 30)
        ctx.fillText('(Chart.js integration pending)', 10, 60)
        
        this.applyNodeStyles(container, node)
        return container
    }

    applyNodeStyles(element, node) {
        if (!node.parameters) return
        
        // Apply basic styling parameters
        const params = node.parameters
        
        if (params.background) {
            element.style.backgroundColor = params.background
        }
        
        if (params.border) {
            element.style.border = params.border
        }
        
        if (params['horizontal-size']) {
            element.style.width = params['horizontal-size']
        }
        
        // Add more style mappings as needed
    }

    getNodeValue(node) {
        // Values are stored in parameters["value"] according to the parser
        let value = null
        
        if (node.parameters && node.parameters["value"] !== undefined) {
            value = node.parameters["value"]
        }
        
        if (value === null || value === undefined) return null
        
        if (typeof value === 'string') {
            return value
        }
        
        if (typeof value === 'object') {
            // Handle different value types
            if (value.String !== undefined) return value.String
            if (value.Integer !== undefined) return value.Integer.toString()
            if (value.Float !== undefined) return value.Float.toString()
            if (value.Boolean !== undefined) return value.Boolean.toString()
            if (value.Date !== undefined) return value.Date
            if (value.Formula !== undefined) return value.Formula
        }
        
        return value.toString()
    }

    makeFieldEditable(element, node, isMultiline = false) {
        const currentValue = element.textContent
        
        const input = document.createElement(isMultiline ? 'textarea' : 'input')
        input.value = currentValue
        input.className = 'field-editor'
        
        if (isMultiline) {
            input.rows = 3
        }
        
        // Replace the element with the input
        element.style.display = 'none'
        element.parentNode.insertBefore(input, element.nextSibling)
        input.focus()
        input.select()
        
        const finishEditing = () => {
            const newValue = input.value
            element.textContent = newValue
            element.style.display = 'inline'
            input.remove()
            
            // TODO: Update the node value and save changes
            console.log('Field updated:', node.name, newValue)
        }
        
        input.addEventListener('blur', finishEditing)
        input.addEventListener('keydown', (e) => {
            if (e.key === 'Enter' && !isMultiline) {
                finishEditing()
            }
            if (e.key === 'Escape') {
                element.style.display = 'inline'
                input.remove()
            }
        })
    }

    renderMarkdown(text) {
        // Basic markdown rendering (replace with proper library later)
        return text
            .replace(/\*\*(.*?)\*\*/g, '<strong>$1</strong>')
            .replace(/\*(.*?)\*/g, '<em>$1</em>')
            .replace(/\n/g, '<br>')
    }
}
