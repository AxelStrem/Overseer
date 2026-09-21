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
        // Only larger tasks are tinted, since a leaf still on this list reads nought - which is
        // useful in itself: the colour picks out the goals out of the work.
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
            // The widths come to about a hundred with `commentary` counted, and `commentary`
            // hides itself when empty - which is most rows - so its share goes to the title.
            //
            // `id` and `of` are the only two that carry a label, and they say so: `layout`
            // on a field decides where its label goes, and inside a row like this one the
            // default is above the value, which would make every row two lines tall for the
            // sake of four characters. They are wider than the value alone would need
            // because the label is now sharing the width with it.

            // What a child names when it says this is its parent. Short, because it is typed
            // by hand: the list is keyed by `added`, which is stable for addressing and no use
            // at all for a person to type.
            //
            // Nothing enforces that these are unique or that the tree has no loops. Two tasks
            // sharing a handle are treated as one by anything counting children; two naming
            // each other read as an error. Both are assumed not to happen.
            string handle (label="id", font-size=11px, width=8%) = ""

            string title (label="title", font-size=14px, width=28%) = ""

            // Which larger task this is part of, by that task's handle. Empty for a task that
            // stands on its own, which is most of them.
            //
            // Typed rather than picked. A task is a row in a list as a tag is, but the picker
            // built for tags chooses several from a fixed vocabulary and this wants exactly
            // one, so it is a plain handle until a single-select picker exists.
            string parent (label="of", font-size=11px, width=7%) = ""

            // The tags, as chips. `vocabulary` is what makes them chips rather than a line of
            // text: it names the list below, and each chip takes its colour and its display
            // name from there.
            tags labels (label="tags", vocabulary="/project/Labels", width=18%) = ""

            // How far along this one is, worked out rather than typed. Only shown for a task
            // with children, because only such a task has anything to work it out from.
            //
            // A leaf is done or it is not, and an open leaf is not: a leaf that is finished has
            // left `Items` for `History`, so everything still on this list reads nought. There
            // is nothing to type and no half-finished leaf. Work part-way through a task is
            // recorded by splitting it - the piece that is done becomes a child with its share
            // of the points and gets finished - which credits the same points on the same day
            // and says what was done as well as how much. The percentage is then a reading of
            // the tree rather than a second account of it kept by hand.
            //
            // A task with children is worth the sum of theirs and reads their progress weighted
            // by it, so a task of one ten-pointer and three one-pointers does not read 75% with
            // the real work untouched. Children already finished are counted from `History` at
            // full weight, or a task would fall back towards nought as they were completed.
            //
            // `weight == 0` happens only if everything under a task has been set to nought
            // points by hand. It reads 0% rather than dividing by it, because that is a hard
            // error and a row saying `invalid formula error` says nothing useful.
            // `precision=0` because the arithmetic is fractional and the answer is not: a task
            // made of a five-pointer and a three-pointer lands on 58.333333333333336 and wants
            // to read 58%.
            int done (label="done", format="trim", precision=0, suffix="%", width=7%,
                      hidden=$(kids == 0)) = $(kids == 0 || weight == 0 ? 0 :
                          (/project/Items.filter(|x| x/parent == ../handle)
                                         .map(|x| x/done * x/weight).sum()
                           + finished_weight * 100) / weight)

            // What finishing this is worth.
            //
            // On a leaf, the work itself. On a task with children, a bonus on top of everything
            // already earned beneath it, paid when `finish` is pressed - which is what makes
            // closing a goal worth doing rather than a formality.
            //
            // One rather than nought by default: a task all of whose children are worth nothing
            // has no weight to divide by, so this way a tree cannot be typed into an error on
            // the way to being filled in.
            int points (label="pts", format="trim", width=6%) = 1

            // Finished. The same two actions as the task manager's done button, and for the
            // same reason: nothing is edited into place, so History is a record of what
            // happened rather than of what the list looks like now.
            //
            // One button for both kinds of task, because there is only one thing to do: record
            // it as finished and take its points. What differs is when it is offered - hidden
            // while a task still has children *open*, rather than while it has children at all.
            //
            // So a leaf always offers it; a task with work outstanding beneath it does not; and
            // a task whose children are all finished does, and then sits there unfinished until
            // it is pressed. That last state is the useful one: everything under a goal is done
            // and the goal is still open, so more can be put under it instead of closing it.
            //
            // `open_kids` rather than "done reads 100" on purpose. A child still on the list has
            // to be finished before its parent can be, or the parent would leave it naming
            // something no longer there and nothing would say so. Hidden keeps that out of the
            // way rather than making it impossible: anything addressing the button directly can
            // still press it.
            button finish (label="finish", margin=0, width=9%,
                           hidden=$(open_kids > 0)) {
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

            // How many tasks name this one, in either list. Nought means a leaf, which is
            // what decides whether a percentage is shown at all. History is counted too, so a
            // task does not turn back into a leaf when the last of its children is finished.
            int kids (hidden=true) =
                $(/project/Items.filter(|x| x/parent == ../handle).count()
                  + /project/History.filter(|x| x/parent == ../handle).count())

            // Of those, the ones still open. What `finish` waits for.
            int open_kids (hidden=true) =
                $(/project/Items.filter(|x| x/parent == ../handle).count())

            // The points already finished under this one.
            float finished_weight (hidden=true) =
                $(/project/History.filter(|x| x/parent == ../handle).map(|x| x/points).sum())

            // What this task is worth: its own points if it is a leaf, otherwise everything
            // under it. `done` divides by this rather than adding the same numbers up again -
            // see the note on the template above, it is worth three times the speed.
            float weight (hidden=true) = $(../kids == 0 ? ../points :
                /project/Items.filter(|x| x/parent == ../handle).map(|x| x/weight).sum()
                + ../finished_weight)

            // When this last moved, read rather than stamped.
            //
            // For a task with finished children, the most recent of those finishes. For a leaf,
            // when it was added, which is the same question asked of something that has not
            // moved at all - and a leaf reading three weeks is exactly the signal wanted.
            // Elapsed, because "eleven days ago" is the thing worth reading.
            //
            // This was stamped by the `+25%` button, which made it true only when the button was
            // remembered. Nothing writes it now, so it cannot be wrong. `max` compares
            // timestamps properly and hands the timestamp back; the count is tested first
            // because the largest of nothing is not a time.
            timestamp moved_at (label="moved", mode="elapsed", font-size=11px, width=12%) =
                $(/project/History.filter(|x| x/parent == ../handle).count() == 0 ? ../added :
                  /project/History.filter(|x| x/parent == ../handle)
                                  .map(|x| x/finished_at).max())

            // Whatever is worth remembering about this one. Hidden until there is something,
            // which is most rows - and its share of the width then goes back to the title.
            string commentary (label="", font-size=11px, span="row",
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
    //
    // `status` reads the same percentage the rows show, so the three boxes are really a question
    // about larger tasks: every leaf here reads "not started", one that had started having been
    // split and one that was finished being in History. Which makes "finished" the useful box -
    // it finds exactly the goals whose work is all done and which are waiting to be closed.
    div (layout="horizontal", margin=0, spacing=10, alignment="center") {

        filter (target="/project/Items", text="title, commentary, handle", tags="labels",
                status="done", vocabulary="/project/Labels", label="find", width=80%) { }

        // A new task, without editing the document or asking the bot.
        //
        // It arrives with nothing but the moment it was added, which is its key, and every
        // other field at whatever the template says - one point in particular, so whatever it
        // is put under has something to divide by before anything else is typed. That is the
        // point of it: the row appears at the top of the list - newest first - and is filled in
        // by typing into it.
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
    // that are not obvious: three levels of task, percentages worked out rather than typed, and
    // finished children still counted from the History below.
    //
    // `editor` reads 50% and `parser` 75%, neither of them typed anywhere. `docs` is the one to
    // look at: both things under it are finished, so it reads 100%, offers `finish` and waits.
    // Its own two points are not paid until that is pressed - and until it is, more work can go
    // under it.
    list Items (entry=<Item>, key="added", layout="vertical", spacing=1, view="table", header=true, sticky=true, lines="vertical", hover-text=true, sort_by=$(|x| 0 - millis_since_epoch(x/added))) {
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
            - labels = "ui"
            - points = 4
        }
        - {
            - added = "2026-09-02T10:30:00+04:00"
            - handle = "undo"
            - title = "Undo survives a repaint"
            - parent = "editor"
            - labels = "bug, later"
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
            - labels = "ui, later"
            - points = 1
            - commentary = "stands on its own; nothing is part of it and it is part of nothing"
        }
    }

    // What is left and what has been earned.
    //
    // `points outstanding` counts every open row, which includes the bonus on a larger task - so
    // it is what is still available rather than what the remaining work is worth. The two differ
    // by the bonuses, and the bonuses are the point of them.
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
    //
    // Kept rather than deleted, because this is where a task's progress comes from: what has
    // been finished under it is counted at full weight, or a task would fall back towards nought
    // as its children were completed. `roundtrip` is three quarters of `parser`; `gfilter` and
    // `gparent` are all of `docs`, which is why `docs` offers its `finish`.
    list History (entry=<Finished>, layout="vertical", spacing=1, sort_by=$(|x| 0 - millis_since_epoch(x/finished_at))) {
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
