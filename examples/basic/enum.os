// One value out of a list of them.
//
// An `enum` reads the same vocabulary a `tags` field does and draws the same chip, holding one.
// Pressing it offers the other values in the order the list gives them. A field can say what
// happens when it changes: here, choosing "finished" moves the task to Finished, which is what a
// finish button would otherwise be for.
tab stages (label="Stages", mutable=true) {
    text intro (markdown=true) = "## A plan, by stage"

    div (hidden=true) {
        div Stage (layout="horizontal") {
            string tag = ""
            string name = ""
            string colour = "#6b7280"
        }

        div Task (layout="horizontal", spacing=8, alignment="center") {
            string handle (width=15%) = ""
            string title (width=50%) = ""
            enum stage (vocabulary="/stages/Stages", width=20%) = "filed" {
                on change {
                    if (cond=$(../stage == "finished")) {
                        append (list="/stages/Finished") {
                            - handle = $(../handle)
                            - title = $(../title)
                        }
                        remove (from="/stages/Tasks", keyField="handle", keyValue=$(../handle))
                    }
                }
            }
        }

        div Done (layout="horizontal", spacing=8) {
            string handle (width=15%) = ""
            string title (width=50%) = ""
        }
    }

    list Stages (entry=<Stage>, key="tag", hidden=true) {
        - {
            - tag = "filed"
            - name = "filed"
        }
        - {
            - tag = "asked"
            - name = "asked"
            - colour = "#b45309"
        }
        - {
            - tag = "ready"
            - name = "ready"
            - colour = "#1d4ed8"
        }
        - {
            - tag = "testing"
            - name = "testing"
            - colour = "#7c3aed"
        }
        - {
            - tag = "finished"
            - name = "finished"
            - colour = "#15803d"
        }
    }

    // A formula reads the stage as the text it is.
    int ready (label="ready to start") = $(Tasks.filter(|x| x/stage == "ready").count())

    list Tasks (entry=<Task>, key="handle", layout="vertical") {
        - {
            - handle = "parse"
            - title = "Write the parser"
            - stage = "testing"
        }
        - {
            - handle = "draw"
            - title = "Draw the chips"
            - stage = "ready"
        }
        - {
            - handle = "order"
            - title = "Decide what a stage is called"
            - stage = "asked"
        }
        - {
            - handle = "later"
            - title = "Something nobody has looked at"
        }
    }

    text finished_header (markdown=true) = "### Finished"

    list Finished (entry=<Done>, key="handle", layout="vertical") {
    }
}
