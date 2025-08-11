// Disappearing comments on save (low priority)
// Description: When saving from the app, comments in the original file may be lost.
// Root cause: The save flow merges comments using the UI-provided regenerated content as
// the merge source rather than reading the on-disk original. As a result, comment blocks
// are not available for merging back.
// Status: Low priority; defer fix. Proposed fix is to have the backend read current
// on-disk file for the merge source (fallback to UI content if unreadable/new file).

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
    
    list BugList(entry=<Bug>) {
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

        - {
            - description = "Template detection relies on hidden=true parameter instead of allowing any node to be a template"
            - fixed = false
            - storypoints = 3
        }

        - {
            - description = "When creating node from template, parent node doesn't inherit template's parameters (only children do)"
            - fixed = false
            - storypoints = 2
        }
    }
}
