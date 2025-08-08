// Set this to false to disable all debug info in the rendered UI
const DEBUG_MODE = false;

// Import marked for markdown rendering
import { marked } from 'marked';

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
        
        // Check if markdown is enabled for this text field
        const isMarkdownEnabled = this.getParameterValue(node, 'markdown') === true
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
            if (isMarkdownEnabled) {
                this.makeMarkdownFieldEditable(value, node)
            } else {
                this.makeFieldEditable(value, node, true)
            }
        })
        
        container.appendChild(value)
        
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
        value.textContent = this.getNodeValue(node) || '0'
        
        // Make it editable on double-click
        value.addEventListener('dblclick', () => {
            this.makeFieldEditable(value, node)
        })
        
        container.appendChild(value)
        
        
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
        
        // Apply default field styling if no explicit parameters are set
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
        
        // Apply default field styling if no explicit parameters are set
        this.applyFieldDefaultStyles(container, node)
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

        // Apply default field styling if no explicit parameters are set
        this.applyFieldDefaultStyles(container, node)
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
        
          // Check if user wants zero spacing/margin layout (tight grid)
        const spacing = this.getParameterValue(node, 'spacing')
        const margin = this.getParameterValue(node, 'margin')
        const isTightLayout = (spacing === 0 || margin === 0)
        
        // Apply default padding unless explicitly overridden or in tight layout mode
        
        const hasExplicitPadding = node.parameters.padding !== undefined || 
                                 node.parameters['padding-top'] !== undefined ||
                                 node.parameters['padding-bottom'] !== undefined ||
                                 node.parameters['padding-left'] !== undefined ||
                                 node.parameters['padding-right'] !== undefined
        
        if (!hasExplicitPadding) {
            if (isTightLayout) {
                // In tight layout mode, use minimal or no padding
                element.style.padding = '0px'
            } else {
                // Apply default padding for containers (div/list) unless explicitly set to 0
                if (node.node_type === 'div' || node.node_type === 'list') {
                    element.style.padding = '16px'
                }
            }
        } else {
            // Apply explicit padding parameters
            this.applyPaddingStyles(element, node)
        }
        
        // Apply default margin unless explicitly set
        const hasExplicitMargin = node.parameters.margin !== undefined ||
                                node.parameters['margin-top'] !== undefined ||
                                node.parameters['margin-bottom'] !== undefined ||
                                node.parameters['margin-left'] !== undefined ||
                                node.parameters['margin-right'] !== undefined
        
        if (!hasExplicitMargin && (node.node_type === 'div' || node.node_type === 'list')) {
            element.style.marginBottom = '16px'
        }

        if (spacing !== null) {
            const spacingValue = parseInt(spacing)
            element.style.gap = `${spacingValue}px`
        } else {
            // Apply default spacing only if no explicit spacing parameter
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

    
    applyPaddingStyles(element, node) {
        if (!node.parameters) return
        
        // Handle shorthand padding parameter
        const padding = this.getParameterValue(node, 'padding')
        if (padding !== null) {
            const paddingValue = parseInt(padding) || 0
            element.style.padding = `${paddingValue}px`
        }
        
        // Handle individual padding parameters (these override shorthand)
        const paddingTop = this.getParameterValue(node, 'padding-top')
        const paddingBottom = this.getParameterValue(node, 'padding-bottom')
        const paddingLeft = this.getParameterValue(node, 'padding-left')
        const paddingRight = this.getParameterValue(node, 'padding-right')
        
        if (paddingTop !== null) {
            element.style.paddingTop = `${parseInt(paddingTop) || 0}px`
        }
        if (paddingBottom !== null) {
            element.style.paddingBottom = `${parseInt(paddingBottom) || 0}px`
        }
        if (paddingLeft !== null) {
            element.style.paddingLeft = `${parseInt(paddingLeft) || 0}px`
        }
        if (paddingRight !== null) {
            element.style.paddingRight = `${parseInt(paddingRight) || 0}px`
        }
    }

    
    applyFieldDefaultStyles(element, node) {
        if (!node.parameters) {
            // Apply default field styling when no parameters
            element.style.marginBottom = '12px'
            element.style.padding = '8px'
            return
        }

        
        // Check if user wants zero spacing/margin layout (tight grid)
        const spacing = this.getParameterValue(node, 'spacing')
        const margin = this.getParameterValue(node, 'margin')
        const isTightLayout = (spacing === 0 || margin === 0)
        
        // Check if any margin/padding parameters are explicitly set
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
        
        // Apply defaults only if not explicitly set
        if (!hasExplicitMargin) {
            if (isTightLayout) {
                element.style.margin = '0px'
            } else {
                element.style.marginBottom = '12px'
            }
        }
        if (!hasExplicitPadding) {
            if (isTightLayout) {
                element.style.padding = '2px' // Minimal padding for readability
            } else {
                element.style.padding = '8px'
            }
        }
    }

    applyNodeStyles(element, node) {
        if (!node.parameters) return
        
        // Apply basic styling parameters
        const params = node.parameters
        
        // Legacy background support (keep for compatibility)
        if (params.background) {
            element.style.backgroundColor = params.background
        }
        
        // New styling parameters
        if (params['background-color']) {
            element.style.backgroundColor = this.convertColorValue(params['background-color'])
        }
        
        if (params['font-color']) {
            element.style.color = this.convertColorValue(params['font-color'])
            
            // Also apply font-color to any field-value children to override CSS class specificity
            const fieldValueElements = element.querySelectorAll('.field-value')
            fieldValueElements.forEach(fieldValue => {
                fieldValue.style.setProperty('color', this.convertColorValue(params['font-color']), 'important')
            })
        }
        
        if (params['font-size']) {
            element.style.setProperty('font-size', this.convertCssSizeValue(params['font-size']), 'important')
            
            // Also apply font-size to any field-value children to override CSS class specificity
            const fieldValueElements = element.querySelectorAll('.field-value')
            fieldValueElements.forEach(fieldValue => {
                fieldValue.style.setProperty('font-size', this.convertCssSizeValue(params['font-size']), 'important')
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
        
        // Border style parameter
        if (params['border-style']) {
            const borderStyle = this.convertBorderStyleValue(params['border-style'])
            if (borderStyle) {
                element.style.border = borderStyle
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
        
        // Legacy border support (keep for compatibility)
        if (params.border) {
            element.style.border = params.border
        }
        
        // Legacy horizontal-size support (keep for compatibility)
        if (params['horizontal-size']) {
            element.style.width = params['horizontal-size']
        }
        
        // Apply margins to all elements
        this.applyMarginStyles(element, node)
        
        // Add more style mappings as needed
    }

    convertColorValue(colorParam) {
        // Handle different color value types from the Rust backend
        if (typeof colorParam === 'string') {
            return colorParam // Legacy string colors
        }
        
        if (typeof colorParam === 'object' && colorParam !== null) {
            if (colorParam.Color) {
                const color = colorParam.Color
                if (color.Hex) return color.Hex
                if (color.Named) return color.Named
                if (color.Rgb) {
                    const [r, g, b] = color.Rgb
                    // Convert from 0.0-1.0 range to 0-255 range
                    const r255 = Math.round(r * 255)
                    const g255 = Math.round(g * 255)
                    const b255 = Math.round(b * 255)
                    return `rgb(${r255}, ${g255}, ${b255})`
                }
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
            if (sizeParam.CssSize) {
                const size = sizeParam.CssSize
                if (size.Pixels) return `${size.Pixels}px`
                if (size.Percentage) return `${size.Percentage}%`
                if (size.Em) return `${size.Em}em`
                if (size.Rem) return `${size.Rem}rem`
                if (size.ViewportWidth) return `${size.ViewportWidth}vw`
                if (size.ViewportHeight) return `${size.ViewportHeight}vh`
                if (size.Auto) return 'auto'
                if (size.FitContent) return 'fit-content'
            }
        }
        
        return sizeParam // Fallback
    }

    convertBorderStyleValue(borderParam) {
        // Handle different border style value types from the Rust backend
        if (typeof borderParam === 'string') {
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

    getNodeValue(node) {
        // If the node itself is a string, return it
        if (typeof node === 'string') {
            console.log('[DEBUG] getNodeValue: node is a string:', node);
            return node
        }

        // Prefer computed value if present
        if (node.parameters && node.parameters["_computed_value"] !== undefined) {
            const value = node.parameters["_computed_value"]
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

        // Try parameters["value"] next
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
                if (paramValue.Formula !== undefined) return paramValue.Formula
                if (paramValue.Color !== undefined) return paramValue.Color
                if (paramValue.CssSize !== undefined) return paramValue.CssSize
            }
            return paramValue.toString()
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
            if (paramValue.Formula !== undefined) return paramValue.Formula
            if (paramValue.Color !== undefined) return paramValue.Color
            if (paramValue.CssSize !== undefined) return paramValue.CssSize
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
        
        const finishEditing = (save = true) => {
            if (save) {
                const newValue = textarea.value
                // Update the element with rendered markdown
                element.innerHTML = this.renderMarkdown(newValue)
                
                // Update the node value in the document structure
                this.updateNodeValue(node, newValue)
                console.log('Markdown field updated:', node.name, newValue)
                
                // Mark document as modified
                if (window.app && window.app.markDocumentModified) {
                    window.app.markDocumentModified()
                }
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
}
