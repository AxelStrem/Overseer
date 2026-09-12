// Tasks: what is open, what made it open, and what has been done.
// -
// Three lists. `Open` is what there is to do; `History` is what has been done; `Rules` is what
// puts things into `Open` without anyone asking.
// -
// Closing a task is a button, the same two actions as buying something off the shopping list:
// it leaves `Open` and lands in `History`. Nothing is ever edited into place, so the history
// is a record of what happened rather than of what the list looks like now.
// -
// A rule decides *whether* it is due; it does not act. The sweep in the bot reads `due` and
// does the appending, because only the bot can tell you a task appeared, and because nothing
// on the server drives a clock. That split is the point: every reason a task did or did not
// show up is a formula in this document, readable in the browser, and the sweep is dumb.
// -
// Priority climbs. A task's `priority` is its base plus `priority_gain` for every day it has
// sat there, so something small and ignorable rises past something big and stalled eventually.
// A gain of 0 means a task that never gets more urgent than it started.

tab tasks (label="Tasks", mutable=true) {

    text header (markdown=true) = "# Tasks"

    div (hidden=true) {

        // Something to do.
        //
        // Coloured by how far its priority has climbed, so the list reads before it is read -
        // and red once it is late, which is the one state worth interrupting the gradient for.
        div Task (layout="vertical", margin=0, spacing=2, padding=8, shadow="soft",
                  background-color=$(overdue ? "#5a1414" :
                                    (priority >= 100 ? "#3a1c1c" :
                                    (priority >= 50 ? "#332d1c" : "inherit")))) {

            timestamp added (hidden=true) = "2026-01-01T00:00:00Z"

            // Which rule made this, empty if a person did. Kept so the rule can see whether
            // its last task is still open, and so history can say what has been recurring.
            string rule (hidden=true) = ""

            // When it has to be done by. Empty means there is no deadline, which is most
            // tasks - a due date is a promise, and one on everything means one on nothing.
            timestamp deadline (hidden=true) = ""
            bool has_deadline (hidden=true) = $(deadline != "")

            // Minutes rather than days, because a deadline is exactly the question `days_since`
            // cannot answer: it truncates toward zero, so three hours late and three hours
            // early both come out 0. Positive here means the moment has passed.
            bool overdue (hidden=true) = $(has_deadline && minutes_since(../deadline) > 0)

            div (layout="horizontal", margin=0, alignment="center") {
                string title (label="", font-size=18px, width=44%) = ""

                // The deadline twice over, because they answer different questions: the date
                // is what you plan around, the countdown is what you feel. `remaining` counts
                // down live and goes negative once it is past, so a late task says how late.
                timestamp due_at (label="", format="datetime", precision="minutes",
                                  font-size=13px, width=14%,
                                  hidden=$(../has_deadline == false)) = $(../deadline)
                timestamp left (label="", mode="remaining", font-size=13px, width=10%,
                                hidden=$(../has_deadline == false)) = $(../deadline)

                int difficulty (label="", format="trim", width=8%) = 1
                int priority (label="", font-size=18px, width=10%) =
                    $(../base_priority + ../priority_gain * days_since(../added))
                button done (label="done", margin=0, width=8%) {
                    on click {
                        append (list="/tasks/History") {
                            - done_at = $(now())
                            - added = $(../added)
                            - rule = $(../rule)
                            - title = $(../title)
                            - difficulty = $(../difficulty)
                            - deadline = $(../deadline)
                            - was_late = $(../overdue)
                        }
                        remove (from="/tasks/Open", keyField="added", keyValue=$(../added))
                    }
                }

                // Closed without being done. The same two actions as `done`, deliberately: a
                // task that was never finished still happened, and a list that remembers only
                // successes cannot answer "how often do I actually manage this?" - which is
                // most of what the history is for.
                //
                // Pressed by a person who knows the day is gone, and by the sweep when a
                // recurring rule comes round again and its last task is still sitting there.
                button fail (label="fail", margin=0, width=8%) {
                    on click {
                        append (list="/tasks/History") {
                            - done_at = $(now())
                            - added = $(../added)
                            - rule = $(../rule)
                            - title = $(../title)
                            - difficulty = $(../difficulty)
                            - deadline = $(../deadline)
                            - was_late = $(../overdue)
                            - failed = true
                        }
                        remove (from="/tasks/Open", keyField="added", keyValue=$(../added))
                    }
                }
            }

            // What the list is ordered by, which is not quite what is shown.
            //
            // An overdue task goes above every task that is not, whatever either of them has
            // climbed to - so the boost has to be larger than any priority can reach, and
            // `priority` itself stays the honest number a person reads.
            float rank (hidden=true) = $(priority + (overdue ? 1000000 : 0))

            // Whether the bot has said anything about this one being late. It tells you once;
            // the flag lives here rather than in the bot because the document is the only
            // thing that survives a restart, and being told twice is worse than being told
            // late.
            bool announced (hidden=true) = false

            // What it climbs from and how fast. Hidden on the card - they belong to the rule
            // that set them, and reading them every time would bury the title.
            int base_priority (hidden=true) = 0
            float priority_gain (hidden=true) = 0

            string description (label="", font-size=13px, hidden=$(description == "")) = ""
        }

        // Something done. Difficulty travels with it: the history is what any statistic about
        // how much was got through will be computed from.
        div Record (layout="horizontal", margin=0, alignment="center") {
            timestamp done_at (format="datetime", width=22%) = "2026-01-01T00:00:00Z"
            timestamp added (hidden=true) = "2026-01-01T00:00:00Z"
            string rule (hidden=true) = ""
            string title (width=50%) = ""

            // Whether it was late when it was finished. Worked out once, at the moment the
            // button was pressed, and stored - the deadline and the doing are both in the
            // past now, and nothing later should be able to change the answer.
            timestamp deadline (hidden=true) = ""
            bool was_late (label="late", width=8%, hidden=$(was_late == false)) = false

            // Closed without being done. `done_at` is really "closed at" on these - the field
            // is shared because a record is a record, and every count that means *completed*
            // filters on this flag rather than on the mere presence of a record.
            bool failed (label="not done", width=10%, hidden=$(failed == false)) = false

            int difficulty (label="", format="trim", width=10%) = 1
        }

        // A reason for a task to appear.
        //
        // `mode` picks which question is asked:
        //   manual    - never fires by itself. A template for the button below.
        //   interval  - a while after the last time `trigger` was completed.
        //   daily     - every day.
        //   weekly    - every <weekday>, 1 is Monday and 7 is Sunday.
        //   monthly   - every month on <day>.
        //   yearly    - every year on <day> of <month>.
        div Rule (layout="vertical", margin=0, spacing=2, padding=8, shadow="soft") {

            string handle (hidden=true) = ""

            div (layout="horizontal", margin=0, alignment="center") {
                string title (label="", font-size=17px, width=38%) = ""
                string mode (label="", width=13%) = "manual"
                int difficulty (label="", format="trim", width=7%) = 1

                // Whether being due matters right now. Shown so a rule that is waiting on
                // something says so, rather than looking broken.
                string state (label="", font-size=13px, width=20%) =
                    $(active == false ? "off" :
                     (due ? "due" :
                     (open_now > 0 ? "still open" :
                     (has_time && time_has_come == false ? "not yet today" : "waiting"))))

                button add (label="add", margin=0, width=14%) {
                    on click {
                        append (list="/tasks/Open") {
                            - added = $(now())
                            - rule = $(../handle)
                            - title = $(../title)
                            - description = $(../description)
                            - difficulty = $(../difficulty)
                            - base_priority = $(../base_priority)
                            - priority_gain = $(../priority_gain)

                            // Worked out here, at the moment the task appears, rather than
                            // stored as an interval and computed later: the deadline is a
                            // fact about this occurrence, and editing the rule afterwards
                            // should not move a promise already made.
                            - deadline = $(../has_due_time ? date_add_hours(now(), ../hours_until_due) :
                                          (../due_in_hours > 0 ? date_add_hours(now(), ../due_in_hours) : ""))
                        }
                        set (path="../last_created", mode="value") = $(now())
                    }
                }
            }

            string description (hidden=$(description == ""), label="", font-size=13px) = ""

            // What the rule is for, and what it is made of.
            //
            // Each setting shows only for the mode that reads it. Hiding all of them keeps the
            // card short at the cost of making a rule unreadable - you cannot see what "every
            // 10 days" means, and neither can the bot, which can write these fields but only
            // reads back what is shown. Hiding the ones that do not apply costs nothing and
            // leaves every rule saying exactly what it does.
            //
            // No widths here on purpose. Percentages would be shared out between whichever
            // fields this mode happens to show, so a weekly rule and an interval rule would
            // put their settings in different places and the column of cards would read as
            // ragged. Sized to their contents, every rule starts its settings where the last
            // one did.
            div (layout="horizontal", margin=0, spacing=16, alignment="center") {

                // Switched off by hand, from here. A rule keeps everything it knows while it
                // is off - its interval, when it last ran - so turning it back on is not the
                // same as making it again. Nothing else reads `active`: it is only ever the
                // first thing `due` asks.
                bool active (label="on") = true

                int base_priority (label="from", format="trim") = 0
                float priority_gain (label="+/day", format="trim") = 0

                int interval_days (label="every (days)", format="trim",
                                   hidden=$(../mode != "interval")) = 7
                string after (label="after", hidden=$(../mode != "interval")) = ""

                int weekday (label="weekday", format="trim",
                             hidden=$(../mode != "weekly")) = 1
                int day (label="day", format="trim",
                         hidden=$(../mode != "monthly" && ../mode != "yearly")) = 1
                int month (label="month", format="trim",
                           hidden=$(../mode != "yearly")) = 1

                // What time of day it should appear. 0:00 means any time, which is what every
                // rule did before there was a choice, and stays the default.
                //
                // Two integers rather than one timestamp, deliberately. A timestamp would show
                // as "07:30" and read back through `minutes_of_day` - but it carries an
                // offset, and whatever writes it has to get that offset right. The bot writes
                // these fields, and a zone guessed wrong fires the rule four hours out with
                // nothing on the card to say why. 7 and 30 cannot be wrong about a zone.
                int at_hour (label="at", format="trim") = 0
                int at_minute (label=":", format="trim") = 0

                // How long the task gets, once it appears. 0 means no deadline, which is the
                // default and should stay the default: a rule that puts a deadline on
                // something every day teaches you to ignore deadlines.
                //
                // Hours rather than days, and fractional, so "by the end of the day" and
                // "within half an hour" are both sayable without a second field.
                float due_in_hours (label="due in (h)", format="trim") = 0

                // The same promise as a clock time rather than a length, and it wins over
                // `due_in_hours` when both are set. "Due by nine" survives the container being
                // down until half past eight; "due in an hour and a half" quietly becomes ten.
                int due_hour (label="due at", format="trim") = 0
                int due_minute (label=":", format="trim") = 0

                // What to do when this rule comes round and its last task is still open.
                //
                //   skip - leave it be; this occasion passes. The dishwasher does not need
                //          running twice because nobody emptied it.
                //   add  - open another alongside. Two of the same thing at once is a
                //          sensible thing to see for some rules and nonsense for most.
                //   fail - close the open one as not done, and open a fresh one. For the
                //          things where yesterday's occasion is genuinely gone: the pills you
                //          did not take, the practice you did not do. Doing it today is not
                //          doing it yesterday, and a history that says otherwise is wrong.
                //
                // Was a boolean, `allow_duplicates`, which could hold only the first two. The
                // third is not a variation on either: it is the only one that closes a task.
                string when_open (label="if open") = "skip"

                // When it last put something in the list. Shown because "why has this not
                // fired" is nearly always answered by it - and hidden until there is one,
                // because the epoch default renders as "20676d 17h" rather than as "never".
                timestamp last_created (label="last", mode="elapsed",
                                        hidden=$(../ever_ran == false)) =
                    "1970-01-01T00:00:00Z"
            }

            // Whose completion starts an interval rule's countdown: its own, unless it is
            // waiting on another rule. That is what makes "wash the floors every 10 days after
            // they were last washed" and "water the plants 3 days after the floors were washed"
            // the same rule with a different word in one field.
            string trigger (hidden=true) = $(after == "" ? ../handle : ../after)

            // How many of this rule's tasks are open, and how long ago its trigger was last
            // completed.
            //
            // The second is the smallest days-ago among the trigger's completions, because
            // the smallest days-ago is the most recent one - `max` over timestamps would say
            // the same thing if timestamps compared, and this needs only numbers.
            int open_now (hidden=true) = $(/tasks/Open.filter(|t| t/rule == ../handle).count())
            //
            // Both count what was *finished*. A task closed as not done is in the history too -
            // that is the point of recording it - but it must never start an interval's
            // countdown: "wash the floors every 10 days" would otherwise be satisfied by ten
            // days of not washing them.
            int done_count (hidden=true) =
                $(/tasks/History.filter(|h| h/rule == ../trigger && h/failed == false).count())
            float since_done (hidden=true) = $(done_count == 0 ? 0 :
                /tasks/History.filter(|h| h/rule == ../trigger && h/failed == false)
                              .map(|h| days_since(h/done_at)).min())

            // Has this rule already fired for the occasion that is current now?
            //
            // For calendar rules the occasion is a day, so "not today" is the whole test. For
            // interval rules the occasion is a completion, and the rule has fired for it
            // already if it created something *after* that completion happened - which, in
            // days-ago, means a smaller number than the completion's.
            bool fresh_today (hidden=true) = $(same_day(../last_created, now()) == false)
            bool fresh_since_done (hidden=true) = $(days_since(../last_created) > ../since_done)

            // Whether this rule has ever put anything in the list. The epoch default is how a
            // rule says "never", and 3650 days is far enough from any real one to be safe.
            bool ever_ran (hidden=true) = $(days_since(../last_created) < 3650)

            // A rule that has never run and never been completed fires once, straight away,
            // rather than waiting out an interval that has nothing to count from.
            bool never_ran (hidden=true) = $(ever_ran == false && done_count == 0)

            // The clock, and what this rule wants of it.
            //
            // `at_minutes` of 0 is the whole of "any time": the gate below is `>=`, so a rule
            // that names no hour passes it from midnight and behaves exactly as it always did.
            int at_minutes (hidden=true) = $(at_hour * 60 + at_minute)
            int due_minutes (hidden=true) = $(due_hour * 60 + due_minute)
            int clock_now (hidden=true) = $(minutes_of_day(now()))
            bool has_time (hidden=true) = $(at_minutes > 0)
            bool has_due_time (hidden=true) = $(due_minutes > 0)
            bool time_has_come (hidden=true) = $(clock_now >= at_minutes)

            // How long this occurrence gets, when the deadline is a time rather than a length.
            //
            // Which day that time falls on is decided by the rule, not by when the sweep
            // happened to run: a deadline at or before the appearing hour means the next day -
            // "up at 22:00, due by 02:00" - and anything later means today. So a task that
            // opens late because nothing was running is honestly late, rather than being
            // handed a fresh 24 hours by the accident of a missed sweep.
            float hours_until_due (hidden=true) =
                $((due_minutes - clock_now + (due_minutes <= at_minutes ? 1440 : 0)) / 60.0)

            // Whether a still-open task is closed to make room for this occasion. Read by the
            // sweep, which does the closing - the rule judges, as with everything else here.
            bool expires_open (hidden=true) = $(when_open == "fail")

            // `active` first, so a rule that is switched off costs nothing to ask and answers
            // the same whatever the calendar says.
            bool due (hidden=true) = $(active && time_has_come && (when_open != "skip" || open_now == 0) && (
                mode == "daily"   ? fresh_today :
                mode == "weekly"  ? (weekday(today()) == ../weekday && fresh_today) :
                mode == "monthly" ? (day_of_month(today()) == ../day && fresh_today) :
                mode == "yearly"  ? (month_of(today()) == ../month &&
                                     day_of_month(today()) == ../day && fresh_today) :
                mode == "interval" ? (never_ran ||
                                     (since_done >= ../interval_days && fresh_since_done)) :
                false))
        }
    }

    // Most urgent first. Presentation only - the file keeps them in the order they arrived,
    // and `sort_by` sorts ascending, so the priority is negated to put the largest on top.
    list Open (entry=<Task>, key="added", layout="vertical",
               sort_by=$(|x| 0 - x/rank), spacing=6) {
        - {
            - added = "2026-08-04T09:00:00Z"
            - title = "Move the plants to bigger pots"
            - description = "strawberries, peppers, rosemary"
            - difficulty = 40
            - base_priority = 10
            - priority_gain = 2
        }
        - {
            - added = "2026-08-10T18:00:00Z"
            - rule = "kitchen_floor"
            - title = "Wash the kitchen floor"
            - difficulty = 25
            - base_priority = 5
            - priority_gain = 3
        }
    }

    text rules_header (markdown=true) = "## Rules"

    list Rules (entry=<Rule>, key="handle", layout="vertical", spacing=6) {
        - {
            - handle = "kitchen_floor"
            - title = "Wash the kitchen floor"
            - mode = "interval"
            - difficulty = 25
            - interval_days = 10
            - base_priority = 5
            - priority_gain = 3
        }
        - {
            - handle = "water_plants"
            - title = "Water the plants"
            - mode = "interval"
            - difficulty = 10
            - after = "kitchen_floor"
            - interval_days = 3
            - base_priority = 5
            - priority_gain = 1
        }
        - {
            - handle = "make_bed"
            - title = "Make the bed"
            - mode = "daily"
            - difficulty = 3
            - base_priority = 20
            - priority_gain = 5
        }
        - {
            - handle = "bins"
            - title = "Take the bins out"
            - mode = "weekly"
            - weekday = 2
            - difficulty = 5
            - base_priority = 40
            - priority_gain = 20
        }
        - {
            - handle = "rent"
            - title = "Pay the rent"
            - mode = "monthly"
            - day = 1
            - difficulty = 5
            - base_priority = 60
            - priority_gain = 30
        }
    }

    text history_header (markdown=true) = "## Done"

    // Newest last, like every other history here.
    // Newest first. `sort_by` sorts ascending only, so the instant is negated - the same way
    // `tasks/Open` negates its priority. `millis_since_epoch` rather than "how long ago",
    // because a key that follows the clock changes every minute and every entry of it then
    // lands in the delta of every interaction.
    //
    // Presentation only: the file keeps them in the order they happened, so appending stays an
    // append and the backup's line-wise merge sees what it expects.
    list History (entry=<Record>, layout="vertical",
               sort_by=$(|x| 0 - millis_since_epoch(x/done_at))) { }
}
