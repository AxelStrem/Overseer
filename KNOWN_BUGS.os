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

tab known_bugs(label="Known Bugs") {
    
    text header(markdown=true) = "# Current Known Issues"

    text instructions(width=100%) = "To ML agents: Find bug entries with `fixed = false`, carefully read the description. After implementing the fix, allow the user to test it and update the fix date. Do not edit this document by yourself. When implementing a fix, always run all tests to make sure nothing is broken."

    div (hidden=true) {
        div Bug (width = 400px)  {
            int id (hidden=true) = 0
            string description = ""
            div (border-style=none) {
                int storypoints (label="Story Points") = 1
                date fix_date (label="Fix Date") = 10.07.2025
                int priority (label="Priority") = 0
                checkbox fixed (label="Fixed") = false
            }
        }
    }
    
    list BugList(entry=<Bug>) {
        - {
            - id = 0
            - description = "Checkboxes still display their internal names as labels instead of showing clean checkboxes"
            - fixed = true
            - fix_date = 27.07.2025
        }

        - {
            - id = 1
            - description = "Save file doesn't seem to work at all"
            - fixed = true
            - fix_date = 04.08.2025
        }
        
        - {
            - id = 2
            - description = "DEVELOPMENT_PLAN.os fails to parse due to complex nested list structures"
            - fixed = true
            - fix_date = 20.07.2025
        }
        
        - {
            - id = 3
            - description = "Parser returns 0 nodes for complex documents with nested object structures in lists"
            - fixed = true
            - fix_date = 26.07.2025
        }
        
        - {
            - id = 4
            - description = "List items with complex nested syntax (- { - name = value }) not supported"
            - fixed = true
            - fix_date = 26.07.2025
        }

        - {
            - id = 5
            - description = "Strings are still displayed as their names instad of their values sometimes"
            - fixed = true
            - fix_date = 26.07.2025
        }

        - {
            - id = 6
            - description = "Template detection relies on hidden=true parameter instead of allowing any node to be a template"
            - fixed = true
            - storypoints = 3
        }

        - {
            - id = 7
            - description = "When creating node from template, parent node doesn't inherit template's parameters (only children do)"
            - fixed = true
            - storypoints = 2
        }

        - {
            - id = 8
            - description = "Tabs display their node name as a label. This is incorrect behavior for all node types, 
               node name should be internal and not visible in the UI. same as other node types, tabs should support a 'label' parameter instead."
            - fixed = false
            - storypoints = 1
        }

        - {
            - id = 9
            - description = "`text` nodes render as plain text, with no markdown formatting enabled by default. (markdown=true) should be set by default for `text`"
            - fixed = false
            - storypoints = 1
        }

         - {
            - id = 10
            - description = "the way buttons are rendered is sometimes inconsistent with other elements, buttons tend to align to the top of the div with no padding at all;
                            in other cases though buttons align as expected, in line with other elements, so this inconsistency should be investigated."
            - fixed = false
            - storypoints = 1
        }

    }
}
