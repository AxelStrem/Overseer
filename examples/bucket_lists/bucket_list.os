tab bucket (label="Bucket list", mutable=true) {

    div (hidden=true) {
        div Thing (layout="vertical", width=280px, spacing=0, padding=10, margin=4, border-radius=10px) {
            timestamp added (hidden=true) = "2026-01-01T00:00:00Z"

            div (layout="horizontal", margin=0, spacing=4, alignment="center") {
                text name (label="", font-weight="bold", font-size=15px, width=150px, padding=0) = ""
                int priority (label="", hover-text="Priority", font-size=13px, font-color=gray, width=36px) = 0
                button describe (icon="note", margin=0, hover-text="Describe it",
                                 background-color="transparent", hover-color=#8882,
                                 hidden=$(../description != "")) {
                    on click {
                        start_editing (path="../description")
                    }
                }
                button drop (icon="trash-2", margin=0, hover-text="Remove it",
                             background-color="transparent", hover-color=#8882) {
                    on click {
                        remove (from="/bucket/All", keyField="added", keyValue=$(../added))
                        remove (from="/bucket/Planned", keyField="added", keyValue=$(../added))
                        remove (from="/bucket/Done", keyField="added", keyValue=$(../added))
                    }
                }
            }
            text description (label="", markdown=true, font-size=13px, margin=0,
                              hidden=$(description == "")) = ""
        }
    }

    div (overflow-x=auto, padding=0) {
        div (layout="vertical", width=2102px, spacing=8, padding=0) {
            div NewThing (layout="horizontal", spacing=8, alignment="center", width=1170px, border-style=none, padding=0) {
                textbox name (label="", placeholder="what", width=280px) = ""
                textbox description (label="", placeholder="why, where, who recommended it", width=740px) = ""
                button add (icon="plus", label="Add", margin=0) {
                    on click {
                        if (cond=$(../name == "")) {
                            refuse = "say what it is"
                        }
                        prepend (list="/bucket/All", from="..") {
                            - added = $(now())
                        }
                    }
                }
            }
            div (layout="horizontal", spacing=16, padding=0) {
                div (layout="vertical", width=1170px, spacing=0, padding=0, margin-right=16) {
                    text all_header (font-weight="bold", font-size=18px, margin=0, padding=0) = "All"
                    list All (entry=<Thing>, key="added", movable=true, layout="horizontal", spacing=8) {
                        - {
                            - added = "2026-10-15T09:00:00Z"
                            - name = "Stalker"
                            - priority = 3
                            - description = "Tarkovsky, 1979."
                        }
                        - {
                            - added = "2026-10-15T09:01:00Z"
                            - name = "Seven Samurai"
                            - priority = 2
                        }
                        - {
                            - added = "2026-10-15T09:02:00Z"
                            - name = "The Discreet Charm of the Bourgeoisie"
                            - priority = 1
                        }
                        - {
                            - added = "2026-10-15T09:02:30Z"
                            - name = "Paris, Texas"
                            - priority = 1
                        }
                    }
                }
                div (layout="vertical", width=306px, spacing=0, padding=0, margin-right=16) {
                    text planned_header (font-weight="bold", font-size=18px, margin=0, padding=0) = "Planned"
                    list Planned (entry=<Thing>, key="added", movable=true, layout="horizontal", spacing=8) {
                        - {
                            - added = "2026-10-15T09:03:00Z"
                            - name = "Perfect Days"
                            - priority = 5
                        }
                    }
                }
                div (layout="vertical", width=594px, spacing=0, padding=0) {
                    text done_header (font-weight="bold", font-size=18px, margin=0, padding=0) = "Done"
                    list Done (entry=<Thing>, key="added", movable=true, layout="horizontal", spacing=8) {
                        - {
                            - added = "2026-10-15T09:04:00Z"
                            - name = "Spirited Away"
                            - priority = 4
                            - description = "Watched twice."
                        }
                    }
                }
            }
        }
    }
}
