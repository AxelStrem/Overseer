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

        console.log('[DEBUG] createNodeElement:', { nodeType, node });
        if (node.children && Array.isArray(node.children)) {
            console.log(`[DEBUG] Node ${nodeType} has ${node.children.length} children:`, node.children.map(c => ({ name: c.name, type: c.node_type, parameters: c.parameters })));
        }

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
        if (node.parameters && (node.parameters.hidden === true || node.parameters.hidden === 'true' || 
            (node.parameters.hidden && node.parameters.hidden.Boolean === true))) {
            div.style.display = 'none'
        }
        
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

    createListItemElement(node) {
        const listItem = document.createElement('div')
        listItem.className = 'overseer-list-item'

        console.log('[DEBUG] createListItemElement:', { nodeType: node.node_type, node });

        // Check if this is a simple value list item (has a value parameter but no children)
        const hasValue = node.parameters && node.parameters["value"] !== undefined
        const hasChildren = node.children && Array.isArray(node.children) && node.children.length > 0
        
        if (hasValue && !hasChildren) {
            // This is a simple value list item like - "some string"
            console.log('[DEBUG] List item is simple value node:', node);
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
                console.log('[DEBUG] List item is typed value node, extracting value:', node);
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
                // This is a complex list item with children
                if (hasChildren) {
                    console.log(`[DEBUG] List item is complex node with ${node.children.length} children:`, node.children.map(c => ({ name: c.name, type: c.node_type, parameters: c.parameters })));
                    for (const child of node.children) {
                        const childElement = this.createNodeElement(child)
                        if (childElement) {
                            listItem.appendChild(childElement)
                        }
                    }
                } else {
                    console.log('[DEBUG] List item has no value or children:', node);
                }
            }
        }

        this.applyNodeStyles(listItem, node)
        return listItem
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
        
        // Only show a label if a user-friendly label is provided (e.g., via a 'label' parameter)
        const labelText = this.getParameterValue(node, 'label');
        if (labelText) {
            const label = document.createElement('label')
            label.textContent = labelText
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

        // Handle checkbox changes
        checkbox.addEventListener('change', () => {
            if (DEBUG_MODE) console.log('Checkbox changed:', node.name, checkbox.checked)
            this.updateNodeValue(node, checkbox.checked)
            
            // Mark document as modified
            if (window.app && window.app.markDocumentModified) {
                window.app.markDocumentModified()
            }
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
        
        // Apply spacing (for container elements)
        const spacing = this.getParameterValue(node, 'spacing')
        if (spacing !== null) {
            const spacingValue = parseInt(spacing) || 8 // Default to 8px
            element.style.gap = `${spacingValue}px`
        } else {
            // Apply default spacing
            element.style.gap = '8px'
        }
        
        // Apply margins (for all elements)
        this.applyMarginStyles(element, node)
    }

    applyMarginStyles(element, node) {
        if (!node.parameters) return
        
        const params = node.parameters
        
        // Handle shorthand margin parameter
        const margin = this.getParameterValue(node, 'margin')
        if (margin !== null) {
            const marginValue = parseInt(margin) || 0
            element.style.margin = `${marginValue}px`
        }
        
        // Handle individual margin parameters (these override shorthand)
        const marginTop = this.getParameterValue(node, 'margin-top')
        const marginBottom = this.getParameterValue(node, 'margin-bottom')
        const marginLeft = this.getParameterValue(node, 'margin-left')
        const marginRight = this.getParameterValue(node, 'margin-right')
        
        if (marginTop !== null) {
            element.style.marginTop = `${parseInt(marginTop) || 0}px`
        }
        if (marginBottom !== null) {
            element.style.marginBottom = `${parseInt(marginBottom) || 0}px`
        }
        if (marginLeft !== null) {
            element.style.marginLeft = `${parseInt(marginLeft) || 0}px`
        }
        if (marginRight !== null) {
            element.style.marginRight = `${parseInt(marginRight) || 0}px`
        }
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
        
        // Apply margins to all elements
        this.applyMarginStyles(element, node)
        
        // Add more style mappings as needed
    }

    getNodeValue(node) {
        // If the node itself is a string, return it
        if (typeof node === 'string') {
            console.log('[DEBUG] getNodeValue: node is a string:', node);
            return node
        }

        // Try parameters["value"] first
        if (node.parameters && node.parameters["value"] !== undefined) {
            const value = node.parameters["value"]
            console.log('[DEBUG] getNodeValue: found parameters["value"]:', value, 'in node:', node);
            if (typeof value === 'string') return value
            if (typeof value === 'object') {
                if (value.String !== undefined) return value.String
                if (value.Integer !== undefined) return value.Integer.toString()
                if (value.Float !== undefined) return value.Float.toString()
                if (value.Boolean !== undefined) return value.Boolean.toString()
                if (value.Date !== undefined) return value.Date
                if (value.Formula !== undefined) return value.Formula
            }
            return value.toString()
        }

        // Fallback: check node.value directly
        if (node.value !== undefined && node.value !== null) {
            console.log('[DEBUG] getNodeValue: found node.value:', node.value, 'in node:', node);
            if (typeof node.value === 'string') return node.value
            if (typeof node.value === 'object') {
                if (node.value.String !== undefined) return node.value.String
                if (node.value.Integer !== undefined) return node.value.Integer.toString()
                if (node.value.Float !== undefined) return node.value.Float.toString()
                if (node.value.Boolean !== undefined) return node.value.Boolean.toString()
                if (node.value.Date !== undefined) return node.value.Date
                if (node.value.Formula !== undefined) return node.value.Formula
            }
            return node.value.toString()
        }

        // Fallback: check node.String (for string nodes)
        if (node.String !== undefined && node.String !== null) {
            console.log('[DEBUG] getNodeValue: found node.String:', node.String, 'in node:', node);
            return node.String
        }

        // As a last resort, return the first string property that isn't a metadata field
        if (typeof node === 'object' && node !== null) {
            const skip = new Set(['name', 'type', 'node_type', 'parameters', 'children', 'label'])
            for (const key in node) {
                if (!skip.has(key) && typeof node[key] === 'string') {
                    console.log(`[DEBUG] getNodeValue: found string property '${key}' in node:`, node);
                    return node[key]
                }
            }
        }

        console.log('[DEBUG] getNodeValue: no value found for node:', node);
        return null
    }

    // Helper function to extract parameter values from OverseerValue objects
    getParameterValue(node, parameterName) {
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
            if (paramValue.Integer !== undefined) return paramValue.Integer.toString()
            if (paramValue.Float !== undefined) return paramValue.Float.toString()
            if (paramValue.Boolean !== undefined) return paramValue.Boolean.toString()
            if (paramValue.Date !== undefined) return paramValue.Date
            if (paramValue.Formula !== undefined) return paramValue.Formula
        }
        
        // Fallback: convert to string
        return paramValue.toString()
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
            
            // Update the node value in the document structure
            this.updateNodeValue(node, newValue)
            console.log('Field updated:', node.name, newValue)
            
            // Mark document as modified
            if (window.app && window.app.markDocumentModified) {
                window.app.markDocumentModified()
            }
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

    // Helper function to update a node's value in the document structure
    updateNodeValue(node, newValue) {
        // Update the node's parameters.value with the appropriate OverseerValue type
        if (!node.parameters) {
            node.parameters = {}
        }
        
        // Handle boolean values (from checkboxes)
        if (typeof newValue === 'boolean') {
            node.parameters.value = { Boolean: newValue }
            return
        }
        
        // Try to preserve the original type for other values, or default to String
        const currentValue = node.parameters.value
        if (currentValue && typeof currentValue === 'object') {
            if (currentValue.Integer !== undefined) {
                const numValue = parseInt(newValue)
                if (!isNaN(numValue)) {
                    node.parameters.value = { Integer: numValue }
                    return
                }
            }
            if (currentValue.Float !== undefined) {
                const floatValue = parseFloat(newValue)
                if (!isNaN(floatValue)) {
                    node.parameters.value = { Float: floatValue }
                    return
                }
            }
            if (currentValue.Boolean !== undefined) {
                if (newValue === 'true' || newValue === 'false') {
                    node.parameters.value = { Boolean: newValue === 'true' }
                    return
                }
            }
        }
        
        // Default to String type
        node.parameters.value = { String: newValue }
    }

    renderMarkdown(text) {
        // Basic markdown rendering (replace with proper library later)
        return text
            .replace(/\*\*(.*?)\*\*/g, '<strong>$1</strong>')
            .replace(/\*(.*?)\*/g, '<em>$1</em>')
            .replace(/\n/g, '<br>')
    }
}
