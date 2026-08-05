// Known Bugs and Issues Tracker
// Created: 2025-07-26
// Purpose: Track bugs and issues found during development

tab known_bugs (label="Known Bugs") {
    
    text header (markdown=true) = "# Current Known Issues"

    text instructions (width=100%) = "To ML agents: Find bug entries with `fixed = false`, carefully read the description. After implementing the fix, allow the user to test it and update the fix date. Do not edit this document by yourself. When implementing a fix, always run all tests to make sure nothing is broken."

    div (hidden=true) {
        div Bug (width=400px) {
            int id (hidden=true) = 0
            string description = ""
            div {
                int storypoints (label="Story Points") = 1
                string fix_date (label="Fix Date") = 10.07
                int priority (label="Priority") = 0
                checkbox fixed (label="Fixed") = false

            }
        }

        div Feature (width=400px) {
            int id (hidden=true) = 0
            string description = ""
            div {
                int storypoints (label="Story Points") = 1
                string implemented_on (label="Implemented On") = 10.07
                int priority (label="Priority") = 0
                checkbox fixed (label="Fixed") = false

            }
        }
    }
    
    list BugList (entry=<Bug>) {

        - {
            - id = 0
            - description = "Checkboxes still display their internal names as labels instead of showing clean checkboxes"
            - fix_date = 27.07
            - fixed = true
        }
        - {
            - id = 1
            - description = "Save file doesn't seem to work at all"
            - fix_date = 4.08
            - fixed = true
        }
        - {
            - id = 2
            - description = "DEVELOPMENT_PLAN.os fails to parse due to complex nested list structures"
            - fix_date = 20.07
            - fixed = true
        }
        - {
            - id = 3
            - description = "Parser returns 0 nodes for complex documents with nested object structures in lists"
            - fix_date = 26.07
            - fixed = true
        }
        - {
            - id = 4
            - description = "List items with complex nested syntax (- { - name = value }) not supported"
            - fix_date = 26.07
            - fixed = true
        }
        - {
            - id = 5
            - description = "Strings are still displayed as their names instad of their values sometimes"
            - fix_date = 26.07
            - fixed = true
        }
        - {
            - id = 6
            - description = "Template detection relies on hidden=true parameter instead of allowing any node to be a template"
            - storypoints = 3
            - fixed = true
        }
        - {
            - id = 7
            - description = "When creating node from template, parent node doesn't inherit template's parameters (only children do)"
            - storypoints = 2
            - fixed = true
        }
        - {
            - id = 8
            - description = "Tabs display their node name as a label. This is incorrect behavior for all node types, 
               node name should be internal and not visible in the UI. same as other node types, tabs should support a 'label' parameter instead."
            - storypoints = 1
            - fixed = false
        }
        - {
            - id = 9
            - description = "`text` nodes render as plain text, with no markdown formatting enabled by default. (markdown=true) should be set by default for `text`"
            - storypoints = 1
            - fix_date = 19.08
            - fixed = true
        }
        - {
            - id = 10
            - description = "the way buttons are rendered is sometimes inconsistent with other elements, buttons tend to align to the top of the div with no padding at all;
                            in other cases though buttons align as expected, in line with other elements, so this inconsistency should be investigated."
            - storypoints = 1
            - fix_date = 19.08
            - fixed = true
        }
        - {
            - id = 11
            - description = "When nodes are serialized, the parameters change order in which they are listed (seemingly at random). Node parameters should always be serialized in the same order, and they should preserve the original order when deserialized."
            - storypoints = 1
            - fix_date = 19.08
            - fixed = true
        }
        - {
            - id = 12
            - description = "Field values in template inheritance hierarchies get reset during periodic document re-evaluation, causing user input to disappear and UI to flicker. This was caused by the reevaluateDocument() function performing full document serialize->parse->resolve cycles every 60 seconds, which destroyed user modifications and caused performance issues."
            - storypoints = 2
            - fix_date = 26.12
            - fixed = true
        }
        - {
            - id = 13
            - description = "When editing a field in the UI that is a part of a list, the document gets corrupted (looks like all lists lose their entry templates); This only happens on frontend, after save and reload everything looks correct, including the change"
            - storypoints = 5
            - fixed = true
            - fix_date = 22.08
        }
        - {
            - id = 14
            - description = "Documents refresh periodically, resetting the UI scroll and causing flicker and refresh animation on plot charts. This is an old behavior that was needed before dependency graph for formulas was introduced. Now it's redundant and has to be removed."
            - storypoints = 5
            - fixed = true
            - fix_date = 22.08
        }
        - {
            - id = 15
            - description = "Timers are not immediately updated upon loading a document; instead user has to wait for a few seconds looking at an incorrect unupdated document before the timers fire. All timers in the document should be checked and processed at the moment the document is loaded, before rendering it and presenting to the user"
            - storypoints = 5
            - fixed = false
        }
        - {
            - id = 16
            - description = "Formula propagation is broken on the frontend; Document has to be saved and reloaded to display the correct values"
            - storypoints = 3
            - fix_date = 22.08
            - fixed = true
        }
        - {
            - id = 17
            - description = "A standalone comment line containing only the two slash characters is deleted on save, and every comment line above it in the same leading block is relocated to the end of the document. Supersedes the older note about disappearing comments on save, whose stated root cause (the merge_comments path) no longer exists. Repro: src-tauri/tests/comment_trivia.rs, test bare_comment_marker_survives_round_trip, currently marked ignore."
            - storypoints = 2
            - fixed = false
        }
    }

     list FeatureList (entry=<Feature>) {
         - {
            - id = 1
            - description = "primitive type should also have `layout` parameter that would only affect the placement of the label relative to the field value; this layout parameter should follow the default logic of div layout: opposite to parent by default"
            - storypoints = 1
            - fixed = false
        }

        - {
            - id = 2
            - description = "buttons should not simply inherit parent div background color; instead button default background color should be same hue as parent background but 20% brighter, on hover it should be 20% more brighter yet, and on press it should be 20% darker. Also all of these colors should be configurable via parameters"
            - storypoints = 2
            - fixed = false
        }

         - {
            - id = 3
            - description = "add support for themes and styles"
            - storypoints = 4
            - fixed = false
        }

         - {
            - id = 4
            - description = "add support for background fallback formulas (hidden from UI)"
            - storypoints = 5
            - fixed = false
        }
        - {
            - id = 5
            - description = "add support for more chart types (dots, bars, pie charts)"
            - storypoints = 5
        }
        - {
            - id = 6
            - description = "add a 'hide-labels` parameter to divs and other container nodes"
            - storypoints = 5
        }
        - {
            - id = 7
            - description = "add list headers"
            - storypoints = 5
        }
        - {
            - id = 8
            - description = "add support for list scrolling (and maybe for other containers too?)"
            - storypoints = 5
        }
     }
}
