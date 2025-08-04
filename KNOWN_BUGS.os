// Known Bugs and Issues Tracker
// Created: 2025-07-26
// Purpose: Track bugs and issues found during development

tab (title="Known Bugs") {
    
    text header = "Current Known Issues"

    div Bug (hidden=true) {
        string description = ""
        bool fixed = false
        int storypoints = 1
        date fix_date = 10.07.2025
    }
    
    list BugList(entry=<../Bug>) {
        - {
            - description = "Checkboxes still display their internal names as labels instead of showing clean checkboxes"
            - fixed = true
            - fix_date = 27.07.2025
        }

        - {
            - description = "Save file doesn't seem to work at all"
            - fixed = true
            - fix_date = 04.08.2025
        }
        
        - {
            - description = "DEVELOPMENT_PLAN.os fails to parse due to complex nested list structures"
            - fixed = true
            - fix_date = 20.07.2025
        }
        
        - {
            - description = "Parser returns 0 nodes for complex documents with nested object structures in lists"
            - fixed = true
            - fix_date = 26.07.2025
        }
        
        - {
            - description = "List items with complex nested syntax (- { - name = value }) not supported"
            - fixed = true
            - fix_date = 26.07.2025
        }

        - {
            - description = "Strings are still displayed as their names instad of their values sometimes"
            - fixed = true
            - fix_date = 26.07.2025
        }
    }
}
