tab board (label="Board", mutable=true) {
    filter (target="/board/Tasks", text="handle, title", label="find") { }

    div (hidden=true) {
        div Stage (layout="horizontal") {
            string tag = ""
            string name = ""
            string colour = "#6b7280"
        }

        div Task (layout="vertical", spacing=4) {
            string handle (font-size=11px) = ""
            string title = ""
            enum stage (vocabulary="/board/Stages") = "filed" {
                on change {
                    if (cond=$(../stage == "finished")) {
                        append (list="/board/Finished") {
                            - handle = $(../handle)
                            - title = $(../title)
                        }
                        remove (from="/board/Tasks", keyField="handle", keyValue=$(../handle))
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

    list Tasks (entry=<Task>, key="handle", view="board", by="stage") {
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
            - stage = "ready"
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
