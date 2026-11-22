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
            mark (type="grip", padding="4px 8px", margin=0, font-color=$(theme/muted))
            string name (hidden=true, width=1fr, padding="0px 8px") = ""
            string shown (mutable=false, width=1fr, padding="0px 8px",
                          font-size=$(path == "" ? "13px" : "14px"),
                          font-weight=$(here || path == "" ? 600 : "normal"),
                          font-color=$(path == "" ? theme/muted : "inherit")) = $(name)
            string path (hidden=true, width=1fr, font-size=12px, font-color=$(theme/muted)) = ""
            bool here (hidden=true) = $(path != "" && path == host/open/path)
            button more (label="…", padding="0px 8px", font-color=$(theme/muted), hover-text="Rename, move or take off") {
                on click {
                    toggle_fold (path="../menu")
                }
            }
            div menu (foldable=true, folded=true, float=true, layout="vertical", alignment="near", spacing=0) {
                button rename (label="Rename") {
                    on click {
                        start_editing (path="../../name")
                        fold (path="..")
                    }
                }
                button repath (label="Edit path") {
                    on click {
                        start_editing (path="../../path")
                        fold (path="..")
                    }
                }
                button drop (label="Remove") {
                    on click {
                        remove (from="../../..", keyField="name", keyValue=$(../../name))
                    }
                }
            }
            list Places (entry=<Place>, key="name") { }

            on click {
                if (cond=$(./path != "")) {
                    call (name="open") {
                        - path = $(./path)
                    }
                }
            }
        }

        div Heading (layout="horizontal", alignment="center", width=100%, spacing=0, padding=0, margin=0, border-style="none", shadow="none") {
            text (width=1fr, font-size=11px, font-weight=600, font-color=$(theme/muted), padding="12px 8px 2px") = ""
        }

        div Choice {
            string tag = ""
            string name = ""
            string hover = ""
            string colour = ""
            string icon = ""
            string shape = ""
        }

        list Modes (entry=<Choice>, key="tag") {
            - {
                - tag = "light"
                - name = "<icon=sun>"
                - hover = "Light"
            }
            - {
                - tag = "dark"
                - name = "<icon=moon>"
                - hover = "Dark"
            }
            - {
                - tag = "system"
                - name = "<icon=monitor>"
                - hover = "System"
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

    bool stranger (hidden=true) = $(host/accounts && me/handle == "")
    bool kept (hidden=true) = $(Places.filter(|x| x/path == host/open/path).count() + Places.map(|x| x/Places.filter(|y| y/path == host/open/path).count()).sum() > 0)

    div (hidden=$(stranger), layout="vertical", alignment="near", spacing=0, padding=0, margin=0, border-style="none", shadow="none") {
        <Heading> {
            text = "DOCUMENTS"
            button all (label="All documents", hidden=$(host/kind != "browser"), font-color=$(theme/muted), font-size=12px, padding="12px 8px 2px") {
                on click {
                    call (name="open")
                }
            }
        }
        text (hidden=$(Places.count() > 0), font-size=13px, font-color=$(theme/muted), padding="2px 8px") = "Nothing kept here yet."
        list Places (entry=<Place>, key="name", tree="Places", movable="anywhere", width=100%,
                     padding=0, margin=0, border-style="none", shadow="none", background-color="transparent") { }

        div (layout="horizontal", alignment="center", spacing=0, padding=0, margin=0, border-style="none", shadow="none") {
            button keep (label="+ Keep this document", hover-text="Keep the open document here", hidden=$(host/open/path == "" || kept),
                         font-color=$(theme/muted), font-size=13px) {
                on click {
                    append (list="../Places") {
                        - name = $(host/open/label)
                        - path = $(host/open/path)
                    }
                }
            }
            button adding (label="+ Add…", hover-text="Add a document by its path", font-color=$(theme/muted), font-size=13px) {
                on click {
                    toggle_fold (path="../Adding")
                }
            }
        }
        div Adding (layout="vertical", alignment="near", mutable="guarded", foldable=true, folded=true,
                    spacing=2, padding="2px 8px 6px", margin=0, border-style="none", shadow="none") {
            textbox name (placeholder=$(described != "" ? described : "name"), width=100%) = ""
            textbox path (placeholder="document path", width=100%) = "" {
                on change {
                    call (name="describe", into="../described") {
                        - path = $(../path)
                    }
                }
            }
            string described (hidden=true) = ""
            button add (label="Add") {
                on click {
                    if (cond=$(../name == "" && ../described == "" && ../path == "")) {
                        refuse = "Give it a name, or the path of a document."
                    }
                    if (cond=$(/drawer/Places.filter(|x| x/name == (../name != "" ? ../name : (../described != "" ? ../described : ../path))).count() > 0)) {
                        refuse = "There is one by that name here already."
                    }
                    append (list="../../Places", from="..") {
                        - name = $(../name != "" ? ../name : (../described != "" ? ../described : ../path))
                    }
                    set (path="../described") = ""
                    fold (path="..")
                }
            }
        }
    }

    div (hidden=$(host/open/path == "" && host/open/can != "edit" && host/kind != "desktop"), layout="vertical", alignment="near", spacing=0, padding=0, margin=0, border-style="none", shadow="none") {
        <Heading> {
            text = "THIS DOCUMENT"
        }
        text (hidden=$(host/open/path == ""), font-size=12px, font-color=$(theme/muted), padding="0px 8px 2px") = $(host/open/path)
        div (layout="horizontal", alignment="center", spacing=0, padding=0, margin=0, border-style="none", shadow="none") {
            button undo (label="Undo", hover-text="Take back the last change (Ctrl+Z)", hidden=$(host/open/can != "edit")) {
                on click {
                    call (name="undo")
                }
            }
            button open_file (label="Open…", hover-text="Open a file", hidden=$(host/kind != "desktop")) {
                on click {
                    call (name="open_file")
                }
            }
            button new_file (label="New…", hover-text="Make a new file", hidden=$(host/kind != "desktop")) {
                on click {
                    call (name="new_file")
                }
            }
        }
    }

    div (hidden=$(stranger), layout="vertical", alignment="near", spacing=0, padding=0, margin=0, border-style="none", shadow="none") {
        <Heading> {
            text = "LOOK"
        }
        div (layout="horizontal", alignment="center", width=100%, spacing=6, padding="0px 8px 0px 0px", margin=0, border-style="none", shadow="none") {
            enum look (label="Theme", vocabulary="/host/themes", view="select", mutable="guarded", width=1fr) = $(host/theme) {
                on change {
                    call (name="look") {
                        - theme = $(../look)
                    }
                }
            }
            enum shade (label="Light or dark", vocabulary="/drawer/Modes", view="segments", mutable="guarded") = $(host/chosen_mode) {
                on change {
                    call (name="look") {
                        - mode = $(../shade)
                    }
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

    div (hidden=$(me/handle == ""), layout="vertical", alignment="near", spacing=0, padding=0, margin=0, border-style="none", shadow="none") {
        <Heading> {
            text = "ACCOUNT"
        }
        div (layout="horizontal", alignment="center", width=100%, spacing=6, padding="2px 0px 2px 8px", margin=0, border-style="none", shadow="none") {
            enum who (vocabulary="/drawer/Chips", mutable=false, width=1fr) = "me"
            button sign_out (label="Sign out", font-color=$(theme/muted), font-size=13px) {
                on click {
                    call (name="sign_out")
                }
            }
        }
        button sharing (label="Sharing") {
            on click {
                call (name="sharing")
            }
        }

        div Profile (label="Profile", foldable=true, folded=true, font-size=14px, padding="4px 8px", margin=0, border-style="none", shadow="none", background-color="transparent", mutable="guarded") {
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

        div Keys (label="Keys", foldable=true, folded=true, font-size=14px, padding="4px 8px", margin=0, border-style="none", shadow="none", background-color="transparent", mutable="guarded") {
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

        div People (label="People", foldable=true, folded=true, font-size=14px, padding="4px 8px", margin=0, border-style="none", shadow="none", background-color="transparent", hidden=$(!me/admin), mutable="guarded") {
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
    }
}
