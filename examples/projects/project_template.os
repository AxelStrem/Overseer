// A project's work: what is outstanding, how far along it is, and what has been finished.
// -
// A template to copy per project. Nothing here names a project, so a copy needs only its `name`
// changed and its tag list adjusted to whatever that project actually has.
// -
// Deliberately not the task manager. There are no rules and nothing appears by itself: a
// project's work arrives because someone found it, not because a calendar came round. What it
// borrows is `points`, so that finishing something here can later be recorded as done in
// tasks.os on the day it happened - the plan being that project work reaches the daily recap
// and the performance figures without the open items crowding the chore list.
// -
// Built for many more open items than the task manager holds, so the rows are tight: no shadow,
// almost no padding, one line each. What makes a hundred rows navigable is the tag list, which
// is why the tags are chips rather than prose.

tab project (label="Project", mutable=true) {

    string name (label="", font-size=22px) = "Untitled project"

    div (hidden=true) {

        // A tag that exists in this project.
        //
        // The vocabulary lives in the document rather than in the renderer, so each copy can
        // have its own: a colour and a display name, keyed by the short handle that a task's
        // `tags` field actually stores. Renaming one here renames it everywhere it is used.
        // Removing one leaves any task still holding it showing the bare handle, marked as no
        // longer listed - which is the honest thing, since the task really does still hold it.
        div Label (layout="horizontal", margin=0, spacing=6, alignment="center") {
            string tag (label="", width=25%) = ""
            string name (label="", width=45%) = ""
            string colour (label="", width=30%) = "#6b7280"
        }

        // Something to do, or something wrong - at any size.
        //
        // There is one list and no separate notion of a larger task. A task becomes one by
        // having others name it as their `parent`, and what it has got through is worked out
        // from them rather than typed. That keeps the filter, the sort and "everything open"
        // working over the whole tree, which nesting would have broken, and re-parenting is
        // editing one field rather than moving an entry between lists.
        //
        // Three things are worth knowing about the rollup, all of them measured:
        //
        //   - It goes three hops deep. A leaf under a task under a task under a task resolves;
        //     one level further and the root reads `invalid formula error` rather than warning.
        //   - A loop - two tasks naming each other - is an `invalid formula error` on those two
        //     rows in a few milliseconds. It does not hang, so a typo costs a wrong-looking row
        //     and nothing worse.
        //   - `done` divides by `weight` rather than filtering the list a second time to add the
        //     same numbers up again. That one change took a hundred and twenty items from 789ms
        //     to 237ms a resolve, and a button press from two seconds to 700ms. Written the
        //     obvious way, this document is slow enough to be unpleasant.
        //
        // Tinted by how far along it is, so a column of a hundred reads before it is read.
        // A bar would say it better, but a bar needs a width of `$(done + "%")` and there is no
        // string concatenation in the formula language - a bare number is read as pixels.
        div Item (layout="horizontal", margin=0, spacing=6, padding=2, alignment="center",
                  background-color=$(done >= 75 ? "#14321f" :
                                    (done >= 25 ? "#1c2a3a" : "inherit"))) {

            timestamp added (hidden=true) = "2026-01-01T00:00:00Z"

            // One row per task, and everything on it.
            //
            // This was a vertical stack holding one horizontal strip, which meant `id`,
            // `moved` and `commentary` each took a line of their own and a task stood four
            // lines tall. For a list meant to hold a hundred of them that is most of the
            // screen spent on three short fields.
            //
            // The widths add to a hundred with `commentary` counted, and `commentary` hides
            // itself when empty - which is most rows - so its share goes back to the title.

            // What a child names when it says this is its parent. Short, because it is typed
            // by hand: the list is keyed by `added`, which is stable for addressing and no use
            // at all for a person to type.
            //
            // Nothing enforces that these are unique or that the tree has no loops. Two tasks
            // sharing a handle are treated as one by anything counting children; two naming
            // each other read as an error. Both are assumed not to happen.
            string handle (label="id", font-size=11px, width=6%) = ""

            string title (label="", font-size=14px, width=24%) = ""

            // Which larger task this is part of, by that task's handle. Empty for a task that
            // stands on its own, which is most of them.
            //
            // Typed rather than picked. A task is a row in a list as a tag is, but the picker
            // built for tags chooses several from a fixed vocabulary and this wants exactly
            // one, so it is a plain handle until a single-select picker exists.
            string parent (label="of", font-size=11px, width=6%) = ""

            // The tags, as chips. `vocabulary` is what makes them chips rather than a line of
            // text: it names the list below, and each chip takes its colour and its display
            // name from there.
            tags labels (label="", vocabulary="/project/Labels", width=16%) = ""

            // Nought to a hundred, typed freely; the button beside it is what stamps
            // `moved_at`. Only read for a task with nothing under it - a task with children
            // takes its figure from them, and typing here would be overruled.
            //
            // A handler on this field would be better, so that any edit stamped it. The parser
            // now allows one - `int done (...) = 0 { on change { ... } }` reads and writes back
            // correctly - but not yet inside a list entry: an entry records an override as
            // `- done = 37`, and the machinery that writes those drops a field that has
            // children, so a typed percentage would read back as whatever the template
            // declares. Until that is sorted, the stamp records progress made by the button.
            int own (label="", format="trim", suffix="%", width=5%,
                     hidden=$(kids > 0)) = 0

            // What this task has got through, and what it is worth.
            //
            // A task with nothing under it is worth its own points and reads its own
            // figure. A task with children is worth the sum of theirs and reads their
            // progress weighted by it - so a task of one ten-pointer and three one-pointers
            // does not read 75% with the real work untouched.
            //
            // Children finished already have left `Items` for `History`; they are counted
            // there at full weight, or a task would read nought at the moment its last
            // child was done.
            // `precision=0` because the arithmetic is fractional and the answer is not:
            // a task made of a five-pointer and a three-pointer lands on 58.333333333333336
            // and wants to read 58%.
            int done (label="", format="trim", precision=0, suffix="%", width=5%,
                      hidden=$(kids == 0)) = $(kids == 0 ? own :
                          (/project/Items.filter(|x| x/parent == ../handle)
                                         .map(|x| x/done * x/weight).sum()
                           + finished_weight * 100) / weight)

            // A quarter at a time, stamping as it goes. A button rather than a note to
            // remember: a stamp that can be forgotten is a field that lies, and "this has
            // not moved in three weeks" is the whole reason the stamp exists.
            button advance (label="+25%", margin=0, width=6%,
                            hidden=$(kids > 0)) {
                on click {
                    set (path="../own", mode="value") =
                        $(../own + 25 > 100 ? 100 : ../own + 25)
                    set (path="../moved_at", mode="value") = $(now())
                }
            }

            int points (label="", format="trim", width=4%) = 0

            // Finished. The same two actions as the task manager's done button, and for the
            // same reason: nothing is edited into place, so History is a record of what
            // happened rather than of what the list looks like now.
            // Hidden while anything still names this task, which keeps it out of the way
            // rather than making it impossible - anything addressing the button directly can
            // still press it. Finishing a task with children would leave them naming
            // something no longer in the list, and nothing would say so.
            button finish (label="finish", margin=0, width=8%,
                           hidden=$(kids > 0)) {
                on click {
                    append (list="/project/History") {
                        - finished_at = $(now())
                        - added = $(../added)
                        - handle = $(../handle)
                        - title = $(../title)
                        - parent = $(../parent)
                        - labels = $(../labels)
                        - points = $(../points)
                        - commentary = $(../commentary)
                    }
                    remove (from="/project/Items", keyField="added", keyValue=$(../added))
                }
            }

            // How many tasks name this one, in either list. Nought means a leaf, and a leaf
            // is the only kind of task whose figure is typed rather than worked out.
            int kids (hidden=true) =
                $(/project/Items.filter(|x| x/parent == ../handle).count()
                  + /project/History.filter(|x| x/parent == ../handle).count())

            // The points already finished under this one.
            float finished_weight (hidden=true) =
                $(/project/History.filter(|x| x/parent == ../handle).map(|x| x/points).sum())

            // What this task is worth: its own points if it is a leaf, otherwise everything
            // under it. `done` divides by this rather than adding the same numbers up again -
            // see the note on the template above, it is worth three times the speed.
            float weight (hidden=true) = $(../kids == 0 ? ../points :
                /project/Items.filter(|x| x/parent == ../handle).map(|x| x/weight).sum()
                + ../finished_weight)

            // When the progress last moved. Written by the button above, which is the only
            // thing that touches it, and empty until something happens so a new task does not
            // claim progress it has not made. Elapsed, because "eleven days ago" is what is
            // worth reading.
            timestamp moved_at (label="", mode="elapsed", font-size=11px, width=8%,
                                hidden=$(moved_at == "")) = ""

            // Whatever is worth remembering about this one. Hidden until there is something,
            // which is most rows - and its share of the width then goes back to the title.
            string commentary (label="", font-size=11px, width=18%,
                               hidden=$(commentary == "")) = ""
        }

        // Something finished.
        //
        // Keeps its points and its tags: any figure about what this project got through will be
        // computed from here, and the tags are how "how much of it was interface work" gets
        // answered.
        div Finished (layout="horizontal", margin=0, spacing=6, padding=2, alignment="center") {
            timestamp finished_at (format="datetime", precision="minutes", width=20%) =
                "2026-01-01T00:00:00Z"
            timestamp added (hidden=true) = "2026-01-01T00:00:00Z"
            string title (width=40%) = ""
            // Kept so a task can still count what has been finished under it, and so a finished
            // task's own children can still find it.
            string handle (hidden=true) = ""
            string parent (hidden=true) = ""
            tags labels (label="", vocabulary="/project/Labels", width=24%) = ""
            int points (label="", format="trim", width=6%) = 0
            string commentary (hidden=true) = ""
        }
    }

    text items_header (markdown=true) = "## Open"

    // What to look at, out of everything that is open.
    //
    // A view and nothing more: it writes nothing, and the server never hears about it. `text`
    // names the fields the typed text is matched against - the fields, not what is on screen,
    // because what is on screen is formatted and sometimes hidden. Picking two tags narrows to
    // the things that are both.
    div (layout="horizontal", margin=0, spacing=10, alignment="center") {

        filter (target="/project/Items", text="title, commentary", tags="labels",
                status="done", vocabulary="/project/Labels", label="find", width=80%) { }

        // A new task, without editing the document or asking the bot.
        //
        // It arrives with nothing but the moment it was added, which is its key, and every
        // other field at whatever the template says. That is the point: the row appears at the
        // top of the list - newest first - and is filled in by typing into it.
        button add (label="+ task", margin=0, width=20%) {
            on click {
                append (list="/project/Items") {
                    - added = $(now())
                }
            }
        }
    }

    // Newest first, so the thing just added is where it was put. Nothing here climbs the way a
    // chore does: a project's order is whatever you decide it is, and pretending otherwise
    // would pin the oldest bug permanently to the top.
    // Rows to look at and then delete. A copy of this template starts with them so there is
    // something to press before there is anything real, and they are written to show the parts
    // that are not obvious: three levels of task, a `done` that is worked out rather than typed,
    // and a finished child still counted from History below.
    //
    // `editor` reads 58% and `parser` 75%, neither of them typed anywhere.
    list Items (entry=<Item>, key="added", layout="vertical", spacing=1,
                sort_by=$(|x| 0 - millis_since_epoch(x/added))) {
        - {
            - added = "2026-09-01T09:00:00+04:00"
            - handle = "editor"
            - title = "Rewrite the document editor"
            - labels = "feature"
            - points = 1
        }
        - {
            - added = "2026-09-01T09:05:00+04:00"
            - handle = "parser"
            - parent = "editor"
            - title = "Parser mishandles nested blocks"
            - labels = "bug"
            - points = 1
        }
        - {
            - added = "2026-09-01T09:10:00+04:00"
            - handle = "brace"
            - parent = "parser"
            - title = "A body after a value closes the block early"
            - labels = "bug"
            - own = 60
            - points = 5
            - moved_at = "2026-09-08T18:20:00+04:00"
            - commentary = "only when the value comes first; a body on its own is fine"
        }
        - {
            - added = "2026-09-01T09:15:00+04:00"
            - handle = "roundtrip"
            - parent = "parser"
            - title = "Round-trip every real document in a test"
            - labels = "bug, docs"
            - own = 100
            - points = 3
            - moved_at = "2026-09-09T11:00:00+04:00"
        }
        - {
            - added = "2026-09-02T10:00:00+04:00"
            - handle = "rows"
            - parent = "editor"
            - title = "Tighten the row layout"
            - labels = "ui"
            - points = 3
        }
        - {
            - added = "2026-09-02T10:30:00+04:00"
            - handle = "undo"
            - parent = "editor"
            - title = "Undo survives a repaint"
            - labels = "bug, later"
            - own = 25
            - points = 5
            - moved_at = "2026-09-06T14:40:00+04:00"
        }
        - {
            - added = "2026-09-03T11:00:00+04:00"
            - handle = "docs"
            - title = "Write the guide"
            - labels = "docs"
            - points = 1
        }
        - {
            - added = "2026-09-03T11:05:00+04:00"
            - handle = "gfilter"
            - parent = "docs"
            - title = "Document the filter"
            - labels = "docs"
            - own = 40
            - points = 2
            - moved_at = "2026-09-09T09:30:00+04:00"
        }
        - {
            - added = "2026-09-03T11:10:00+04:00"
            - handle = "gparent"
            - parent = "docs"
            - title = "Document how a task becomes a larger one"
            - labels = "docs"
            - points = 1
        }
        - {
            - added = "2026-09-04T16:00:00+04:00"
            - handle = "favicon"
            - title = "Pick a favicon"
            - labels = "ui, later"
            - points = 1
            - commentary = "stands on its own; nothing is part of it and it is part of nothing"
        }
    }

    div (layout="horizontal", margin=0, spacing=16, alignment="center") {
        int open_now (label="open") = $(/project/Items.count())
        int points_open (label="points outstanding") = $(/project/Items.map(|x| x/points).sum())
        int points_won (label="points finished") = $(/project/History.map(|x| x/points).sum())
    }

    text labels_header (markdown=true) = "## Tags"

    // The vocabulary, editable like anything else. Kept below the work: it is read far less
    // often than it is used.
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
        - {
            - tag = "later"
            - name = "later"
            - colour = "#fbca04"
        }
    }

    text history_header (markdown=true) = "## Finished"

    // Newest first, as every history here is.
    // One finished child, kept here rather than deleted - it is what stops `editor` reading
    // lower than it should. A task counts what has been finished under it at full weight, or a
    // task would fall back towards nought as its children were completed.
    list History (entry=<Finished>, layout="vertical", spacing=1,
                  sort_by=$(|x| 0 - millis_since_epoch(x/finished_at))) {
        - {
            - finished_at = "2026-09-05T17:20:00+04:00"
            - added = "2026-09-01T09:20:00+04:00"
            - handle = "escape"
            - parent = "editor"
            - title = "Escape quotes and newlines when saving"
            - labels = "bug"
            - points = 5
        }
    }
}
