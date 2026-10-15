tab bucket (label="Bucket list", mutable=true) {

    div (hidden=true) {
        div Thing (layout="vertical", width=190px, spacing=4, padding=8, shadow="soft") {
            timestamp added (hidden=true) = "2026-01-01T00:00:00Z"

            div (layout="horizontal", margin=0, spacing=4, alignment="center") {
                string name (label="", font-weight="bold", width=40%) = ""
                int priority (label="", width=28px) = 0
                button describe (icon="note", margin=0, hover-text="Describe it",
                                 hidden=$(../description != "")) {
                    on click {
                        start_editing (path="../description")
                    }
                }
                button drop (icon="trash-2", margin=0, hover-text="Remove it") {
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

    div NewThing (layout="horizontal", spacing=8, alignment="center") {
        textbox name (label="", placeholder="what", width=30%) = ""
        textbox description (label="", placeholder="why, where, who recommended it") = ""
        button add (label="add", margin=0) {
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

    div (overflow-x=auto) {
        div (layout="horizontal", spacing=16, width=1500px) {
            div (layout="vertical", width=820px) {
                text all_header (markdown=true) = "### All"
                list All (entry=<Thing>, key="added", movable=true, layout="horizontal", spacing=6) {
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
                        - name = "Paris, Texas"
                        - priority = 1
                    }
                }
            }
            div (layout="vertical", width=220px) {
                text planned_header (markdown=true) = "### Planned"
                list Planned (entry=<Thing>, key="added", movable=true, layout="horizontal", spacing=6) {
                    - {
                        - added = "2026-10-15T09:03:00Z"
                        - name = "Perfect Days"
                        - priority = 5
                    }
                }
            }
            div (layout="vertical", width=420px) {
                text done_header (markdown=true) = "### Done"
                list Done (entry=<Thing>, key="added", movable=true, layout="horizontal", spacing=6) {
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
