// Set this to false to disable all debug info in the rendered UI
const DEBUG_MODE = false;

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
                        console.log(`[BG] enter node=${dbgName} type=${node.node_type || node.type} parentBg=${inheritedStyles?.backgroundColor ?? 'null'}`)
                        console.log(`[BG] node=${dbgName} ownAny=${ownBgAny ? JSON.stringify(ownBgAny) : 'null'} computed=${hasComputedBg} rawFormula=${hasRawFormulaBg} ownEffective=${ownEffectiveBg ?? 'null'} effectiveBg=${effectiveBg ?? 'null'}`)
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
                                    console.log(`[BG] pass to child parent=${dbgParent} child=${dbgChild} inheritedBg=${nextInherited.backgroundColor ?? 'null'}`)
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
            intervalId = setInterval(update, 1000)
            const obs = new MutationObserver(() => {
                if (!document.body.contains(container)) { clearInterval(intervalId); obs.disconnect() }
            })
            obs.observe(document.body, { childList: true, subtree: true })
        }

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
        intervalId = setInterval(update, 1000)
        const obs = new MutationObserver(() => {
            if (!document.body.contains(container)) { clearInterval(intervalId); obs.disconnect() }
        })
        obs.observe(document.body, { childList: true, subtree: true })

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

    // Apply default padding unless explicitly overridden or in tight layout mode

        
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
                    // symmetric top/bottom margins for more balanced look
                    element.style.marginTop = '8px'
                    element.style.marginBottom = '8px'
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
            console.log('[DEBUG] getNodeValue: node is a string:', node);
            return node
        }

        // Prefer raw value if present and not a Formula; otherwise prefer computed
        if (node.parameters && node.parameters["value"] !== undefined) {
            const raw = node.parameters["value"]
            const isFormula = typeof raw === 'object' && raw !== null && raw.Formula !== undefined
            if (!isFormula) {
                if (typeof raw === 'string') {
                    const nt = (node.node_type || node.type || '').toLowerCase()
                    if (nt === 'timestamp') return this.formatTimestampValue(node, raw)
                    return raw
                }
                if (typeof raw === 'object' && raw !== null) {
                    if (raw.String !== undefined) return raw.String
                    if (raw.Integer !== undefined) return raw.Integer.toString()
                    if (raw.Float !== undefined) return raw.Float.toString()
                    if (raw.Boolean !== undefined) return raw.Boolean.toString()
                    if (raw.Date !== undefined) return raw.Date
                    if (raw.Timestamp !== undefined) return this.formatTimestampValue(node, raw.Timestamp)
                }
                return String(raw)
            }
            // If it's a Formula, fall through to computed if available
        }

        // Use computed value if present
        if (node.parameters && node.parameters["_computed_value"] !== undefined) {
            const value = node.parameters["_computed_value"]
            if (typeof value === 'string') return value
            if (typeof value === 'object') {
                if (value.String !== undefined) return value.String
                if (value.Integer !== undefined) return value.Integer.toString()
                if (value.Float !== undefined) return value.Float.toString()
                if (value.Boolean !== undefined) return value.Boolean.toString()
                if (value.Date !== undefined) return value.Date
                if (value.Timestamp !== undefined) return this.formatTimestampValue(node, value.Timestamp)
                if (value.Formula !== undefined) return value.Formula
            }
            return value.toString()
        }

        // Try parameters["value"] next
        if (node.parameters && node.parameters["value"] !== undefined) {
            const value = node.parameters["value"]
            console.log('[DEBUG] getNodeValue: found parameters["value"]:', value, 'in node:', node);
            if (typeof value === 'string') {
                // If this node is a timestamp-typed field, format string value as timestamp
                const nt = (node.node_type || node.type || '').toLowerCase()
                if (nt === 'timestamp') return this.formatTimestampValue(node, value)
                return value
            }
            if (typeof value === 'object') {
                if (value.String !== undefined) return value.String
                if (value.Integer !== undefined) return value.Integer.toString()
                if (value.Float !== undefined) return value.Float.toString()
                if (value.Boolean !== undefined) return value.Boolean.toString()
                if (value.Date !== undefined) return value.Date
                if (value.Timestamp !== undefined) return this.formatTimestampValue(node, value.Timestamp)
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
                if (node.value.Timestamp !== undefined) return this.formatTimestampValue(node, node.value.Timestamp)
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
        // Capture the old value before editing starts
        const oldValue = element.textContent
        
        // Prefer editing the raw formula if this field has one; otherwise use displayed text
        const originalParam = node?.parameters?.value
        const hasFormula = originalParam && typeof originalParam === 'object' && originalParam.Formula !== undefined
        const currentValue = element.textContent
        const initialEditorText = hasFormula ? `$(${originalParam.Formula})` : currentValue

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
            
            // Capture field path before any DOM manipulation
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
            
            // Update the node value in the document structure
            // Instead of using the local node reference, find and update the node in the main document
            if (fieldPath && window.app && window.app.currentDocument) {
                console.log('🔧 Updating node in main document at path:', fieldPath, 'with value:', newValue)
                const success = this.updateNodeValueByPath(window.app.currentDocument, fieldPath, newValue)
                if (!success) {
                    console.warn('⚠️ Failed to update node by path, falling back to local node update')
                    this.updateNodeValue(node, newValue)
                }
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
            console.log('Field updated:', node.name, newValue)
            
            // Mark document as modified
            if (window.app && window.app.markDocumentModified) {
                window.app.markDocumentModified()
            }
            // Trigger reevaluation so formulas and computed values refresh
            if (window.app && window.app.reevaluateDocumentSelective) {
                // Use pre-captured field path for selective update
                if (fieldPath) {
                    console.log('🔄 Triggering selective update for field:', fieldPath)
                    // Pass the old and new values to help selective update system
                    const updateResult = await window.app.reevaluateDocumentSelective([fieldPath], [{
                        path: fieldPath,
                        oldValue: oldValue,
                        newValue: newValue
                    }])
                    
                    // Skip event emission for any successful selective update (DOM-only or backend selective)
                    if (updateResult && updateResult.success) {
                        if (updateResult.domOnly) {
                            console.log('🎯 Skipping event emission for DOM-only update (prevents chart refresh)')
                        } else {
                            console.log('🎯 Skipping event emission for successful selective backend update (prevents chart refresh)')
                        }
                        return // Skip the event emission below
                    }
                } else {
                    console.warn('No field path available, falling back to full update')
                    await window.app.reevaluateDocumentSelective([])
                }
            }
            // Emit change event for actions (only if not DOM-only update)
            try { await this.emitEvent(node, element, 'change') } catch(_) {}
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
                console.log('Markdown field updated:', node.name, newValue)
                
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
    try { console.debug('[Overseer] emitEvent', eventName, 'path=', path) } catch(_) {}
        const updated = await invoke('execute_overseer_event', {
            nodes: window.app.currentDocument,
            nodePath: path,
            eventName
        })
        window.app.currentDocument = updated
        window.app.renderer.renderDocument(updated)
        window.app.markDocumentModified && window.app.markDocumentModified()
    // Reschedule timers based on the new document state
    try { window.app.startScheduler && window.app.startScheduler() } catch(_) {}
    }

    // Helper function to update a node's value in the document structure by path
    updateNodeValueByPath(document, fieldPath, newValue) {
        try {
            const pathParts = fieldPath.split('/')
            let currentNodes = document
            let targetNode = null
            
            // Navigate to the target node
            for (let i = 0; i < pathParts.length; i++) {
                const part = pathParts[i]
                
                // Handle array-style names with ordinals (e.g., "item#1")
                const [baseName, ordinal] = part.includes('#') ? 
                    part.split('#') : [part, '0']
                const ordinalIndex = parseInt(ordinal, 10)
                
                const matches = currentNodes.filter(node => node.name === baseName)
                if (ordinalIndex >= matches.length) {
                    console.warn('❌ Could not find node at path:', fieldPath, 'missing:', part)
                    return false
                }
                
                targetNode = matches[ordinalIndex]
                
                // If not the last part, move to children
                if (i < pathParts.length - 1) {
                    currentNodes = targetNode.children || []
                }
            }
            
            if (targetNode) {
                console.log('✅ Found target node:', targetNode.name, 'updating value to:', newValue)
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
            console.log('🎯 Selective DOM update for paths:', changedFieldPaths)
            if (fieldChanges.length > 0) {
                console.log('💡 Using field change info for selective updates')
            }
            
            // Create a map of field changes for quick lookup
            const changeMap = new Map()
            for (const change of fieldChanges) {
                changeMap.set(change.path, change)
            }
            
            let updateCount = 0
            
            for (const fieldPath of changedFieldPaths) {
                console.log('🔍 Looking for DOM elements with path:', fieldPath)
                
                const changeInfo = changeMap.get(fieldPath)
                if (changeInfo) {
                    console.log('📝 Field change detected:', changeInfo)
                    
                    // Find the specific element to update
                    const elements = document.querySelectorAll(`[data-path]`)
                    
                    for (const element of elements) {
                        try {
                            const elementPath = JSON.parse(element.dataset.path || '[]')
                            const elementPathStr = elementPath.join('/')
                            
                            // Check if this element's path exactly matches the changed field
                            if (elementPathStr === fieldPath) {
                                console.log('🎯 Found exact matching element for path:', elementPathStr)
                                
                                // Find the corresponding node in the new document
                                const newNode = this.findNodeByPath(newDocument, elementPath)
                                
                                if (newNode) {
                                    console.log('📝 Updating element for changed field:', newNode.name, 'from', changeInfo.oldValue, 'to', changeInfo.newValue)
                                    this.updateSingleElement(element, newNode, elementPath)
                                    updateCount++
                                } else {
                                    console.log('❌ Node not found:', { path: elementPath })
                                }
                                break // Found the exact match, no need to continue
                            }
                        } catch (error) {
                            console.warn('Error processing element:', error)
                        }
                    }
                } else {
                    // Fallback to old comparison-based approach if no change info
                    console.log('⚠️ No change info available, using comparison approach for:', fieldPath)
                    this.updateFieldByComparison(oldDocument, newDocument, fieldPath)
                    updateCount++ // Assume it worked for now
                }
            }
            
            console.log(`✅ Selective update completed: ${updateCount} elements updated`)
            return updateCount > 0
            
        } catch (error) {
            console.error('Error in selective DOM update:', error)
            return false
        }
    }
    
    /**
     * Update DOM for cascade fields that were changed by backend processing
     */
    updateDocumentForCascadeFields(oldDocument, newDocument, userChangedFields, cascadeFields = null) {
        console.log('🔄 Updating DOM for cascade fields after backend processing')
        
        // Use provided cascade fields if available, otherwise compute them
        let fieldsToUpdate = cascadeFields
        if (!fieldsToUpdate) {
            // Find all fields that changed between old and new documents
            const allChangedFields = this.findAllChangedFields(oldDocument, newDocument, '')
            
            // Filter out user-changed fields to get only cascade fields
            fieldsToUpdate = allChangedFields.filter(field => !userChangedFields.includes(field))
        }
        
        console.log('🎯 Cascade fields to update:', fieldsToUpdate)
        
        // Update DOM for each cascade field
        for (const fieldPath of fieldsToUpdate) {
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
        this.collectChangedFields(node1, node2, currentPath, changedFields)
        return changedFields
    }

    /**
     * Recursively collect changed field paths
     */
    collectChangedFields(node1, node2, currentPath, changedFields) {
        if (!node1 || !node2) return

        // Check computed parameters for changes
        if (node1.parameters && node2.parameters) {
            for (const [key, value1] of Object.entries(node1.parameters)) {
                if (key.startsWith('_computed_')) {
                    const value2 = node2.parameters[key]
                    if (!this.valuesEqual(value1, value2)) {
                        const fieldName = key.replace('_computed_', '')
                        const fieldPath = currentPath ? `${currentPath}/${fieldName}` : fieldName
                        changedFields.push(fieldPath)
                        console.log(`📝 Detected cascade change: ${fieldPath}`)
                    }
                }
            }
        }

        // Recursively check children
        if (node1.children && node2.children) {
            for (let i = 0; i < Math.min(node1.children.length, node2.children.length); i++) {
                const child1 = node1.children[i]
                const child2 = node2.children[i]
                if (child1 && child2 && child1.name === child2.name) {
                    const childPath = currentPath ? `${currentPath}/${child1.name}` : child1.name
                    this.collectChangedFields(child1, child2, childPath, changedFields)
                }
            }
        }
    }

    /**
     * Update a single field in the DOM based on document comparison
     */
    updateSingleFieldInDOM(oldDocument, newDocument, fieldPath) {
        console.log(`🔄 Updating single field in DOM: ${fieldPath}`)
        
        // Find the DOM element for this field using the same logic as selective updates
        const elements = document.querySelectorAll(`[data-path]`)
        
        for (const element of elements) {
            try {
                const elementPath = JSON.parse(element.dataset.path || '[]').join('/')
                if (elementPath === fieldPath || (elementPath === fieldPath && element.dataset.path)) {
                    // Get the new computed value
                    const newValue = this.getComputedValueFromDocument(newDocument, fieldPath)
                    if (newValue !== undefined) {
                        console.log(`📝 Updating cascade field ${fieldPath} to:`, newValue)
                        this.updateElementDisplayValue(element, newValue)
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
        console.log(`✅ Updated element display to: ${displayValue}`)
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
                    console.log('🎯 Found matching element for path:', elementPathStr)
                    
                    // Find the corresponding node in the new document
                    const newNode = this.findNodeByPath(newDocument, elementPath)
                    const oldNode = this.findNodeByPath(oldDocument, elementPath)
                    
                    if (newNode && oldNode) {
                        // Check if the node actually changed
                        console.log('🔍 Comparing nodes:', {
                            oldValue: this.getNodeValue(oldNode),
                            newValue: this.getNodeValue(newNode),
                            oldComputed: oldNode.parameters?._computed_value,
                            newComputed: newNode.parameters?._computed_value
                        })
                        
                        if (this.nodeHasChanged(oldNode, newNode)) {
                            console.log('📝 Updating element for changed node:', newNode.name)
                            this.updateSingleElement(element, newNode, elementPath)
                        } else {
                            console.log('⏭️ Node unchanged, skipping:', newNode.name)
                        }
                    } else {
                        console.log('❌ Node not found:', { newNode: !!newNode, oldNode: !!oldNode, path: elementPath })
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
            console.log('🔄 Node type changed:', oldNode.node_type, '->', newNode.node_type)
            return true
        }
        
        // Compare the actual values using getNodeValue
        const oldValue = this.getNodeValue(oldNode)
        const newValue = this.getNodeValue(newNode)
        
        if (JSON.stringify(oldValue) !== JSON.stringify(newValue)) {
            console.log('🔄 Node value changed:', oldValue, '->', newValue)
            return true
        }
        
        // Compare computed values if they exist
        const oldComputed = oldNode.parameters?._computed_value
        const newComputed = newNode.parameters?._computed_value
        
        if (JSON.stringify(oldComputed) !== JSON.stringify(newComputed)) {
            console.log('🔄 Computed value changed:', oldComputed, '->', newComputed)
            return true
        }
        
        console.log('🔄 No relevant changes detected in node:', oldNode.name)
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
                case 'string':
                case 'text':
                    this.updateTextElement(element, newNode)
                    break
                    
                case 'int':
                case 'float':
                    this.updateNumericElement(element, newNode)
                    break
                    
                case 'checkbox':
                    this.updateCheckboxElement(element, newNode)
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
        const textElement = element.querySelector('.field-content, .field-value') || element
        const computedValue = this.getNodeValue(newNode)
        
        if (textElement.textContent !== computedValue) {
            textElement.textContent = computedValue
            console.log('📝 Updated text element:', computedValue)
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
                console.log('📝 Updated numeric element:', computedValue)
            }
        } else {
            // Fallback to text update
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
                console.log('📝 Updated checkbox element:', isChecked)
            }
        }
    }
}
