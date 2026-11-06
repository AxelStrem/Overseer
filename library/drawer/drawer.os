tab drawer (label="Drawer", mutable=true, alignment="near") {
    context theme {
        - mono = $(font)
        - field {
            - value-frame = "none"
            - spacing = "2px"
        }
        - button {
            - style = "plain"
            - accent = $(../text)
            - padding = "6px 8px"
            - spacing = "0px"
            - size = "14px"
            - weight = 400
        }
        - tree {
            - style = "none"
            - indent = "12px"
        }
    }
    context hide-labels = true

    div (hidden=true) {
        div Place (layout="horizontal", alignment="center", spacing=0, padding=0, margin=0, border-radius=6px,
                   border-style="none", shadow="none",
                   hover-text=$(path),
                   background-color=$(here ? theme/background : "transparent"),
                   border-left=$(here ? "solid 3px " + theme/accent : "solid 3px transparent")) {
            string name (mutable=$(/drawer/editing), width=1fr, padding="4px 8px",
                         font-size=$(path == "" ? "13px" : "14px"),
                         font-weight=$(here || path == "" ? 600 : "normal"),
                         font-color=$(path == "" ? theme/muted : "inherit")) = ""
            string path (hidden=$(!/drawer/editing), width=1fr, font-size=12px, font-color=$(theme/muted)) = ""
            bool here (hidden=true) = $(path != "" && path == host/open/path)
            button drop (icon="x", hidden=$(!/drawer/editing), hover-text="Take off this list") {
                on click {
                    remove (from="../..", keyField="name", keyValue=$(../name))
                }
            }
            list Places (entry=<Place>, key="name") { }

            on click {
                if (cond=$(./path != "" && !/drawer/editing)) {
                    call (name="open") {
                        - path = $(./path)
                    }
                }
            }
        }

        div Choice {
            string tag = ""
            string name = ""
            string colour = ""
            string icon = ""
            string shape = ""
        }

        list Modes (entry=<Choice>, key="tag") {
            - {
                - tag = "light"
                - name = "Light"
            }
            - {
                - tag = "dark"
                - name = "Dark"
            }
            - {
                - tag = "system"
                - name = "System"
            }
        }

        list Weeks (entry=<Choice>, key="tag") {
            - {
                - tag = "system"
                - name = "Weeks as the locale has them"
            }
            - {
                - tag = "monday"
                - name = "Weeks start on Monday"
            }
            - {
                - tag = "tuesday"
                - name = "Weeks start on Tuesday"
            }
            - {
                - tag = "wednesday"
                - name = "Weeks start on Wednesday"
            }
            - {
                - tag = "thursday"
                - name = "Weeks start on Thursday"
            }
            - {
                - tag = "friday"
                - name = "Weeks start on Friday"
            }
            - {
                - tag = "saturday"
                - name = "Weeks start on Saturday"
            }
            - {
                - tag = "sunday"
                - name = "Weeks start on Sunday"
            }
        }

        list Shapes (entry=<Choice>, key="tag") {
            - {
                - tag = "pill"
                - shape = "pill"
            }
            - {
                - tag = "box"
                - shape = "box"
            }
            - {
                - tag = "slant"
                - shape = "slant"
            }
            - {
                - tag = "round"
                - shape = "round"
            }
            - {
                - tag = "tag"
                - shape = "tag"
            }
            - {
                - tag = "notch"
                - shape = "notch"
            }
            - {
                - tag = "hex"
                - shape = "hex"
            }
            - {
                - tag = "ticket"
                - shape = "ticket"
            }
            - {
                - tag = "bubbles"
                - shape = "bubbles"
            }
            - {
                - tag = "steps"
                - shape = "steps"
            }
            - {
                - tag = "chainsaw"
                - shape = "chainsaw"
            }
            - {
                - tag = "wave"
                - shape = "wave"
            }
        }

        list Chips (entry=<Choice>, key="tag") {
            - {
                - tag = "me"
                - name = $(me/written_name)
                - colour = $(me/colour)
                - icon = $(me/icon)
                - shape = $(me/shape)
            }
            - {
                - tag = "profile"
                - name = $(/drawer/Profile/name)
                - colour = $(/drawer/Profile/colour)
                - icon = $(/drawer/Profile/icon)
                - shape = $(/drawer/Profile/shape)
            }
        }
    }

    bool editing (hidden=true, mutable="guarded") = false
    bool stranger (hidden=true) = $(host/accounts && me/handle == "")
    bool kept (hidden=true) = $(Places.filter(|x| x/path == host/open/path).count() + Places.map(|x| x/Places.filter(|y| y/path == host/open/path).count()).sum() > 0)

    text (font-size=11px, font-weight=600, font-color=$(theme/muted), padding="10px 8px 2px") = "DOCUMENTS"
    div (layout="vertical", alignment="near", hidden=$(stranger), spacing=0, padding=0, margin=0, border-style="none", shadow="none") {
        text (hidden=$(Places.count() > 0), font-size=13px, font-color=$(theme/muted)) = "Nothing kept here yet."
        list Places (entry=<Place>, key="name", tree="Places", movable=$(editing ? "anywhere" : false), width=100%,
                     padding=0, margin=0, border-style="none", shadow="none", background-color="transparent") { }

        button keep (label="+ Keep this document here", hidden=$(host/open/path == "" || kept)) {
            on click {
                append (list="../Places") {
                    - name = $(host/open/label)
                    - path = $(host/open/path)
                }
            }
        }
        button folder (label="+ New folder", hidden=$(!editing)) {
            on click {
                append (list="../Places") {
                    - name = $("Folder " + (../Places.count() + 1))
                }
            }
        }
        button edit (label=$(editing ? "Done" : "Edit this list"), font-color=$(theme/muted), font-size=13px) {
            on click {
                toggle (path="../editing")
            }
        }
    }
    button all (label="All documents", hidden=$(host/kind != "browser" || stranger), font-color=$(theme/muted), font-size=13px) {
        on click {
            call (name="open")
        }
    }

    text (font-size=11px, font-weight=600, font-color=$(theme/muted), padding="10px 8px 2px") = "THIS DOCUMENT"
    text (hidden=$(host/open/path == ""), font-size=12px, font-color=$(theme/muted)) = $(host/open/path)
    button undo (label="Undo", hover-text="Take back the last change (Ctrl+Z)", hidden=$(host/open/can != "edit")) {
        on click {
            call (name="undo")
        }
    }
    button open_file (label="Open a file…", hidden=$(host/kind != "desktop")) {
        on click {
            call (name="open_file")
        }
    }
    button new_file (label="New file…", hidden=$(host/kind != "desktop")) {
        on click {
            call (name="new_file")
        }
    }

    div (layout="vertical", alignment="near", hidden=$(stranger), padding=0, margin=0, border-style="none", shadow="none") {
        text (font-size=11px, font-weight=600, font-color=$(theme/muted), padding="10px 8px 2px") = "LOOK"
        enum look (label="Theme", vocabulary="/host/themes", view="select", mutable="guarded", width=100%) = $(host/theme) {
            on change {
                call (name="look") {
                    - theme = $(../look)
                }
            }
        }
        enum shade (label="Light or dark", vocabulary="/drawer/Modes", view="segments", mutable="guarded", width=100%) = $(host/chosen_mode) {
            on change {
                call (name="look") {
                    - mode = $(../shade)
                }
            }
        }
        enum week (label="Weeks start on", vocabulary="/drawer/Weeks", view="select", mutable="guarded", width=100%) = $(host/chosen_week_start) {
            on change {
                call (name="look") {
                    - week_start = $(../week)
                }
            }
        }
    }

    div (layout="vertical", alignment="near", hidden=$(me/handle == ""), padding=0, margin=0, border-style="none", shadow="none") {
        div (layout="horizontal", alignment="center", spacing=6) {
            text (font-size=11px, font-weight=600, font-color=$(theme/muted), padding="10px 8px 2px") = "SIGNED IN AS"
            enum who (vocabulary="/drawer/Chips", mutable=false) = "me"
        }
        button sharing (label="Sharing") {
            on click {
                call (name="sharing")
            }
        }

        div Profile (label="Profile", foldable=true, folded=true, mutable="guarded") {
            context hide-labels = false
            textbox name (label="Display name") = $(me/written_name)
            text hint (font-size=12px, font-color=$(theme/muted)) = "Markdown and icons are drawn in it, as in any text."
            div (layout="horizontal", alignment="center") {
                string colour (label="Chip colour", kind="color") = $(me/colour)
                enum preview (label="", vocabulary="/drawer/Chips", mutable=false) = "profile"
            }
            div (layout="horizontal", alignment="center") {
                enum icon (label="Chip icon", vocabulary="/host/icons", placeholder="none") = $(me/icon)
                button no_icon (label="None", font-color=$(theme/muted), font-size=13px) {
                    on click {
                        set (path="../icon") = ""
                    }
                }
            }
            div (layout="horizontal", alignment="center") {
                enum shape (label="Chip shape", vocabulary="/drawer/Shapes", placeholder="default") = $(me/shape)
                button default_shape (label="Default", font-color=$(theme/muted), font-size=13px) {
                    on click {
                        set (path="../shape") = ""
                    }
                }
            }
            button save (label="Save the name, colour, icon and shape") {
                on click {
                    call (name="save_profile", into="../said") {
                        - name = $(../name)
                        - colour = $(../colour)
                        - icon = $(../icon)
                        - shape = $(../shape)
                    }
                }
            }
            string said (hidden=$(said == ""), font-size=12px, font-color=$(theme/muted)) = ""
            textbox current (label="Current password", kind="secret") = ""
            textbox password (label="New password", kind="secret") = ""
            button change (label="Change the password") {
                on click {
                    call (name="change_password", into="../changed") {
                        - current = <../current>
                        - password = <../password>
                    }
                }
            }
            string changed (hidden=$(changed == ""), font-size=12px, font-color=$(theme/muted)) = ""
        }

        div Keys (label="Keys", foldable=true, folded=true, mutable="guarded") {
            text (font-size=12px, font-color=$(theme/muted)) = "A key lets a program act as you through the API."
            text (markdown=true, font-size=13px, hidden=$(me/keys.count() == 0)) = $(me/keys.reduce("", |all, x| all + (all == "" ? "" : ", ") + "**" + x/name + "** (" + (x/used == "" ? "never used" : "used " + day(x/used)) + ")"))
            div (layout="horizontal", alignment="center") {
                textbox name (label="Name", placeholder="desktop bot", width=1fr) = ""
                button make (label="Make a key") {
                    on click {
                        call (name="make_key", into="../made") {
                            - name = $(../name)
                        }
                        set (path="../name") = ""
                    }
                }
            }
            div (hidden=$(made == ""), layout="horizontal", alignment="center") {
                string made (label="Copy it now: it is not shown again", width=1fr, font-size=12px) = ""
                button copy (label="Copy") {
                    on click {
                        call (name="copy", into="../said") {
                            - text = $(../made)
                        }
                    }
                }
            }
            div (hidden=$(me/keys.count() == 0), layout="horizontal", alignment="center") {
                enum revoking (label="", vocabulary="/me/keys", placeholder="a key to revoke") = ""
                button revoke (label="Revoke", hidden=$(revoking == "")) {
                    on click {
                        call (name="drop_key", into="../said") {
                            - name = $(../revoking)
                        }
                        set (path="../revoking") = ""
                        set (path="../made") = ""
                    }
                }
            }
            string said (hidden=$(said == ""), font-size=12px, font-color=$(theme/muted)) = ""
        }

        div People (label="People", foldable=true, folded=true, hidden=$(!me/admin), mutable="guarded") {
            button invite (label="Invite someone") {
                on click {
                    call (name="invite", into="../link")
                }
            }
            div (layout="horizontal", alignment="center") {
                textbox username (label="Username", width=1fr) = ""
                button reset (label="Send a new password") {
                    on click {
                        call (name="reset_password", into="../link") {
                            - person = $(../username)
                        }
                    }
                }
            }
            div (hidden=$(link == ""), layout="horizontal", alignment="center") {
                string link (label="The link, good for 7 days", width=1fr, font-size=12px) = ""
                button copy (label="Copy") {
                    on click {
                        call (name="copy", into="../said") {
                            - text = $(../link)
                        }
                    }
                }
            }
            string said (hidden=$(said == ""), font-size=12px, font-color=$(theme/muted)) = ""
        }

        button sign_out (label="Sign out", font-color=$(theme/muted), font-size=13px) {
            on click {
                call (name="sign_out")
            }
        }
    }
}
