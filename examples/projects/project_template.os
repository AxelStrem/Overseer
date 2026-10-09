// Stages, for an agent working through the items below. Unless told otherwise, take 'ready' items
// first, then 'planning', and 'filed' only when neither is left; within a stage, items flagged
// first go before the rest, and past that, use your judgement. Skip any item flagged manual or
// asked, and any whose after list names an item that is still open.
//
//   ready     Everything is decided: implement it, then move it to testing.
//   planning  Its plan is being worked out: finish it, and move it to ready once nothing is left
//             to decide.
//   filed     Nothing done yet. Move it to ready with its plan if nothing is left to decide - or,
//             if it has to wait for another item first, add that one to its after list.
//   testing, later
//             Waiting on a person: leave them.
//
// Flags, the same in every project, beside its own tags:
//
//   manual    For a person to work: leave it.
//   auto      Filed by an agent of its own accord. File what you find along the way, flagged auto.
//   first     Goes before the rest of its stage.
//   asked     A question in its note waits on a person, who takes the flag off once it is answered.
//   discuss   Only your thoughts are wanted: give them in the note, move it at most to planning,
//             and flag it asked. Never move it to ready.
//
// Whenever something needs a person's decision, at whatever stage, add the question to the note,
// move the item to planning and flag it asked. Agents set no flag but auto and asked, and take
// none off.
//
// Whenever you move an item to ready, set its complexity - how much thinking building it takes,
// given the plan in its note, where points say how much work: trivial (straightforward or
// boilerplate, however much of it), low, medium, or high (a good understanding of several parts
// of the project and real analysis).
//
// Whenever you move an item, add what you did or found to the end of its note. Finishing,
// cancelling or dropping an item is for a person.

tab project (label="Project", mutable=true) {
    string name (label="", font-size=22px) = "Untitled project"
    text description (markdown=true, font-size=14px) = "**What it is:** say in a sentence or two what the project is. **Items:** a larger piece of work is the parent of smaller ones and reads how much of them is done; points say how big each is, 1 the smallest thing worth writing down and 8 most of a day; think says how much thinking building it takes, from trivial, straightforward however much of it, through low and medium to high, which wants a good understanding of several parts of the project - unassigned until it is ready; after names the items one waits on, and it is drawn grey while any of them is still open. **Stages:** an item starts filed; planning means its plan is being worked out; ready that nothing is left to decide; testing that it is done and waits to be confirmed; later that it is decided against for now. Picking finished moves it to Closed, from where it reaches the task history. Picking cancelled moves it there too, marked cancelled: it counts as neither work done nor an open part of its parent, and never reaches the task history. Neither can be picked while any part of the item is still open. **Flags:** the same in every project, unlike the tags. manual marks an item for a person to work, which agents leave alone; auto one an agent filed of its own accord; first one to take before the rest of its stage; asked one whose note holds a question for the owner - once it is answered there, the flag comes off and work goes on; discuss one the owner wants an opinion on before anything is decided, which an agent gives in the note, moving the item at most to planning and flagging it asked. Say what the tags mean here. **Tabs:** name each tab and what it holds, starting with this one, the plan, which has two pages: Open, what is still to do, and Closed, what was finished or cancelled. A tab beside this one is the project's own, outside both pages."

    div (hidden=true) {
        div Label (layout="horizontal", margin=0, spacing=6, alignment="center") {
            string tag (label="", width=25%) = ""
            string name (label="", width=45%) = ""
            string colour (label="", kind="color", width=30%) = "#6b7280"
        }

        list Stages (entry=<Label>, key="tag") {
            - {
                - tag = "filed"
                - name = "filed"
                - colour = "#6b7280"
            }
            - {
                - tag = "planning"
                - name = "planning"
                - colour = "#0f766e"
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
                - tag = "later"
                - name = "later"
                - colour = "#475569"
            }
            - {
                - tag = "finished"
                - name = "finished"
                - colour = "#15803d"
            }
            - {
                - tag = "cancelled"
                - name = "cancelled"
                - colour = "#9f1239"
            }
        }

        list Complexities (entry=<Label>, key="tag") {
            - {
                - tag = "unassigned"
                - name = "unassigned"
                - colour = "#6b7280"
            }
            - {
                - tag = "trivial"
                - name = "trivial"
                - colour = "#15803d"
            }
            - {
                - tag = "low"
                - name = "low"
                - colour = "#0f766e"
            }
            - {
                - tag = "medium"
                - name = "medium"
                - colour = "#b45309"
            }
            - {
                - tag = "high"
                - name = "high"
                - colour = "#b91c1c"
            }
        }

        list Statuses (entry=<Label>, key="tag") {
            - {
                - tag = "finished"
                - name = "finished"
                - colour = "#15803d"
            }
            - {
                - tag = "cancelled"
                - name = "cancelled"
                - colour = "#9f1239"
            }
        }

        list Flags (entry=<Label>, key="tag") {
            - {
                - tag = "manual"
                - name = "manual"
                - colour = "#78716c"
            }
            - {
                - tag = "auto"
                - name = "auto"
                - colour = "#6b7280"
            }
            - {
                - tag = "first"
                - name = "first"
                - colour = "#e11d48"
            }
            - {
                - tag = "asked"
                - name = "asked"
                - colour = "#b45309"
            }
            - {
                - tag = "discuss"
                - name = "discuss"
                - colour = "#0369a1"
            }
        }

        div Item (layout="horizontal", margin=0, spacing=6, padding=2, alignment="center",
                  background-color=$(done >= 75 ? "#14321f" :
                                    (done >= 25 ? "#1c2a3a" : "inherit")),
                  font-color=$(waiting > 0 ? "#6b7280" : "inherit")) {
            timestamp added (hidden=true) = "2026-01-01T00:00:00Z"
            string handle (label="id", font-size=11px, width=8%) = ""
            string title (label="title", font-size=14px, width=17%,
                          font-weight=$(../kids > 0 ? "bold" : "normal")) = ""

            enum stage (label="stage", vocabulary="/project/Stages", width=9%,
                        withhold=$(open_kids > 0 ? "finished, cancelled" : "")) = "filed" {
                on change {
                    if (cond=$((../stage == "finished" || ../stage == "cancelled") && ../open_kids == 0)) {
                        append (list="/project/History") {
                            - finished_at = $(now())
                            - added = $(../added)
                            - handle = $(../handle)
                            - title = $(../title)
                            - status = $(../stage)
                            - parent = $(../parent)
                            - after = $(../after)
                            - labels = $(../labels)
                            - flags = $(../flags)
                            - points = $(../points)
                            - complexity = $(../complexity)
                            - commentary = $(../commentary)
                        }
                        remove (from="/project/Items", keyField="handle", keyValue=$(../handle))
                    }
                }
            }

            string parent (label="of", font-size=11px, width=7%) = ""
            tags after (label="after", vocabulary="/project/Items, /project/History", width=8%) = ""
            tags labels (label="tags", vocabulary="/project/Labels", width=11%) = ""
            tags flags (label="flags", vocabulary="/project/Flags", width=9%) = ""
            int done (label="done", format="trim", precision=0, suffix="%", width=6%,
                      hidden=$(kids == 0)) = $(kids == 0 || weight == 0 ? 0 :
                          (/project/Items.filter(|x| x/parent == ../handle)
                                         .map(|x| x/done * x/weight).sum()
                           + finished_weight * 100) / weight)
            int points (label="pts", format="trim", width=5%) = 1
            enum complexity (label="think", vocabulary="/project/Complexities", width=7%) = "unassigned"

            button note (icon="note", margin=0, width=3%,
                         font-color=$(../commentary == "" ? "#6b7280" : "inherit")) {
                on click {
                    start_editing (path="../commentary")
                }
            }

            timestamp moved_at (label="moved", mode="elapsed", font-size=11px, width=7%) =
                $(/project/History.filter(|x| x/parent == ../handle).count() == 0 ? ../added :
                  /project/History.filter(|x| x/parent == ../handle)
                                  .map(|x| x/finished_at).max())

            button drop (icon="cross", margin=0, width=3%, hidden=$(open_kids > 0)) {
                on click {
                    remove (from="/project/Items", keyField="handle", keyValue=$(../handle))
                }
            }

            string commentary (label="", font-size=11px, span="row",
                               hidden=$(commentary == "")) = ""

            int kids (hidden=true) =
                $(/project/Items.filter(|x| x/parent == ../handle).count()
                  + /project/History.filter(|x| x/parent == ../handle && x/status != "cancelled").count())
            int open_kids (hidden=true) =
                $(/project/Items.filter(|x| x/parent == ../handle).count())
            float finished_weight (hidden=true) =
                $(/project/History.filter(|x| x/parent == ../handle && x/status != "cancelled")
                                  .map(|x| x/points).sum())
            float weight (hidden=true) = $(../kids == 0 ? ../points :
                /project/Items.filter(|x| x/parent == ../handle).map(|x| x/weight).sum()
                + ../finished_weight)
            int waiting (hidden=true) =
                $(../after.filter(|h| /project/Items.filter(|x| x/handle == h).count() > 0).count())
            string colour (hidden=true) =
                $(/project/Stages.filter(|s| s/tag == ../stage).map(|s| s/colour).first())
        }

        div Finished (layout="vertical", margin=0, spacing=2) {
            div (layout="horizontal", margin=0, spacing=6, padding=2, alignment="center") {
                timestamp finished_at (format="datetime", precision="minutes", width=20%) =
                    "2026-01-01T00:00:00Z"
                timestamp added (hidden=true) = "2026-01-01T00:00:00Z"
                string title (width=30%) = ""
                enum status (label="", vocabulary="/project/Statuses", width=10%) = "finished"
                string handle (hidden=true) = ""
                string parent (hidden=true) = ""
                tags after (hidden=true) = ""
                tags labels (label="", vocabulary="/project/Labels", width=21%) = ""
                tags flags (hidden=true) = ""
                int points (label="", format="trim", width=6%) = 0
                enum complexity (label="", vocabulary="/project/Complexities", width=7%) = "unassigned"

                button note (icon="note", margin=0, width=3%,
                             font-color=$(../commentary == "" ? "#6b7280" : "inherit")) {
                    on click {
                        start_editing (path="../commentary")
                    }
                }

                // Back on the open list at filed, with what the record kept. Hidden while the
                // parent is closed too, so a closed item never has an open part: reopen the
                // parent first. The task history keeps the record of the earlier finish.
                button reopen (icon="arrow-up", margin=0, width=3%,
                               hidden=$(/project/History.filter(|x| x/handle == ../parent).count() > 0)) {
                    on click {
                        if (cond=$(/project/Items.filter(|x| x/handle == ../handle).count() == 0)) {
                            append (list="/project/Items") {
                                - added = $(../added)
                                - handle = $(../handle)
                                - title = $(../title)
                                - parent = $(../parent)
                                - after = $(../after)
                                - labels = $(../labels)
                                - flags = $(../flags)
                                - points = $(../points)
                                - commentary = $(../commentary)
                            }
                            remove (from="/project/History", keyField="handle", keyValue=$(../handle))
                        }
                    }
                }
            }
            string commentary (label="", font-size=11px, font-color="#9ca3af", hidden=true) = ""
            string colour (hidden=true) =
                $(/project/Statuses.filter(|s| s/tag == ../status).map(|s| s/colour).first())
        }
    }

    // The plan in two tabs - what is still to do, and what was finished or cancelled - so the
    // closed items are a tap away rather than a scroll past the open ones. Unnamed, so they are
    // no step of any address: the lists are /project/Items and /project/History as they always
    // were, to the formulas here, the bot and the sweep alike.
    tab (label="Open", hover-text="What is still to do") {
        div (layout="horizontal", margin=0, spacing=10, alignment="center") {
            filter (target="/project/Items", text="title, commentary, handle", enum="stage", tags="labels, flags",
                    status="done", hide="waiting", vocabulary="/project/Labels", label="find", width=100%) { }
        }

        div NewTask (layout="horizontal", margin=0, spacing=6, alignment="center") {
            textbox handle (label="", placeholder="id", width=10%) = $(fresh_key(/project/Items, /project/History))
            textbox title (label="", placeholder="what needs doing", width=26%) = ""
            textbox parent (label="", placeholder="under", width=10%) = ""
            textbox after (label="", placeholder="after", vocabulary="/project/Items, /project/History",
                           width=12%) = ""
            textbox labels (label="", placeholder="tags", vocabulary="/project/Labels", width=12%) = ""
            textbox flags (label="", placeholder="flags", vocabulary="/project/Flags", width=10%) = ""
            textbox points (label="", placeholder="pts", width=7%) = ""
            button add (label="+ task", margin=0, width=12%) {
                on click {
                    if (cond=$(../handle != "" && /project/Items.filter(|x| x/handle == ../handle).count() == 0 && /project/History.filter(|x| x/handle == ../handle).count() == 0)) {
                        append (list="/project/Items", from="..") {
                            - added = $(now())
                        }
                    }
                }
            }
        }

        list Items (entry=<Item>, key="handle", layout="vertical", spacing=1, view="table", header=true, sticky=true, lines="vertical", hover-text=true, sort_by=$(|x| 0 - millis_since_epoch(x/added))) {
            - {
                - added = "2026-09-01T09:00:00+04:00"
                - handle = "editor"
                - title = "Rewrite the document editor"
                - labels = "feature"
                - points = 3
            }
            - {
                - added = "2026-09-01T09:05:00+04:00"
                - handle = "parser"
                - title = "Parser mishandles nested blocks"
                - parent = "editor"
                - labels = "bug"
                - points = 2
            }
            - {
                - added = "2026-09-01T09:10:00+04:00"
                - handle = "brace"
                - title = "A body after a value closes the block early"
                - parent = "parser"
                - labels = "bug"
                - points = 2
                - commentary = "only when the value comes first; a body on its own is fine"
            }
            - {
                - added = "2026-09-02T10:00:00+04:00"
                - handle = "rows"
                - title = "Tighten the row layout"
                - parent = "editor"
                - after = "escape"
                - labels = "ui"
                - points = 4
            }
            - {
                - added = "2026-09-02T10:30:00+04:00"
                - handle = "undo"
                - title = "Undo survives a repaint"
                - stage = "later"
                - parent = "editor"
                - labels = "bug"
                - points = 5
            }
            - {
                - added = "2026-09-03T11:00:00+04:00"
                - handle = "docs"
                - title = "Write the guide"
                - labels = "docs"
                - points = 2
            }
            - {
                - added = "2026-09-04T16:00:00+04:00"
                - handle = "favicon"
                - title = "Pick a favicon"
                - stage = "later"
                - after = "rows"
                - labels = "ui"
                - points = 1
                - commentary = "stands on its own; nothing is part of it and it is part of nothing"
            }
        }

        div (layout="horizontal", margin=0, spacing=16, alignment="center") {
            int open_now (label="open") = $(/project/Items.count())
            int points_open (label="points outstanding") = $(/project/Items.map(|x| x/points).sum())
            int points_won (label="points finished") =
                $(/project/History.filter(|x| x/status != "cancelled").map(|x| x/points).sum())
        }

        text labels_header (markdown=true) = "## Tags"

        list Labels (entry=<Label>, key="tag", layout="vertical", spacing=1) {
            - {
                - tag = "bug"
                - name = "bug"
                - colour = "#d73a4a"
            }
            - {
                - tag = "feature"
                - name = "feature"
                - colour = "#0e8a16"
            }
            - {
                - tag = "ui"
                - name = "interface"
                - colour = "#1d76db"
            }
            - {
                - tag = "docs"
                - name = "docs"
                - colour = "#7057ff"
            }
        }

        div NewTag (layout="horizontal", margin=0, spacing=6, alignment="center") {
            textbox tag (label="", placeholder="tag", width=22%) = ""
            textbox name (label="", placeholder="shown as", width=36%) = ""
            textbox colour (label="", placeholder="#rrggbb", width=22%) = ""
            button add (label="+ tag", margin=0, width=12%) {
                on click {
                    if (cond=$(../tag != "" && /project/Labels.filter(|x| x/tag == ../tag).count() == 0)) {
                        append (list="/project/Labels", from="..")
                    }
                }
            }
        }
    }

    tab (label="Closed", hover-text="Finished and cancelled items, newest first") {
        list History (entry=<Finished>, key="handle", layout="vertical", spacing=1, sort_by=$(|x| 0 - millis_since_epoch(x/finished_at))) {
            - {
                - finished_at = "2026-09-05T17:20:00+04:00"
                - added = "2026-09-01T09:20:00+04:00"
                - title = "Escape quotes and newlines when saving"
                - handle = "escape"
                - parent = "editor"
                - labels = "bug"
                - points = 5
            }
            - {
                - finished_at = "2026-09-09T09:30:00+04:00"
                - added = "2026-09-03T11:05:00+04:00"
                - title = "Document the filter"
                - handle = "gfilter"
                - parent = "docs"
                - labels = "docs"
                - points = 2
            }
            - {
                - finished_at = "2026-09-09T11:00:00+04:00"
                - added = "2026-09-01T09:15:00+04:00"
                - title = "Round-trip every real document in a test"
                - handle = "roundtrip"
                - parent = "parser"
                - labels = "bug, docs"
                - points = 6
            }
            - {
                - finished_at = "2026-09-10T15:10:00+04:00"
                - added = "2026-09-03T11:10:00+04:00"
                - title = "Document how a task becomes a larger one"
                - handle = "gparent"
                - parent = "docs"
                - labels = "docs"
                - points = 1
            }
        }
    }
}
