// Known Bugs and Issues Tracker
// Created: 2025-07-26
// Purpose: Track bugs and issues found during development

tab (title="Known Bugs") {
    
    text header = "Current Known Issues"

    div Bug (hidden=true) {
        string description = ""
        bool fixed = false
    }
    
    list BugList(entry=../Bug) {
        - {
            - description = "Checkboxes still display their internal names as labels instead of showing clean checkboxes"
        }

        - {
            - description = "Save file doesn't seem to work at all"
        }
        
        - {
            - description = "DEVELOPMENT_PLAN.os fails to parse due to complex nested list structures"
        }
        
        - {
            - description = "Parser returns 0 nodes for complex documents with nested object structures in lists"
        }
        
        - {
            - description = "List items with complex nested syntax (- { - name = value }) not supported"
        }
    }
}
