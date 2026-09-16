// Overseer's own work: what is planned, how far along it is, and what has been finished.
// -
// A copy of project_template.os with nothing changed but the content - the machinery below is
// the template's, comment for comment, so a fix made there can be brought here by diffing them.
// -
// What this is for: the performance work has reached the point where the next steps are worth
// doing but not all of them are worth doing *now*. tracker_v2 opens in eight seconds where it
// took sixty-seven, which is the difference between unusable and usable, and several of the items
// below would each take another large bite out of what is left. Written down and tagged `later`,
// they can sit there without being forgotten and without being started.
// -
// The tags say what kind of work something is rather than how urgent it is, with one exception:
// `later` means decided-against-for-now. Anything carrying it is a judgement that the current
// behaviour is good enough, not a note that nobody has got round to it.

tab project (label="Overseer", mutable=true) {

    string name (label="", font-size=22px) = "Overseer"

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
        div Item (layout="horizontal", margin=0, spacing=6, padding=2, alignment="center", background-color=$(done >= 75 ? "#14321f" : (done >= 25 ? "#1c2a3a" : "inherit"))) {

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
            int done (label="done", format="trim", precision=0, suffix="%", width=7%, hidden=$(kids == 0)) = $(kids == 0 || weight == 0 ? 0 : (/project/Items.filter(|x| x/parent == ../handle).map(|x| x/done * x/weight).sum() + finished_weight * 100) / weight)

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
            button finish (label="finish", margin=0, width=9%, hidden=$(open_kids > 0)) {
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
            int kids (hidden=true) = $(/project/Items.filter(|x| x/parent == ../handle).count() + /project/History.filter(|x| x/parent == ../handle).count())

            // Of those, the ones still open. What `finish` waits for.
            int open_kids (hidden=true) = $(/project/Items.filter(|x| x/parent == ../handle).count())

            // The points already finished under this one.
            float finished_weight (hidden=true) = $(/project/History.filter(|x| x/parent == ../handle).map(|x| x/points).sum())

            // What this task is worth: its own points if it is a leaf, otherwise everything
            // under it. `done` divides by this rather than adding the same numbers up again -
            // see the note on the template above, it is worth three times the speed.
            float weight (hidden=true) = $(../kids == 0 ? ../points : /project/Items.filter(|x| x/parent == ../handle).map(|x| x/weight).sum() + ../finished_weight)

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
            timestamp moved_at (label="moved", mode="elapsed", font-size=11px, width=12%) = $(/project/History.filter(|x| x/parent == ../handle).count() == 0 ? ../added : /project/History.filter(|x| x/parent == ../handle).map(|x| x/finished_at).max())

            // Whatever is worth remembering about this one. Hidden until there is something,
            // which is most rows - and its share of the width then goes back to the title.
            string commentary (label="", font-size=11px, span="row", hidden=$(commentary == "")) = ""
        }

        // Something finished.
        //
        // Keeps its points and its tags: any figure about what this project got through will be
        // computed from here, and the tags are how "how much of it was interface work" gets
        // answered.
        div Finished (layout="horizontal", margin=0, spacing=6, padding=2, alignment="center") {
            timestamp finished_at (format="datetime", precision="minutes", width=20%) = "2026-01-01T00:00:00Z"
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
    //
    // The tag to reach for here is `later`: it answers "what did we decide was good enough".
    div (layout="horizontal", margin=0, spacing=10, alignment="center") {

        filter (target="/project/Items", text="title, commentary", tags="labels", status="done", vocabulary="/project/Labels", label="find", width=80%) { }

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
    //
    // Three goals, and the work under each. `perf` is the one with history behind it: the
    // dependency graph, the document reaching a fixed point at all, and the list index are all
    // finished under it, which is why it reads a fair way along while everything visible on it
    // is untouched.
    //
    // Parameters are on one line each, unlike the template this was copied from. The serialiser
    // writes them that way, and this document is meant to be used through the interface - so a
    // multi-line parameter list would reformat itself the first time a button was pressed. The
    // template gets away with it by never being saved.
    list Items (entry=<Item>, key="added", layout="vertical", spacing=1, view="table", header=true, sticky=true, lines="vertical", sort_by=$(|x| 0 - millis_since_epoch(x/added))) {
        - {
            - added = "2026-09-16T09:00:00+04:00"
            - handle = "perf"
            - title = "Open the documents fast"
            - labels = "perf"
            - points = 3
            - commentary = "eight seconds on tracker_v2, from sixty-seven; the rest is worth having but no longer urgent"
        }
        - {
            - added = "2026-09-16T09:06:00+04:00"
            - handle = "disk"
            - title = "Keep the graph and the worked-out values on disk"
            - parent = "perf"
            - labels = "perf, infra"
            - points = 5
            - commentary = "the server pays the full cost on every request, so this is where the bot's slowness lives. Key must cover the mounted documents and a semantics version, or it serves stale numbers silently"
        }
        - {
            - added = "2026-09-16T18:30:00+04:00"
            - handle = "hibernate"
            - title = "Put everything on disk and sleep when nothing is asking"
            - parent = "perf"
            - labels = "perf, infra, later"
            - points = 5
            - commentary = "billed by the gigabyte-hour and idle nearly all day, so the cache should cost nothing while nobody is using it. Builds on disk. Three things decide whether it works: freeing has to return the memory to the OS, which is what malloc_trim already does here; the format decides everything, since deserialising 150 MB of JSON would cost more than resolving from scratch and only a compact binary one gets near the second it wants to be; and files have to be evicted by document rather than by text, or every edit the bot makes leaves another 150 MB behind"
        }
        - {
            - added = "2026-09-16T09:07:00+04:00"
            - handle = "prefix"
            - title = "Cache aggregates on the prefixes of a list"
            - parent = "perf"
            - labels = "perf"
            - points = 5
            - commentary = "makes appending O(1), which is what the bot does all day. Only for chains whose lambda reads nothing outside the item - the rest differ per reader"
        }
        - {
            - added = "2026-09-16T09:08:00+04:00"
            - handle = "topo"
            - title = "Work values out in dependency order instead of repeating passes"
            - parent = "perf"
            - labels = "perf"
            - points = 8
            - commentary = "six passes over 20,345 paths, five of them re-deriving a settled document. Needs per-node evaluation pulled out of the recursive walker"
        }
        - {
            - added = "2026-09-16T09:09:00+04:00"
            - handle = "segtree"
            - title = "Cache aggregates on halves of a list"
            - parent = "perf"
            - labels = "perf, later"
            - points = 8
            - commentary = "logarithmic for a change anywhere, not just an append. Right structure, wrong time: at 43 entries it is a 7x win on a component that is now a small share"
        }
        - {
            - added = "2026-09-16T09:10:00+04:00"
            - handle = "monthly"
            - title = "Split the history into a document per month"
            - parent = "perf"
            - labels = "perf, later"
            - points = 5
            - commentary = "the alternative that makes segtree and much of prefix unnecessary"
        }
        - {
            - added = "2026-09-16T09:20:00+04:00"
            - handle = "fix"
            - title = "Put right what is known to be wrong"
            - labels = "correctness"
            - points = 2
        }
        - {
            - added = "2026-09-16T09:21:00+04:00"
            - handle = "drift"
            - title = "Multi-line parameter lists reformat themselves on save"
            - parent = "fix"
            - labels = "correctness, dsl"
            - points = 2
            - commentary = "diary, shopping and blood_pressure all have them; only tracker_v2 is checked byte for byte, so nothing catches it. Either the serialiser keeps the authored shape or the documents go to one line"
        }
        - {
            - added = "2026-09-16T21:00:00+04:00"
            - handle = "onemodlist"
            - title = "The desktop binary keeps its own list of modules"
            - parent = "fix"
            - labels = "correctness"
            - points = 3
            - commentary = "main.rs declares every module again instead of using the library, so a module added to lib.rs and forgotten there compiles everywhere except the app people actually run. A check in the test runner now catches it; having one list would mean it could not happen"
        }
        - {
            - added = "2026-09-17T01:00:00+04:00"
            - handle = "cleanup"
            - title = "Clear the build warnings, and only the ones that are real"
            - parent = "fix"
            - labels = "correctness"
            - points = 2
            - commentary = "21 warnings across the three builds, and only three of them are what they look like: addressing::segment and FormulaEvaluator::resolve_path_to_node_any are genuinely unreachable, and server.rs takes a `headers` it never reads. The other eighteen are the desktop binary calling things unused that the server and the tests use every day - append_entry, name_path, forget_baseline, Graph::values - because main.rs compiles its own copy of every module and needs only some. Deleting those would break the server. Do onemodlist first and most of this goes with it; what is left is three small edits"
        }
        - {
            - added = "2026-09-16T22:30:00+04:00"
            - handle = "viewreach"
            - title = "Formulas and actions cannot reach an entry out of view"
            - parent = "fix"
            - labels = "correctness, dsl, later"
            - points = 5
            - commentary = "only a write names its address in advance, so the bot can log to any day. A formula aggregating across the whole history, or an action addressing an old entry, finds it uninstantiated instead. The answer is to bring it into view when something reaches it and accept the evaluation that costs - which is the honest reading of a window as a display decision rather than a truncation. Bigger than viewnames and includes it"
        }
        - {
            - added = "2026-09-16T20:00:00+04:00"
            - handle = "viewnames"
            - title = "An entry out of view has no name of its own"
            - parent = "fix"
            - labels = "correctness, dsl"
            - points = 2
            - commentary = "a list entry is named DayRecord__4 while it is being instantiated, so one left out of view is still called \"-\" and can only be addressed by key. Fine for the bot, which addresses by date, and a trap for anything addressing by name or position. Naming them in the window pass would cost one format! per entry"
        }
        - {
            - added = "2026-09-17T09:15:00+04:00"
            - handle = "lostlookup"
            - title = "A lookup that finds nothing reads as an error"
            - parent = "fix"
            - labels = "correctness, dsl"
            - points = 2
            - commentary = "a record whose food handle is not in the catalogue resolves to \"invalid formula error\" - three such records are in the history. Not new and not about tags: every field that looks a food up has always done this, and `name` says so where `food` is declared. It reads worse as a chip than as a field, which is what brought it up. Wants fixing where lookups fail, for all of them at once"
        }
        - {
            - added = "2026-09-16T09:22:00+04:00"
            - handle = "reload"
            - title = "Reload starts before the confirm is answered"
            - parent = "fix"
            - labels = "correctness, ui"
            - points = 1
            - commentary = "the discard-unsaved-changes prompt is fired and not awaited"
        }
        - {
            - added = "2026-09-16T09:23:00+04:00"
            - handle = "frozen"
            - title = "Freeze a day's targets when the day is created"
            - parent = "fix"
            - labels = "correctness, dsl"
            - points = 2
            - commentary = "every past day reads today's standing target. Blocked on nested-div overrides doing nothing, so the fields have to leave the targets div first"
        }
        - {
            - added = "2026-09-16T09:24:00+04:00"
            - handle = "margin"
            - title = "padding and margin do nothing on a wrapped value"
            - parent = "fix"
            - labels = "ui, later"
            - points = 3
            - commentary = "182 uses of margin=0 in the documents, which is how long it has been worked around"
        }
        - {
            - added = "2026-09-16T09:25:00+04:00"
            - handle = "unsettled"
            - title = "Three documents never reach a fixed point"
            - parent = "fix"
            - labels = "correctness, later"
            - points = 3
            - commentary = "listed in KNOWN_UNSETTLED so the settling test can pass while they are wrong"
        }
        - {
            - added = "2026-09-17T09:00:00+04:00"
            - handle = "food"
            - title = "Know more about what is eaten than its macros"
            - labels = "dsl"
            - points = 2
            - commentary = "the catalogue is the place to say things about a food once; the tracker reads them for free"
        }
        - {
            - added = "2026-09-17T09:30:00+04:00"
            - handle = "catalogdata"
            - title = "Finish classifying the catalogue"
            - parent = "food"
            - labels = "dsl"
            - points = 2
            - commentary = "seventeen foods left untagged because the answer was a guess - which pizza, which fries, what is in the Haribo. The caffeine gap the tags made visible is closed: a latte is 33 mg per 100 ml, black tea 20, Coke Zero 9.6, the energy drink 32, and the tags followed. The instant coffee was checked and left alone: 333 mg per 100 g over an 18 g sachet is 60 mg a portion, which is right for a 3-in-1 - the 100 mg it was thought to be would be unusually strong for one"
        }
        - {
            - added = "2026-09-16T09:30:00+04:00"
            - handle = "ui"
            - title = "Make the lists easier to read"
            - labels = "ui"
            - points = 1
        }
        - {
            - added = "2026-09-16T22:35:00+04:00"
            - handle = "pages"
            - title = "A way to ask a windowed list for more"
            - parent = "ui"
            - labels = "ui, dsl, later"
            - points = 3
            - commentary = "the list already says how many it left out; this turns that line into something you can press. Wants a per-reader window rather than the one the document states, so asking for more does not edit the file - and it needs viewreach first, or a page brought into view would still be uninstantiated"
        }
        - {
            - added = "2026-09-16T09:31:00+04:00"
            - handle = "zebra"
            - title = "Alternate the row background"
            - parent = "ui"
            - labels = "ui, later"
            - points = 2
            - commentary = "postponed when the table view was built; the modulo operator it needs was added anyway"
        }
        - {
            - added = "2026-09-16T09:32:00+04:00"
            - handle = "polish"
            - title = "General interface polish"
            - parent = "ui"
            - labels = "ui, later"
            - points = 3
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
    //
    // `later` is the odd one out and deliberately so. The others say what kind of work something
    // is; this one says a decision was taken not to do it yet.
    list Labels (entry=<Label>, key="tag", layout="vertical", spacing=1) {
        - {
            - tag = "perf"
            - name = "speed"
            - colour = "#0e8a16"
        }
        - {
            - tag = "correctness"
            - name = "wrong"
            - colour = "#d73a4a"
        }
        - {
            - tag = "dsl"
            - name = "language"
            - colour = "#7057ff"
        }
        - {
            - tag = "infra"
            - name = "deployment"
            - colour = "#0052cc"
        }
        - {
            - tag = "ui"
            - name = "interface"
            - colour = "#1d76db"
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
    // as its children were completed.
    list History (entry=<Finished>, layout="vertical", spacing=1, sort_by=$(|x| 0 - millis_since_epoch(x/finished_at))) {
        - {
            - finished_at = "2026-09-14T18:00:00+04:00"
            - added = "2026-09-13T10:00:00+04:00"
            - title = "Make a document reach a fixed point at all"
            - handle = "settle"
            - parent = "perf"
            - labels = "correctness, perf"
            - points = 3
            - commentary = "a fallback computed for a field that states a value, and a failed fallback reading as null where a failed formula reads as an error. Until both went, the pass limit was a budget rather than a limit"
        }
        - {
            - finished_at = "2026-09-15T20:00:00+04:00"
            - added = "2026-09-13T12:00:00+04:00"
            - title = "Record what each value was worked out from"
            - handle = "graph"
            - parent = "perf"
            - labels = "perf"
            - points = 8
            - commentary = "recorded rather than predicted, paths interned as u64. An edit went from 38 seconds to 3, and reopening from 38 to 1"
        }
        - {
            - finished_at = "2026-09-16T14:00:00+04:00"
            - added = "2026-09-16T08:00:00+04:00"
            - title = "Index a list lookup by the value it compares"
            - handle = "index"
            - parent = "perf"
            - labels = "perf"
            - points = 5
            - commentary = "27 million walks of the food catalogue per open. 67 seconds to 8.5, and the server 40 to 6.6. Three filter implementations exist; the first one optimised was the wrong one"
        }
        - {
            - finished_at = "2026-09-16T15:00:00+04:00"
            - added = "2026-09-16T08:30:00+04:00"
            - title = "One unanswerable row lost the whole filter"
            - handle = "filter"
            - parent = "fix"
            - labels = "correctness"
            - points = 2
            - commentary = "the two filter implementations disagreed: one skipped such a row, the other propagated the failure and turned a sum into an error"
        }
        - {
            - finished_at = "2026-09-16T15:30:00+04:00"
            - added = "2026-09-16T08:45:00+04:00"
            - title = "Read the food history newest first"
            - handle = "newest"
            - parent = "ui"
            - labels = "ui"
            - points = 1
            - commentary = "sort_by with a negated instant, the same idiom as four other documents"
        }
        - {
            - finished_at = "2026-09-16T18:00:00+04:00"
            - added = "2026-09-16T09:12:00+04:00"
            - title = "Reopening a document resolved it all again"
            - handle = "reopen"
            - parent = "perf"
            - labels = "perf, correctness"
            - points = 2
            - commentary = "the suspicion was right: one slot for every document, so serving any other evicted it. Through the server, repeat requests went 8.79s to 0.99s. Bounded by bytes now - a resolved tracker_v2 and its graph are 150 MB measured, so counting documents would not have been safe"
        }
        - {
            - finished_at = "2026-09-16T20:30:00+04:00"
            - added = "2026-09-16T09:05:00+04:00"
            - title = "Show three days of history, and evaluate only those"
            - handle = "lazy3"
            - parent = "perf"
            - labels = "perf, dsl"
            - points = 5
            - commentary = "window=3 on a list, applied before templates are resolved rather than before formulas - which is what saved the memory as well as the time: only 12% of what a resolve adds is worked-out values, the rest is a template copied onto every entry. Through the server, 10.3s to 1.8s cold and 8.8s to 1.3s on a repeat, at the 64 MB budget and no extra RAM. The cache entry fell from 241 MB to 45. A write names its address before the document is resolved, so the bot can still log a meal to a day nobody is looking at"
        }
        - {
            - finished_at = "2026-09-16T21:15:00+04:00"
            - added = "2026-09-16T21:00:00+04:00"
            - title = "The desktop app stopped compiling and every test stayed green"
            - handle = "deskcheck"
            - parent = "fix"
            - labels = "correctness"
            - points = 1
            - commentary = "cargo test never builds the desktop binary - it is gated behind required-features - so document_cache going into lib.rs and not into main.rs broke the app with 147 tests passing. npm test now builds it and says so. It started as a cargo check, which was not enough: check stops before linking, and the next break was a link failure it reported as fine. A no-op build costs ten seconds and sees the whole way"
        }
        - {
            - finished_at = "2026-09-16T22:00:00+04:00"
            - added = "2026-09-16T21:45:00+04:00"
            - title = "Days out of view were drawn anyway, as raw text"
            - handle = "viewdraw"
            - parent = "fix"
            - labels = "correctness, ui"
            - points = 1
            - commentary = "the renderer compared the marker against `true` when a parameter arrives tagged as `{Boolean: true}`, so it never matched and all forty-three days were drawn - the forty with no template copied onto them showing food handles as unformatted text. The test passed because its fixture was written to match the code rather than the resolver, so both sides held the same mistake. Fixtures now carry the shape the resolver actually sends, and the test fails against the old check"
        }
        - {
            - finished_at = "2026-09-16T23:30:00+04:00"
            - added = "2026-09-16T09:11:00+04:00"
            - title = "Turn the graph on everywhere, desktop and server alike"
            - handle = "desktop"
            - parent = "perf"
            - labels = "perf, infra"
            - points = 1
            - commentary = "on by default in both builds now, OVERSEER_DEPENDENCY_GRAPH=0 to turn it off. Measured either way round so neither got a warm mount cache: recording costs a quarter of a first open, 1.31s against 1.05, and buys a second open at 0.16s. It also uncovered a real fault - a document held partly out of view was being served from the cache to a write that needed it in view, so the bot could not log to an old day whenever the cache happened to be warm. Such a resolve now neither reads the cache nor writes to it"
        }
        - {
            - finished_at = "2026-09-17T00:15:00+04:00"
            - added = "2026-09-17T00:00:00+04:00"
            - title = "The app would not link, and the guard said it was fine"
            - handle = "linkguard"
            - parent = "fix"
            - labels = "correctness"
            - points = 1
            - commentary = "unresolved anon.<hash>.llvm symbols in addressing::segments_for and dependencies::WorkingOut::value - both newly reachable from the binary, both left with stale incremental artifacts from when they were not. Not our code: cargo clean -p overseer fixed it and both dev and release link. The blind spot was ours, and the guard builds rather than checks now"
        }
        - {
            - finished_at = "2026-09-17T11:00:00+04:00"
            - added = "2026-09-17T09:00:00+04:00"
            - title = "A food says what it is, and every meal that ate it says so too"
            - handle = "foodtags"
            - parent = "food"
            - labels = "dsl, ui"
            - points = 5
            - commentary = "no new machinery needed - a Labels list in foods.os, a tags field on Food, a second mount for the vocabulary, and the same catalogue lookup the macros already use. 120 of 137 foods tagged; alcohol and caffeine taken from the figures in per_100g rather than decided again, with a test that stops the two drifting. Not editable on the meal, because the tags belong to the food. Cost 0.25s on a tracker_v2 open, 1.69 to 1.94"
        }
        - {
            - finished_at = "2026-09-17T13:00:00+04:00"
            - added = "2026-09-17T12:00:00+04:00"
            - title = "Nothing was checking that the catalogue survives a save"
            - handle = "catalogguard"
            - parent = "fix"
            - labels = "correctness"
            - points = 1
            - commentary = "the file the bot edits most had no round-trip test, and adding a field to every food is exactly the change that needs one: the serialiser writes children in template order, so a line put in the wrong place moves in all 137 entries on the first save. Mine was in the wrong place. The guard also found a closing brace at four spaces where every other entry uses eight - there since before any of this, and invisible until something saved"
        }
    }
}
