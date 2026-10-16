tab bucket (label="Bucket list", mutable=true) {

    div (hidden=true) {
        div Label (layout="vertical", width=360px, margin=0, spacing=0, padding=2, border-radius=8px,
                   background-color=$(colour + "26")) {
            div (layout="horizontal", margin=0, spacing=4, alignment="center") {
                string name (label="", font-weight="bold", font-size=13px, padding=0, width=38%) = ""
                string tag (label="", font-size=11px, font-color="#9ca3af", padding=0, width=24%) = ""
                string colour (label="", kind="color", font-size=11px, font-color="#9ca3af", padding=0, width=28%) = "#6b7280"
                button about (icon="note", margin=0, padding=0, width=10%, hover-text="What this tag is about",
                              background-color="transparent", hover-color=#8882,
                              font-color=$(../About/about == "" ? "#6b7280" : "inherit")) {
                    on click {
                        toggle_fold (path="../About")
                    }
                }
            }
            div About (foldable=true, folded=true, margin=0) {
                text about (label="", markdown=true, font-size=12px, padding=4) = ""
            }
        }

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
            tags labels (label="", vocabulary="/bucket/Labels", font-size=12px, margin=0) = ""
            text description (label="", markdown=true, font-size=13px, margin=0,
                              hidden=$(description == "")) = ""
        }
    }

    div (overflow-x=auto, padding=0) {
        div (layout="vertical", width=2102px, spacing=8, padding=0) {
            div NewThing (layout="horizontal", spacing=8, alignment="center", width=1170px, border-style=none, padding=0) {
                textbox name (label="", placeholder="what", width=280px) = ""
                textbox description (label="", placeholder="why, where, who recommended it", width=540px) = ""
                textbox labels (label="", placeholder="tags", vocabulary="/bucket/Labels", width=192px) = ""
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
                            - labels = "classic"
                            - description = "Tarkovsky, 1979."
                        }
                        - {
                            - added = "2026-10-15T09:01:00Z"
                            - name = "Seven Samurai"
                            - priority = 2
                            - labels = "classic"
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
                            - labels = "friends"
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
                            - labels = "anime"
                            - description = "Watched twice."
                        }
                    }
                }
            }
            div (layout="vertical", width=1170px, spacing=6, padding=0, border-style=none) {
                text tags_header (font-weight="bold", font-size=18px, margin=0, padding=0) = "Tags"
                list Labels (entry=<Label>, key="tag", layout="horizontal", spacing=6, border-style=none, padding=0) {
                    - {
                        - tag = "classic"
                        - name = "classic"
                        - colour = "#b45309"
                    }
                    - {
                        - tag = "anime"
                        - name = "anime"
                        - colour = "#db2777"
                    }
                    - {
                        - tag = "friends"
                        - name = "with friends"
                        - colour = "#0e8a16"
                    }
                }
                div NewTag (layout="horizontal", margin=0, spacing=6, alignment="center", border-style=none, padding=0) {
                    textbox tag (label="", placeholder="tag", width=120px) = ""
                    textbox name (label="", placeholder="shown as", width=200px) = ""
                    textbox colour (label="", kind="color", width=56px) = "#6b7280"
                    button add (icon="plus", label="tag", margin=0) {
                        on click {
                            if (cond=$(../tag == "")) {
                                refuse = "say a tag"
                            }
                            if (cond=$(/bucket/Labels.filter(|x| x/tag == ../tag).count() > 0)) {
                                refuse = "this tag is there already"
                            }
                            append (list="/bucket/Labels", from="..")
                        }
                    }
                }
            }
        }
    }
}
