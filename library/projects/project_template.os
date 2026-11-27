tab project (label=$(name), mutable=true) {
    context vocabulary {
        - items = "/project/Items, /project/History, /project/Features"
        - topics = ":/project/Discussion"
        - people = "@/project/people/People"
    }
    // Raised whenever Stages, Complexities, Flags or the fields of an item change, so a program that reads projects can tell it was written against another.
    int template_version (hidden=true) = 2
    string name (label="", font-size=22px) = "Untitled project"
    text description (markdown=true, font-size=14px, hidden=$(../description == "")) = ""

    div (hidden=true) {
        div people {
            list People (key="tag") { }
        }

        div Seat {
            string seat = ""
            string account = ""
            timestamp since = "2026-01-01T00:00:00Z"
        }
        list Bench (entry=<Seat>, key="seat", mutable=false) { }

        div ActivityDay (background-color=$(points == 0 ? "" : points * 4 > busiest_day * 3 ? "#216e39"
                : points * 2 > busiest_day ? "#30a14e" : points * 4 > busiest_day ? "#40c463" : "#9be9a8")) {
            string day_key (hidden=true) = $(day(date()))
            int points (label="points") = $(/project/History.filter(|x| x/status != "cancelled" && day(x/finished_at) == ../day_key).sum(|x| x/points))
        }

        div WorkDay {
            string day_key = $(day(date()))
            int sessions = $(/project/History.filter(|x| day(x/finished_at) == ../day_key).sum(|x| x/Agents/Work.count()))
            float cost = $(/project/History.filter(|x| day(x/finished_at) == ../day_key).sum(|x| x/Agents/Work.sum(|r| r/cost)))
            int output = $(/project/History.filter(|x| day(x/finished_at) == ../day_key).sum(|x| x/Agents/Work.sum(|r| r/output)))
            int read = $(/project/History.filter(|x| day(x/finished_at) == ../day_key).sum(|x| x/Agents/Work.sum(|r| r/read)))
            int written = $(/project/History.filter(|x| day(x/finished_at) == ../day_key).sum(|x| x/Agents/Work.sum(|r| r/written)))
            float minutes = $(/project/History.filter(|x| day(x/finished_at) == ../day_key).sum(|x| x/Agents/Work.sum(|r| r/minutes)))
        }

        div StatTile (margin=0, padding=14, alignment="near", border-radius=12, shadow="soft", background-color=$(theme/surface)) { }

        div StatCard (layout="vertical", spacing=4, padding=16, border-radius=12, shadow="soft",
                      background-color=$(theme/surface)) { }

        div Label (layout="vertical", width=360px, margin=0, spacing=0, padding=2, border-radius=8px,
                   background-color=$(colour + "26")) {
            div (layout="horizontal", margin=0, spacing=4, alignment="center") {
                string name (label="", font-weight="bold", font-size=13px, padding=0, width=38%) = ""
                string tag (label="", font-size=11px, font-color="#9ca3af", padding=0, width=24%) = ""
                string colour (label="", kind="color", font-size=11px, font-color="#9ca3af", padding=0, width=28%) = "#6b7280"
                string glow (hidden=true) = ""
                button about (icon="note", margin=0, padding=0, width=10%, hover-text="What it is about",
                              background-color="transparent", hover-color=#8882,
                              font-color=$(../About/about == "" ? "#6b7280" : "inherit")) {
                    on click {
                        toggle_fold (path="../About")
                    }
                }
            }
            string title (label="hover", font-size=12px, padding=0) = ""
            div About (foldable=true, folded=true, margin=0) {
                text about (label="", markdown=true, font-size=12px, padding=4) = ""
            }
        }

        <Label> Tag {
            int points_done (hidden=true) = $(/project/History.filter(|x| x/status != "cancelled" && x/labels.filter(|t| t == ../tag).count() > 0).sum(|x| x/points))
            int points_total (hidden=true) = $(../points_done + /project/Items.filter(|x| x/labels.filter(|t| t == ../tag).count() > 0).sum(|x| x/points))
            float work_cost (hidden=true) = $(/project/History.filter(|x| x/labels.filter(|t| t == ../tag).count() > 0).sum(|x| x/Agents/Work.sum(|r| r/cost))
                + /project/Items.filter(|x| x/labels.filter(|t| t == ../tag).count() > 0).sum(|x| x/Agents/Work.sum(|r| r/cost)))
        }

        <Label> Milestone {
            int points_done (hidden=true) = $(/project/History.filter(|x| x/status != "cancelled" && x/milestones.filter(|t| t == ../tag).count() > 0).sum(|x| x/points))
            int points_total (hidden=true) = $(../points_done + /project/Items.filter(|x| x/milestones.filter(|t| t == ../tag).count() > 0).sum(|x| x/points))
        }

        <Label> Status {
            string icon (hidden=true) = ""
        }

        list Stages (entry=<Label>, key="tag") {
            - {
                - tag = "filed"
                - name = "filed"
                - title = "Nothing done yet"
                - colour = "#6b7280"
                - glow = "#9ca3af"
            }
            - {
                - tag = "planning"
                - name = "planning"
                - title = "Its plan is being worked out"
                - colour = "#0f766e"
                - glow = "#2dd4bf"
            }
            - {
                - tag = "ready"
                - name = "ready"
                - title = "Nothing left to decide"
                - colour = "#1d4ed8"
                - glow = "#60a5fa"
            }
            - {
                - tag = "testing"
                - name = "testing"
                - title = "Done; waits to be confirmed"
                - colour = "#7c3aed"
                - glow = "#a78bfa"
            }
            - {
                - tag = "parts"
                - name = "parts"
                - title = "Waits only on its parts"
                - colour = "#c2410c"
                - glow = "#fb923c"
            }
            - {
                - tag = "later"
                - name = "later"
                - title = "Decided against for now"
                - colour = "#eab308"
                - glow = "#facc15"
            }
            - {
                - tag = "finished"
                - name = "finished"
                - title = "Done and confirmed"
                - colour = "#15803d"
                - glow = "#4ade80"
            }
            - {
                - tag = "cancelled"
                - name = "cancelled"
                - title = "Dropped"
                - colour = "#9f1239"
                - glow = "#fb7185"
            }
        }

        list Complexities (entry=<Label>, key="tag") {
            - {
                - tag = "unassigned"
                - name = "unassigned"
                - title = "Not rated yet"
                - colour = "#6b7280"
            }
            - {
                - tag = "trivial"
                - name = "trivial"
                - title = "Straightforward, however much of it"
                - colour = "#15803d"
            }
            - {
                - tag = "low"
                - name = "low"
                - title = "One part, a little care"
                - colour = "#0f766e"
            }
            - {
                - tag = "medium"
                - name = "medium"
                - title = "A few parts, or real thought"
                - colour = "#b45309"
            }
            - {
                - tag = "high"
                - name = "high"
                - title = "Several parts, real analysis"
                - colour = "#b91c1c"
            }
        }

        list Statuses (entry=<Status>, key="tag") {
            - {
                - tag = "finished"
                - name = "finished"
                - colour = "#15803d"
                - icon = "check"
            }
            - {
                - tag = "cancelled"
                - name = "cancelled"
                - colour = "#9f1239"
                - icon = "cross"
            }
        }

        list Flags (entry=<Label>, key="tag") {
            - {
                - tag = "manual"
                - name = "manual"
                - title = "For a person to work"
                - colour = "#78716c"
            }
            - {
                - tag = "auto"
                - name = "auto"
                - title = "Filed by an agent"
                - colour = "#6b7280"
            }
            - {
                - tag = "discuss"
                - name = "discuss"
                - title = "Only an opinion is wanted"
                - colour = "#0369a1"
            }
            - {
                - tag = "first"
                - name = "first"
                - title = "Goes before everything else"
                - colour = "#e11d48"
            }
            - {
                - tag = "asked"
                - name = "asked"
                - title = "A question waits on a person"
                - colour = "#b45309"
            }
            - {
                - tag = "trusted"
                - name = "trusted"
                - title = "Finished once it reaches testing"
                - colour = "#4d7c0f"
            }
        }

        div Comment (layout="vertical", margin=0, spacing=0) {
            context theme {
                - backdrop (base=none) {
                    - mark {
                        - kind = "icon"
                        - source = $(/project/people/People.find(author)/icon)
                        - colour = $(/project/people/People.find(author)/colour)
                        - opacity = 0.18
                        - size = "120px"
                        - position = "right 16px top 8px"
                    }
                }
                - audio {
                    - request = "transcription"
                }
            }
            div (layout="horizontal", margin=0, spacing=8, alignment="center") {
                timestamp at (format="datetime", precision="minutes", font-size=11px, font-color="#9ca3af", margin=0) =
                    "2026-01-01T00:00:00Z"
                string author (hidden=true) = ""
                text by (font-size=11px, font-weight="bold", margin=0, mutable=false) =
                    $(/project/people/People.filter(|p| p/tag == ../author).count() > 0 ? "<@" + ../author + ">" : ../author)
                string status (font-size=11px, font-color="#9ca3af", margin=0, hover-text="What the author wrote it as",
                               hidden=$(../status == "")) = ""
                button drop (icon="trash-2", margin=0, hover-text="Remove this comment") {
                    on click {
                        set (path="../../../../touched_at", mode="value") = $(now())
                        remove (from="../../Comments", keyField="at", keyValue=$(../at))
                    }
                }
            }
            text body (markdown=true, font-size=12px, margin=0) = "" {
                on change {
                    set (path="../../../../touched_at", mode="value") = $(now())
                }
            }
        }

        div Run (layout="horizontal", margin=0, spacing=6, alignment="center") {
            timestamp at (label="when", format="datetime", precision="minutes", width=13%) = "2026-01-01T00:00:00Z"
            mark (type="separator")
            string kind (label="kind", width=8%) = ""
            mark (type="separator")
            string model (label="model", width=12%) = ""
            mark (type="separator")
            string moved (label="moved", width=14%) = ""
            mark (type="separator")
            int points (label="pts", format="trim", width=5%) = 1
            mark (type="separator")
            float cost (label="cost", prefix="$", precision=2, width=7%) = 0.0
            mark (type="separator")
            int read (label="read", suffix="k", format="trim", width=8%) = 0
            mark (type="separator")
            int written (label="written", suffix="k", format="trim", width=8%) = 0
            mark (type="separator")
            int output (label="output", suffix="k", format="trim", width=7%) = 0
            mark (type="separator")
            int turns (label="turns", format="trim", width=5%) = 0
            mark (type="separator")
            float minutes (label="time", suffix=" min", precision=1, width=7%) = 0.0
            mark (type="separator")
            int lines (label="lines", format="trim", width=5%) = 0
        }

        div Item (layout="horizontal", margin=0, margin-bottom=3px, spacing=6, padding=2, alignment="center",
                  background-color="inherit",
                  movable=$(pinned == false),
                  border-style=$(working == true ? "solid 2px #22c55e" : "default"),
                  shadow=$(stage == "filed" || stage == "later" ? "none" :
                      "inset 3px 0 0 " + glow + ", inset 16px 0 16px -14px " + glow + "88, -3px 0 10px -5px " + glow)) {
            context hover-text = true
            context theme {
                - text = $(waiting > 0 ? muted : "inherit")
                - backdrop {
                    - working_mark {
                        - kind = $(working ? "icon" : "none")
                        - source = "cog"
                        - colour = "#22c55e"
                        - opacity = 0.85
                        - size = "20px"
                        - position = $("left calc(14% - " + (34 + depth() * 16) + "px) top " + (worker_by != "" ? 33 : 12) + "px")
                        - motion = "turn"
                        - period = "3s"
                    }
                    - worker_mark {
                        - kind = $(working && worker != "" && worker_by == "" ? "text" : "none")
                        - text = $(worker)
                        - colour = "#22c55e"
                        - size = "14px"
                        - position = "right 8px top 1px"
                    }
                    - asked_mark {
                        - kind = $(asked && !working ? "icon" : "none")
                        - source = "message-circle-question"
                        - colour = "#fcd34d"
                        - size = "20px"
                        - position = $("left calc(14% - " + (34 + depth() * 16) + "px) top 12px")
                        - motion = "breathe"
                        - period = "2.4s"
                    }
                    - asked_corner {
                        - kind = $(asked ? "gradient" : "none")
                        - from = "#f59e0b"
                        - to = "transparent"
                        - angle = 225
                        - sharpness = "100%"
                        - size = "18px"
                        - position = "right top"
                    }
                    - asked_edge {
                        - kind = $(asked ? "gradient" : "none")
                        - from = "#fcd34d"
                        - to = "transparent"
                        - angle = 225
                        - sharpness = "100%"
                        - size = "20px"
                        - position = "right top"
                    }
                    - asked_glow {
                        - kind = $(asked ? "gradient" : "none")
                        - from = "#f59e0b77"
                        - to = "transparent"
                        - angle = 225
                        - center = "20%"
                        - sharpness = "60%"
                        - size = "72px"
                        - position = "right top"
                        - motion = "breathe"
                        - period = "2.4s"
                    }
                }
            }

            text worker_chip (markdown=true, span="row", mutable=false,
                              font-size=11px, font-color="#22c55e", margin=0, padding=0, alignment="far",
                              hidden=$(!(working && worker_by != ""))) =
                $((/project/people/People.filter(|p| p/tag == ../worker_by).count() > 0 ? "<@" + ../worker_by + ">" : ../worker_by)
                  + (../worker != "" ? " · " + ../worker : ""))
            mark (type="grip")
            timestamp added (hidden=true) = "2026-01-01T00:00:00Z"
            string handle (label="id", font-size=11px, width=14%) = ""
            mark (type="separator")
            enum owner (label="by", vocabulary="/project/people/People", chip="icon", placeholder="", clearable=true, font-size=11px, width=32px,
                        alignment="center",
                        hover-text=$(../owner == "" ? "Nobody owns this item" : "This item is owned by <@" + ../owner + ">")) = ""
            mark (type="separator")
            enum assignee (label="for", vocabulary="/project/people/People", chip="icon", placeholder="", clearable=true, font-size=11px, width=32px,
                           alignment="center",
                           hover-text=$(../assignee == "" ? "Nobody is to work this item" : "This item is for <@" + ../assignee + ">")) = ""
            mark (type="separator")
            string title (label="title", font-size=14px, width=37%,
                          hover-text="Item title or short description",
                          font-weight=$(../kids > 0 ? "bold" : "normal")) = ""

            mark (type="separator")
            enum stage (label="stage", vocabulary="/project/Stages", width=5%, alignment="center",
                        hover-text=$("This item is at the <#" + ../stage + "> stage"),
                        withhold=$(open_kids > 0 ? "finished, cancelled" : "parts")) = "filed" {
                on change {
                    set (path="../touched_at", mode="value") = $(now())
                    if (cond=$((../stage == "finished" || ../stage == "cancelled") && ../open_kids == 0)) {
                        if (cond=$(/project/Items.filter(|x| x/handle == ../parent && x/stage == "parts" && x/open_kids == 1).count() > 0)) {
                            set_in_list (list="/project/Items", keyField="handle", keyValue=$(../parent), field="stage") = "testing"
                            set_in_list (list="/project/Items", keyField="handle", keyValue=$(../parent), field="touched_at") = $(now())
                        }
                        append (list="/project/History", from="..") {
                            - finished_at = $(now())
                            - status = $(../stage)
                        }
                        remove (from="/project/Items", keyField="handle", keyValue=$(../handle))
                    }
                }
            }

            enum parent (hidden=true) = ""
            mark (type="separator")
            tags after (label="after", width=8%,
                        hover-text="The list of items this waits on") = ""
            mark (type="separator")
            tags labels (label="tags", vocabulary="/project/Labels", width=8%,
                         hover-text="Project tags") = ""
            mark (type="separator")
            tags milestones (label="milestones", vocabulary="/project/Milestones", shape="slant", width=8%,
                             hover-text="Milestones") = ""
            mark (type="separator")
            tags flags (label="flags", vocabulary="/project/Flags", width=6%,
                        hover-text="Workflow flags") = ""
            mark (type="separator")
            int points (label="pts", format="trim", width=3%, alignment="center",
                        hover-text="Estimated points") = 1
            mark (type="separator")
            enum complexity (label="think", vocabulary="/project/Complexities", width=6%, alignment="center",
                             hover-text="Estimated cognitive effort: the model is selected based on it") = "unassigned"
            bool pinned (hidden=true) = false
            bool working (hidden=true) = false
            string worker (hidden=true) = ""
            string worker_by (hidden=true) = ""

            timestamp touched_at (hidden=true) = "2026-01-01T00:00:00Z"
            mark (type="separator")
            timestamp moved_at (label="moved", mode="elapsed", font-size=11px, width=5%,
                               hover-text="Time since last change") =
                $(../said_at > ../touched_at ? ../said_at : ../touched_at)

            mark (type="separator")
            button talk (icon="note", margin=0, width=36px,
                         font-color=$(../Talk/Comments.count() == 0 ? "#6b7280" : "inherit")) {
                on click {
                    toggle_fold (path="../Talk")
                }
            }
            button work (icon="burger", margin=0, width=36px, hidden=$(../Agents/Work.count() == 0)) {
                on click {
                    toggle_fold (path="../Agents")
                }
            }
            button pin (icon="pin", margin=0, width=36px, hover-text="Keep it on top of the list",
                        background-color=$(../pinned == true ? "#f59e0b" : "transparent"),
                        font-color=$(../pinned == true ? "#1a1a1a" : "#6b7280")) {
                on click {
                    toggle (path="../pinned")
                }
            }
            button drop (icon="cross", margin=0, width=36px, hidden=$(open_kids > 0)) {
                on click {
                    remove (from="/project/Items", keyField="handle", keyValue=$(../handle))
                }
            }
            // An agent loop takes the item by pressing this: a press is one write, so of two loops
            // pressing at once one gets the item and the other is refused.
            button claim (hidden=true) {
                on click {
                    if (cond=$(../working == true)) {
                        refuse = "another agent is working on it"
                    }
                    set (path="../working") = true
                    set (path="../worker_by") = $(me/handle)
                }
            }

            int done (view="progress", span="row", margin=0, margin-left=24px, margin-right=24px, padding=0,
                      height=5px, color=$(done >= 100 ? "#4ade80" : "#eab308"),
                      shadow=$(done >= 100 ? "0 0 8px #4ade80" : "none"),
                      hover-text=$(../done - ../done % 1 + "% done"),
                      hidden=$(kids == 0)) = $(kids == 0 || weight == 0 ? 0 :
                          (/project/Items.filter(|x| x/parent == ../handle)
                                         .map(|x| x/done * x/weight).sum()
                           + finished_weight * 100) / weight)

            div Talk (foldable=true, folded=true, span="row", layout="vertical", margin=0, spacing=4) {
                list Comments (entry=<Comment>, layout="vertical", spacing=4, border-style=none) { }
                button add (label="add comment", icon="message-square-plus", margin-top=4, margin-bottom=8,
                            margin-left=0, margin-right=0) {
                    on click {
                        set (path="../../touched_at", mode="value") = $(now())
                        append (list="../Comments") {
                            - at = $(now())
                            - author = $(me/handle == "" ? "owner" : me/handle)
                        }
                        start_editing (path="../Comments[-1]/body")
                    }
                }
            }
            div Agents (foldable=true, folded=true, span="row", layout="vertical", margin=0) {
                list Work (entry=<Run>, view="table", header=true, spacing=1) { }
            }

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
            string shape (hidden=true) = "ticket"
            string glow (hidden=true) =
                $(/project/Stages.filter(|s| s/tag == ../stage).map(|s| s/glow).first())
            bool asked (hidden=true) = $(../flags.filter(|t| t == "asked").count() > 0)
            timestamp closed_at (hidden=true) =
                $(/project/History.filter(|x| x/parent == ../handle).count() == 0 ? ../added :
                  /project/History.filter(|x| x/parent == ../handle)
                                  .map(|x| x/finished_at).max())
            timestamp said_at (hidden=true) =
                $(../Talk/Comments.filter(|c| c/at > ../closed_at).count() == 0 ? ../closed_at :
                  ../Talk/Comments.map(|c| c/at).max())
            string last_author (hidden=true) = $(../Talk/Comments.reduce("", |a, c| c/author))

            on dblclick {
                call (name="copy") {
                    - link = $("project/Items/[" + handle + "]")
                }
            }
        }

        div Topic (layout="horizontal", margin=0, margin-bottom=3px, spacing=6, padding=2, alignment="center",
                   background-color="inherit", movable=$(pinned == false)) {
            context hover-text = true

            mark (type="grip")
            timestamp added (hidden=true) = "2026-01-01T00:00:00Z"
            string handle (label="id", font-size=11px, width=8%) = ""
            string shape (hidden=true) = "bubbles"
            mark (type="separator")
            enum owner (label="by", vocabulary="/project/people/People", chip="icon", placeholder="", font-size=11px, width=32px,
                        alignment="center",
                        hover-text=$(../owner == "" ? "Nobody started this topic" : "This topic was started by <@" + ../owner + ">")) = ""
            mark (type="separator")
            string title (label="title", font-size=14px, width=87%,
                          hover-text="What the topic is about") = ""
            bool pinned (hidden=true) = false

            timestamp touched_at (hidden=true) = "2026-01-01T00:00:00Z"
            mark (type="separator")
            timestamp moved_at (label="moved", mode="elapsed", font-size=11px, width=5%,
                               hover-text="Time since last change") =
                $(../said_at > ../touched_at ? ../said_at : ../touched_at)

            mark (type="separator")
            button talk (icon="note", margin=0, width=36px,
                         font-color=$(../Talk/Comments.count() == 0 ? "#6b7280" : "inherit")) {
                on click {
                    toggle_fold (path="../Talk")
                }
            }
            button pin (icon="pin", margin=0, width=36px, hover-text="Keep it on top of the list",
                        background-color=$(../pinned == true ? "#f59e0b" : "transparent"),
                        font-color=$(../pinned == true ? "#1a1a1a" : "#6b7280")) {
                on click {
                    toggle (path="../pinned")
                }
            }
            button drop (icon="cross", margin=0, width=36px) {
                on click {
                    remove (from="/project/Discussion", keyField="handle", keyValue=$(../handle))
                }
            }

            div Talk (foldable=true, folded=true, span="row", layout="vertical", margin=0, spacing=4) {
                list Comments (entry=<Comment>, layout="vertical", spacing=4, border-style=none) { }
                button add (label="add comment", icon="message-square-plus", margin-top=4, margin-bottom=8,
                            margin-left=0, margin-right=0) {
                    on click {
                        set (path="../../touched_at", mode="value") = $(now())
                        append (list="../Comments") {
                            - at = $(now())
                            - author = $(me/handle == "" ? "owner" : me/handle)
                        }
                        start_editing (path="../Comments[-1]/body")
                    }
                }
            }

            timestamp said_at (hidden=true) =
                $(../Talk/Comments.count() == 0 ? ../added : ../Talk/Comments.map(|c| c/at).max())

            on dblclick {
                call (name="copy") {
                    - link = $("project/Discussion/[" + handle + "]")
                }
            }
        }

        div Finished (layout="horizontal", margin=0, margin-bottom=3px, spacing=6, padding=2, alignment="center",
                      background-color="inherit") {
            context hover-text = true

            timestamp added (hidden=true) = "2026-01-01T00:00:00Z"
            enum status (label="", vocabulary="/project/Statuses", chip="icon", font-size=11px, width=32px, alignment="center",
                         hover-text=$("This item was " + ../status)) = "finished"
            mark (type="separator")
            string handle (label="id", font-size=11px, width=14%) = ""
            mark (type="separator")
            enum owner (label="by", vocabulary="/project/people/People", chip="icon", placeholder="", clearable=true, font-size=11px, width=32px,
                        alignment="center",
                        hover-text=$(../owner == "" ? "Nobody owned this item" : "This item was owned by <@" + ../owner + ">")) = ""
            mark (type="separator")
            enum assignee (label="for", vocabulary="/project/people/People", chip="icon", placeholder="", clearable=true, font-size=11px, width=32px,
                           alignment="center",
                           hover-text=$(../assignee == "" ? "Nobody was to work this item" : "This item was for <@" + ../assignee + ">")) = ""
            mark (type="separator")
            string title (label="title", font-size=14px, width=42%,
                          hover-text="Item title or short description") = ""
            string parent (hidden=true) = ""
            tags after (hidden=true) = ""
            mark (type="separator")
            tags labels (label="tags", vocabulary="/project/Labels", width=8%,
                         hover-text="Project tags") = ""
            mark (type="separator")
            tags milestones (label="milestones", vocabulary="/project/Milestones", shape="slant", width=8%,
                             hover-text="Milestones") = ""
            mark (type="separator")
            tags flags (label="flags", vocabulary="/project/Flags", width=6%,
                        hover-text="Workflow flags") = ""
            mark (type="separator")
            int points (label="pts", format="trim", width=3%, alignment="center",
                        hover-text="Points") = 0
            mark (type="separator")
            enum complexity (label="think", vocabulary="/project/Complexities", width=6%, alignment="center",
                             hover-text="How much thinking building it took") = "unassigned"
            timestamp touched_at (hidden=true) = "2026-01-01T00:00:00Z"
            mark (type="separator")
            timestamp finished_at (label="closed", mode="elapsed", font-size=11px, width=6%,
                                   hover-text="Time since it was closed") = "2026-01-01T00:00:00Z"

            mark (type="separator")
            button talk (icon="note", margin=0, width=36px, hover-text="Comments",
                         font-color=$(../Talk/Comments.count() == 0 ? "#6b7280" : "inherit")) {
                on click {
                    toggle_fold (path="../Talk")
                }
            }
            button work (icon="burger", margin=0, width=36px, hover-text="Agent sessions",
                         hidden=$(../Agents/Work.count() == 0)) {
                on click {
                    toggle_fold (path="../Agents")
                }
            }

            // Back on the open list at filed, with what the record kept. Hidden while the
            // parent is closed too, so a closed item never has an open part: reopen the
            // parent first. The task history keeps the record of the earlier finish.
            button reopen (icon="arrow-up", margin=0, width=36px, hover-text="Back to the open list, at filed",
                           hidden=$(/project/History.filter(|x| x/handle == ../parent).count() > 0)) {
                on click {
                    if (cond=$(/project/Items.filter(|x| x/handle == ../handle).count() == 0)) {
                        prepend (list="/project/Items", from="..") {
                            - complexity = "unassigned"
                            - touched_at = $(now())
                        }
                        remove (from="/project/History", keyField="handle", keyValue=$(../handle))
                    }
                }
            }

            div Talk (foldable=true, folded=true, span="row", layout="vertical", margin=0, spacing=4) {
                list Comments (entry=<Comment>, layout="vertical", spacing=4, border-style=none) { }
            }
            div Agents (foldable=true, folded=true, span="row", layout="vertical", margin=0) {
                list Work (entry=<Run>, view="table", header=true, spacing=1) { }
            }
            string colour (hidden=true) =
                $(/project/Statuses.filter(|s| s/tag == ../status).map(|s| s/colour).first())
            string shape (hidden=true) = "ticket"

            on dblclick {
                call (name="copy") {
                    - link = $("project/History/[" + handle + "]")
                }
            }
        }

        list Standings (entry=<Label>, key="tag") {
            - {
                - tag = "proposed"
                - name = "proposed"
                - title = "Suggested, nothing decided"
                - colour = "#6b7280"
            }
            - {
                - tag = "wanted"
                - name = "wanted"
                - title = "Worth having, not yet taken on"
                - colour = "#2563eb"
            }
            - {
                - tag = "accepted"
                - name = "accepted"
                - title = "Part of the project's plan"
                - colour = "#16a34a"
            }
            - {
                - tag = "discarded"
                - name = "discarded"
                - title = "Decided against"
                - colour = "#be123c"
            }
        }

        div Feature (layout="vertical", width=260px, margin=0, spacing=2, padding=8, border-radius=10px,
                     background-color=$(colour + "1a"),
                     shadow=$("inset 0 3px 0 " + colour + ", 0 4px 12px -6px " + colour)) {
            timestamp added (hidden=true) = "2026-01-01T00:00:00Z"
            div (layout="horizontal", margin=0, spacing=4, alignment="center") {
                string handle (label="", font-size=11px, font-weight="bold", font-color=$(colour), padding=0,
                               width=136px) = ""
                enum standing (label="", vocabulary="/project/Standings", font-size=11px, width=96px, alignment="center",
                               hover-text="How close it is to the project: proposed, wanted, accepted or discarded") =
                    "proposed" {
                    on change {
                        set (path="../touched_at", mode="value") = $(now())
                    }
                }
            }
            text title (label="", font-size=14px, font-weight="bold", margin=0) = ""
            bool relating (hidden=true, mutable="guarded") = false
            div (layout="vertical", margin=0, spacing=2) {
                tags requires (label="needs", vocabulary="/project/Features", font-size=11px, margin=0, padding=0,
                               line-arrow=true,
                               hover-text="Features that have to be accepted for this one to make sense",
                               hidden=$(../requires.count() == 0 && !../relating)) = ""
                tags incompatible (label="excludes", vocabulary="/project/Features", font-size=11px, margin=0, padding=0,
                                   line-color="#be123c", line-style="dashed",
                                   hover-text="Features it cannot be accepted together with",
                                   hidden=$(../incompatible.count() == 0 && !../relating)) = ""
                tags synergies (label="goes with", vocabulary="/project/Features", font-size=11px, margin=0, padding=0,
                                line-color="#16a34a",
                                hover-text="Features that work well with it, or gain from it",
                                hidden=$(../synergies.count() == 0 && !../relating)) = ""
            }
            timestamp touched_at (hidden=true) = "2026-01-01T00:00:00Z"
            float x (hidden=true) = 0
            float y (hidden=true) = 0

            div (layout="horizontal", margin=0, spacing=2, alignment="center") {
                button detail (icon="file-text", margin=0, width=32px, background-color="transparent", hover-color=#8882,
                               hover-text="What it is, its pros and its cons",
                               font-color=$(../Detail/description == "" && ../Detail/pros == "" && ../Detail/cons == "" ? "#6b7280" : "inherit")) {
                    on click {
                        toggle_fold (path="../Detail")
                    }
                }
                button links (icon="link", margin=0, width=32px, background-color="transparent", hover-color=#8882,
                              hover-text="What it needs, excludes and goes with",
                              font-color=$(../requires == "" && ../incompatible == "" && ../synergies == "" ? "#6b7280" : "inherit")) {
                    on click {
                        set (path="../relating") = $(!../relating)
                    }
                }
                button talk (icon="message-square", margin=0, width=32px, background-color="transparent", hover-color=#8882,
                             hover-text="Its conversation",
                             font-color=$(../Talk/Comments.count() == 0 ? "#6b7280" : "inherit")) {
                    on click {
                        toggle_fold (path="../Talk")
                    }
                }
                timestamp moved_at (label="", mode="elapsed", font-size=11px, font-color="#9ca3af", width=100px,
                                   alignment="center", hover-text="Time since it last changed") =
                    $(../touched_at > ../added ? ../touched_at : ../added)
                button drop (icon="cross", margin=0, width=32px, background-color="transparent", hover-color=#8882,
                             font-color="#6b7280", hover-text="Remove it") {
                    on click {
                        remove (from="/project/Features", keyField="handle", keyValue=$(../handle))
                    }
                }
            }

            div Detail (foldable=true, folded=true, layout="vertical", margin=0, spacing=4, overflow-x=auto) {
                text description (label="", markdown=true, font-size=12px) = ""
                text pros_header (markdown=true, margin=0) = "**Pros**"
                text pros (label="", markdown=true, font-size=12px) = ""
                text cons_header (markdown=true, margin=0) = "**Cons**"
                text cons (label="", markdown=true, font-size=12px) = ""
            }
            div Talk (foldable=true, folded=true, layout="vertical", margin=0, spacing=4) {
                list Comments (entry=<Comment>, layout="vertical", spacing=4, border-style=none) { }
                button add (label="add comment", icon="message-square-plus", margin-top=4, margin-bottom=8,
                            margin-left=0, margin-right=0) {
                    on click {
                        set (path="../../touched_at", mode="value") = $(now())
                        append (list="../Comments") {
                            - at = $(now())
                            - author = $(me/handle == "" ? "owner" : me/handle)
                        }
                        start_editing (path="../Comments[-1]/body")
                    }
                }
            }
            string colour (hidden=true) =
                $(/project/Standings.filter(|s| s/tag == ../standing).map(|s| s/colour).first())
        }
    }

    // The plan in two tabs - what is still to do, and what was finished or cancelled - so the
    // closed items are a tap away rather than a scroll past the open ones. Unnamed, so they are
    // no step of any address: the lists are /project/Items and /project/History as they always
    // were, to the formulas here and to anything reading them alike.
    tab (label="Open", hover-text="What is still to do") {
        context theme {
            - table {
                - frame = "none"
                - row {
                    - pointed = "tint"
                }
                - field {
                    - spacing = "4px"
                }
            }
            - tree {
                - stand = "row"
                - indent = "16px"
                - color = $(muted)
            }
        }
        div (layout="horizontal", margin=0, spacing=10, alignment="center", border-style=none) {
            div Narrow (layout="horizontal", margin=0, spacing=10, alignment="center", border-style=none) {
                textbox find (placeholder="find", narrows="/project/Items", by="title, handle, Comments", width=20%) = ""
                textbox stage (placeholder="stages", vocabulary="/project/Stages", narrows="/project/Items", by="stage") = ""
                textbox labels (placeholder="tags", vocabulary="/project/Labels", narrows="/project/Items", by="labels") = ""
                textbox milestones (placeholder="milestones", vocabulary="/project/Milestones", narrows="/project/Items",
                                    by="milestones", shape="slant") = ""
                textbox flags (placeholder="flags", vocabulary="/project/Flags", narrows="/project/Items", by="flags") = ""
                textbox branch (placeholder="branch", vocabulary="/project/Items", narrows="/project/Items", by="handle",
                                branch=true) = ""
            }
            div bench (width=1fr, height=36px, margin=0, padding=0, alignment="far", border-style=none) {
                context theme {
                    - backdrop {
                        - armchair {
                            - kind = "icon"
                            - source = "armchair"
                            - opacity = 0.35
                            - size = "32px"
                            - width = "96px"
                            - position = "right top 8px"
                        }
                    }
                }
                div (layout="horizontal", width=fit-content, height=1fr, margin=0, margin-right=6px, padding=0, spacing=4,
                     alignment="center", border-style=none) {
                    tags waiting (vocabulary="/project/people/People", chip="icon", shape="box", mutable=false, font-size=11px, margin=0, padding=0,
                                  hover-text="Agents waiting for work") =
                        $(/project/Bench.filter(|x| /project/Bench.filter(|y| y/account == x/account
                            && (y/since < x/since || (y/since == x/since && y/seat < x/seat))).count() == 0
                            && /project/people/People.filter(|p| p/tag == x/account).count() > 0)
                          .reduce("", |acc, x| acc + (acc == "" ? "" : ", ") + x/account))
                    text strangers (mutable=false, font-size=11px, alignment="far", margin=0, padding=0,
                                    hover-text="Agents waiting for work") =
                        $(/project/Bench.filter(|x| /project/Bench.filter(|y| y/account == x/account
                            && (y/since < x/since || (y/since == x/since && y/seat < x/seat))).count() == 0
                            && /project/people/People.filter(|p| p/tag == x/account).count() == 0)
                          .reduce("", |acc, x| acc + (acc == "" ? "" : " ") + x/account))
                }
            }
        }

        div NewTask (layout="horizontal", margin=0, spacing=6, alignment="center", border-style=none) {
            textbox handle (label="", placeholder="id", width=10%) = $(fresh_key(/project/Items, /project/History, /project/Discussion))
            textbox title (label="", placeholder="what needs doing", width=18%) = ""
            textbox parent (label="", placeholder="under", vocabulary="/project/Items", limit=1, width=10%) = ""
            textbox after (label="", placeholder="after", vocabulary="/project/Items, /project/History",
                           width=10%) = ""
            textbox labels (label="", placeholder="tags", vocabulary="/project/Labels", width=10%) = ""
            textbox milestones (label="", placeholder="milestones", vocabulary="/project/Milestones",
                                shape="slant", width=12%) = ""
            textbox flags (label="", placeholder="flags", vocabulary="/project/Flags", width=10%) = "manual"
            textbox points (label="", placeholder="pts", width=7%) = "5"
            button add (label="+ task", margin=0, width=12%) {
                on click {
                    if (cond=$(../handle == "")) {
                        refuse = "say an id"
                    }
                    if (cond=$(/project/Items.filter(|x| x/handle == ../handle).count() > 0 || /project/History.filter(|x| x/handle == ../handle).count() > 0 || /project/Discussion.filter(|x| x/handle == ../handle).count() > 0)) {
                        refuse = "this id is taken, say another"
                    }
                    prepend (list="/project/Items", from="..") {
                        - added = $(now())
                        - owner = $(me/handle)
                    }
                }
            }
        }

        list Items (entry=<Item>, key="handle", tree="parent", layout="vertical", spacing=1, view="table", header=true, sticky=true, movable="anywhere", sortable="handle, stage, flags, points, moved_at", sort_by=$(|x| x/pinned == true ? 0 : 1)) { }

        div (layout="horizontal", margin=0, spacing=16, alignment="center") {
            int open_now (label="open") = $(/project/Items.count())
            int points_open (label="points outstanding") = $(/project/Items.map(|x| x/points).sum())
            int points_won (label="points finished") =
                $(/project/History.filter(|x| x/status != "cancelled").map(|x| x/points).sum())
        }
    }

    tab (label="Closed", hover-text="Finished and cancelled items, newest first") {
        list History (entry=<Finished>, key="handle", window=20, parts=$(|x| week(x/finished_at)), layout="vertical", spacing=1, view="table", header=true, sticky=true, sort_by=$(|x| 0 - millis_since_epoch(x/finished_at))) { }
    }

    // A tracker writes `- Discussion { ... }` beside `- Items` once it holds any topic. Text names
    // a topic as `<:id>`; its ids still come from the items' pool, so one id is one thing anywhere.
    tab (label="Discussion", hover-text="Anything to talk over that is not an item or a feature") {
        context theme {
            - table {
                - frame = "none"
                - row {
                    - pointed = "tint"
                }
                - field {
                    - spacing = "4px"
                }
            }
        }
        div FindTopic (layout="horizontal", margin=0, spacing=10, alignment="center", border-style=none) {
            textbox find (placeholder="find", narrows="/project/Discussion", by="title, handle, Comments", width=20%) = ""
        }

        div NewTopic (layout="horizontal", margin=0, spacing=6, alignment="center", border-style=none) {
            textbox handle (label="", placeholder="id", width=10%) = $(fresh_key(/project/Items, /project/History, /project/Discussion))
            textbox title (label="", placeholder="what to talk over", width=56%) = ""
            button add (label="+ topic", margin=0, width=12%) {
                on click {
                    if (cond=$(../handle == "")) {
                        refuse = "say an id"
                    }
                    if (cond=$(/project/Items.filter(|x| x/handle == ../handle).count() > 0 || /project/History.filter(|x| x/handle == ../handle).count() > 0 || /project/Discussion.filter(|x| x/handle == ../handle).count() > 0)) {
                        refuse = "this id is taken, say another"
                    }
                    prepend (list="/project/Discussion", from="..") {
                        - added = $(now())
                        - touched_at = $(now())
                        - owner = $(me/handle)
                    }
                }
            }
        }

        list Discussion (entry=<Topic>, key="handle", layout="vertical", spacing=1, view="table", header=true, sticky=true, movable="anywhere", sort_by=$(|x| x/pinned == true ? 0 : 1)) { }
    }

    // Inside the project tab, so that every tracker made from it has the features too: a tracker
    // writes `- Features { ... }` beside `- Items` once it holds any.
    tab (label="Features", hover-text="What the project could become, each feature placed beside the ones it goes with") {
        div FindFeature (layout="horizontal", margin=0, spacing=10, alignment="center", border-style=none) {
            textbox find (placeholder="find", narrows="/project/Features",
                          by="handle, title, description, pros, cons, Comments", width=30%) = ""
            textbox standing (placeholder="standings", vocabulary="/project/Standings", narrows="/project/Features",
                              by="standing", width=20%) = ""
        }

        div NewFeature (layout="horizontal", margin=0, spacing=6, alignment="center", border-style=none) {
            textbox handle (label="", placeholder="HANDLE", width=20%) = ""
            textbox title (label="", placeholder="what it is, in a line", width=52%) = ""
            button add (label="+ feature", margin=0, width=16%) {
                on click {
                    if (cond=$(../handle == "")) {
                        refuse = "say a handle"
                    }
                    if (cond=$(/project/Features.filter(|x| x/handle == ../handle).count() > 0)) {
                        refuse = "this handle is taken, say another"
                    }
                    append (list="/project/Features", from="..") {
                        - added = $(now())
                        - touched_at = $(now())
                    }
                }
            }
        }

        list Features (entry=<Feature>, key="handle", view="canvas", x="x", y="y",
                      lines="requires, incompatible, synergies") { }
    }

    // Worked out from the lists alone and writing nothing. What it reads of History is totals the
    // parts' summaries answer: a filter comparing a function of the entry's fields with what the
    // reader holds. The clock or a calendar day's date read on the entry's side would have every
    // part read on each open.
    tab (label="Stats", hover-text="The project added up", mutable=false) {
        context theme {
            - chart {
                - frame = "none"
            }
        }

        text overview_header (markdown=true) = "### <icon=gauge> Overview"
        div (layout="flow", min-width=140px, spacing=12) {
            <StatTile> {
                int tasks_total (label="tasks", font-size=26px) = $(/project/Items.count() + /project/History.filter(|x| x/status != "cancelled").count())
            }
            <StatTile> {
                int tasks_open (label="tasks open", font-size=26px) = $(/project/Items.count())
            }
            <StatTile> {
                int tasks_finished (label="tasks finished", font-size=26px, font-color=$(theme/success)) = $(/project/History.filter(|x| x/status != "cancelled").count())
            }
            <StatTile> {
                int points_total (label="points", font-size=26px) = $(/project/points_open + /project/points_won)
            }
            <StatTile> {
                int points_left (label="points outstanding", font-size=26px, font-color=$(theme/accent)) = $(/project/points_open)
            }
            <StatTile> {
                int points_finished (label="points finished", font-size=26px, font-color=$(theme/success)) = $(/project/points_won)
            }
        }

        <StatCard> {
            float finished_share (label="finished", view="progress", color=$(theme/success)) =
                $(points_total == 0 ? 0 : points_finished / points_total)
        }

        string held (hidden=true) = $(/project/Items.filter(|x| x/waiting > 0).count() + " waiting on other items, "
            + /project/Items.filter(|x| x/flags.filter(|f| f == "asked").count() > 0).count() + " asked, "
            + /project/Items.filter(|x| x/stage == "testing").count() + " at testing")

        div (layout="flow", min-width=140px, spacing=12) {
            <StatTile> {
                int held_waiting (label="waiting on other items", font-size=26px,
                                  font-color=$(held_waiting > 0 ? theme/warning : theme/muted)) = $(/project/Items.filter(|x| x/waiting > 0).count())
            }
            <StatTile> {
                int held_asked (label="asked, waiting on a person", font-size=26px,
                                font-color=$(held_asked > 0 ? theme/danger : theme/muted)) = $(/project/Items.filter(|x| x/flags.filter(|f| f == "asked").count() > 0).count())
            }
            <StatTile> {
                int held_testing (label="at testing, to be confirmed", font-size=26px,
                                  font-color=$(held_testing > 0 ? theme/accent : theme/muted)) = $(/project/Items.filter(|x| x/stage == "testing").count())
            }
        }

        text open_header (markdown=true) = "### <icon=chart-pie> Open work"
        div (layout="flow", min-width=280px, spacing=12) {
            <StatCard> {
                text stages_header (markdown=true) = "**Open items by stage**"
                chart stages (kind="pie", height=180px) {
                    plot filed (label="filed", color=$(/project/Stages.find("filed")/colour),
                                amount=$(/project/Items.filter(|x| x/stage == "filed").count()))
                    plot planning (label="planning", color=$(/project/Stages.find("planning")/colour),
                                   amount=$(/project/Items.filter(|x| x/stage == "planning").count()))
                    plot ready (label="ready", color=$(/project/Stages.find("ready")/colour),
                                amount=$(/project/Items.filter(|x| x/stage == "ready").count()))
                    plot testing (label="testing", color=$(/project/Stages.find("testing")/colour),
                                  amount=$(/project/Items.filter(|x| x/stage == "testing").count()))
                    plot parts (label="parts", color=$(/project/Stages.find("parts")/colour),
                                amount=$(/project/Items.filter(|x| x/stage == "parts").count()))
                    plot later (label="later", color=$(/project/Stages.find("later")/colour),
                                amount=$(/project/Items.filter(|x| x/stage == "later").count()))
                }
            }

            <StatCard> {
                text complexities_header (markdown=true) = "**Open items by thinking**"
                chart complexities (kind="pie", height=180px) {
                    plot unassigned (label="unassigned", color=$(/project/Complexities.find("unassigned")/colour),
                                     amount=$(/project/Items.filter(|x| x/complexity == "unassigned").count()))
                    plot trivial (label="trivial", color=$(/project/Complexities.find("trivial")/colour),
                                  amount=$(/project/Items.filter(|x| x/complexity == "trivial").count()))
                    plot low (label="low", color=$(/project/Complexities.find("low")/colour),
                              amount=$(/project/Items.filter(|x| x/complexity == "low").count()))
                    plot medium (label="medium", color=$(/project/Complexities.find("medium")/colour),
                                 amount=$(/project/Items.filter(|x| x/complexity == "medium").count()))
                    plot high (label="high", color=$(/project/Complexities.find("high")/colour),
                               amount=$(/project/Items.filter(|x| x/complexity == "high").count()))
                }
            }

            <StatCard> {
                text owners_header (markdown=true) = "**Open items by owner**"
                chart owners (kind="pie", height=180px) {
                    plot each (source=$(/project/people/People), label=$(|p| p/name == "" ? p/tag : p/name), color=$(|p| p/colour),
                               amount=$(|p| /project/Items.filter(|x| x/owner == p/tag).count()))
                    plot nobody (label="nobody", color="#6b7280", amount=$(/project/Items.filter(|x| x/owner == "").count()))
                }
            }

            <StatCard> {
                text last_authors_header (markdown=true) = "**Open items by who wrote last**"
                chart last_authors (kind="pie", height=180px) {
                    plot each (source=$(/project/people/People), label=$(|p| p/name == "" ? p/tag : p/name), color=$(|p| p/colour),
                               amount=$(|p| /project/Items.filter(|x| x/last_author == p/tag).count()))
                    plot agents (label="agents", color="#6b7280",
                                 amount=$(/project/Items.filter(|x| x/last_author != ""
                                     && /project/people/People.filter(|p| p/tag == x/last_author).count() == 0).count()))
                }
            }
        }

        // A closed item's last author is worked out in each chain rather than kept in a field of
        // its own: a field worked out by a formula would have every part of History read.
        text closed_header (markdown=true) = "### <icon=archive> Closed work"
        div (layout="flow", min-width=280px, spacing=12) {
            <StatCard> {
                text statuses_header (markdown=true) = "**Closed items by status**"
                chart statuses (kind="pie", height=180px) {
                    plot each (source=$(/project/Statuses), label=$(|s| s/name == "" ? s/tag : s/name), color=$(|s| s/colour),
                               amount=$(|s| /project/History.filter(|x| x/status == s/tag).count()))
                }
            }

            <StatCard> {
                text closed_complexities_header (markdown=true) = "**Finished items by thinking**"
                chart closed_complexities (kind="pie", height=180px) {
                    plot each (source=$(/project/Complexities), label=$(|c| c/name == "" ? c/tag : c/name), color=$(|c| c/colour),
                               amount=$(|c| /project/History.filter(|x| x/status != "cancelled" && x/complexity == c/tag).count()))
                }
            }

            <StatCard> {
                text closed_owners_header (markdown=true) = "**Finished items by owner**"
                chart closed_owners (kind="pie", height=180px) {
                    plot each (source=$(/project/people/People), label=$(|p| p/name == "" ? p/tag : p/name), color=$(|p| p/colour),
                               amount=$(|p| /project/History.filter(|x| x/status != "cancelled" && x/owner == p/tag).count()))
                    plot nobody (label="nobody", color="#6b7280",
                                 amount=$(/project/History.filter(|x| x/status != "cancelled" && x/owner == "").count()))
                }
            }

            <StatCard> {
                text closed_last_authors_header (markdown=true) = "**Finished items by who wrote last**"
                chart closed_last_authors (kind="pie", height=180px) {
                    plot each (source=$(/project/people/People), label=$(|p| p/name == "" ? p/tag : p/name), color=$(|p| p/colour),
                               amount=$(|p| /project/History.filter(|x| x/status != "cancelled"
                                   && x/Talk/Comments.reduce("", |a, c| c/author) == p/tag).count()))
                    plot agents (label="agents", color="#6b7280",
                                 amount=$(/project/History.filter(|x| x/status != "cancelled"
                                     && x/Talk/Comments.reduce("", |a, c| c/author) != "").count()
                                     - /project/people/People.sum(|p| /project/History.filter(|x| x/status != "cancelled"
                                         && x/Talk/Comments.reduce("", |a, c| c/author) == p/tag).count())))
                }
            }
        }

        div (hidden=true) {
            int week_0 = $(/project/History.filter(|x| x/status != "cancelled" && week(x/finished_at) == week(today())).sum(|x| x/points))
            int week_1 = $(/project/History.filter(|x| x/status != "cancelled" && week(x/finished_at) == week(date_add_days(today(), -7))).sum(|x| x/points))
            int week_2 = $(/project/History.filter(|x| x/status != "cancelled" && week(x/finished_at) == week(date_add_days(today(), -14))).sum(|x| x/points))
            int week_3 = $(/project/History.filter(|x| x/status != "cancelled" && week(x/finished_at) == week(date_add_days(today(), -21))).sum(|x| x/points))
            int week_4 = $(/project/History.filter(|x| x/status != "cancelled" && week(x/finished_at) == week(date_add_days(today(), -28))).sum(|x| x/points))
            int week_5 = $(/project/History.filter(|x| x/status != "cancelled" && week(x/finished_at) == week(date_add_days(today(), -35))).sum(|x| x/points))
            int week_6 = $(/project/History.filter(|x| x/status != "cancelled" && week(x/finished_at) == week(date_add_days(today(), -42))).sum(|x| x/points))
            int week_7 = $(/project/History.filter(|x| x/status != "cancelled" && week(x/finished_at) == week(date_add_days(today(), -49))).sum(|x| x/points))
            int best_recent = $(week_0 >= week_1 && week_0 >= week_2 && week_0 >= week_3 ? week_0
                : week_1 >= week_2 && week_1 >= week_3 ? week_1 : week_2 >= week_3 ? week_2 : week_3)
            int best_earlier = $(week_4 >= week_5 && week_4 >= week_6 && week_4 >= week_7 ? week_4
                : week_5 >= week_6 && week_5 >= week_7 ? week_5 : week_6 >= week_7 ? week_6 : week_7)
            int best_week = $(best_recent >= best_earlier ? best_recent : best_earlier)
            int busiest_day = $(Activity.cells().map(|d| d/points).max())
            timestamp work_since = $(date_add_days(today(), -29))
            calendar WorkDays (entry=<WorkDay>, from=$(date_add_days(today(), -29)), to=$(today())) { }
            float work_minutes = $(/project/Items.sum(|x| x/Agents/Work.sum(|r| r/minutes))
                + /project/History.sum(|x| x/Agents/Work.sum(|r| r/minutes)))
            float work_minutes_month = $(WorkDays.cells().sum(|d| d/minutes) + /project/Items.sum(|x|
                x/Agents/Work.filter(|r| millis_since_epoch(r/at) >= millis_since_epoch(/project/work_since)).sum(|r| r/minutes)))
        }

        text pace_header (markdown=true) = "### <icon=trending-up> Pace"
        <StatCard> {
            text weeks_header (markdown=true) = "**Points by week, against the best**"
            chart weeks (kind="bar", height=180px) {
                plot w7 (label=$(week(date_add_days(today(), -49))), amount=$(week_7), limit=$(best_week))
                plot w6 (label=$(week(date_add_days(today(), -42))), amount=$(week_6), limit=$(best_week))
                plot w5 (label=$(week(date_add_days(today(), -35))), amount=$(week_5), limit=$(best_week))
                plot w4 (label=$(week(date_add_days(today(), -28))), amount=$(week_4), limit=$(best_week))
                plot w3 (label=$(week(date_add_days(today(), -21))), amount=$(week_3), limit=$(best_week))
                plot w2 (label=$(week(date_add_days(today(), -14))), amount=$(week_2), limit=$(best_week))
                plot w1 (label=$(week(date_add_days(today(), -7))), amount=$(week_1), limit=$(best_week))
                plot w0 (label=$(week(today())), amount=$(week_0), limit=$(best_week))
            }
        }

        <StatCard> {
            text activity_header (markdown=true) = "**Points by day, the last year**"
            calendar Activity (entry=<ActivityDay>, from=$(date_add_days(today(), -364)), to=$(today()), layout="year") { }
        }

        text work_header (markdown=true) = "### <icon=bot> What the agents' work cost"
        text work_all_header (markdown=true) = "**All told**"
        div (layout="flow", min-width=140px, spacing=12) {
            <StatTile> {
                int work_sessions (label="sessions", font-size=26px) = $(/project/Items.sum(|x| x/Agents/Work.count())
                    + /project/History.sum(|x| x/Agents/Work.count()))
            }
            <StatTile> {
                float work_cost (label="spent", font-size=26px, prefix="$", precision=2, font-color=$(theme/accent)) = $(/project/Items.sum(|x| x/Agents/Work.sum(|r| r/cost))
                    + /project/History.sum(|x| x/Agents/Work.sum(|r| r/cost)))
            }
            <StatTile> {
                int work_output (label="output", font-size=26px, suffix="k") = $(/project/Items.sum(|x| x/Agents/Work.sum(|r| r/output))
                    + /project/History.sum(|x| x/Agents/Work.sum(|r| r/output)))
            }
            <StatTile> {
                int work_read (label="read", font-size=26px, suffix="k") = $(/project/Items.sum(|x| x/Agents/Work.sum(|r| r/read))
                    + /project/History.sum(|x| x/Agents/Work.sum(|r| r/read)))
            }
            <StatTile> {
                int work_written (label="written", font-size=26px, suffix="k") = $(/project/Items.sum(|x| x/Agents/Work.sum(|r| r/written))
                    + /project/History.sum(|x| x/Agents/Work.sum(|r| r/written)))
            }
            <StatTile> {
                float work_hours (label="time", font-size=26px, suffix=" h", precision=1) = $(work_minutes / 60)
            }
        }

        text work_month_header (markdown=true) = "**The last thirty days**"
        div (layout="flow", min-width=140px, spacing=12) {
            <StatTile> {
                int work_sessions_month (label="sessions", font-size=26px) = $(WorkDays.cells().sum(|d| d/sessions) + /project/Items.sum(|x|
                    x/Agents/Work.filter(|r| millis_since_epoch(r/at) >= millis_since_epoch(/project/work_since)).count()))
            }
            <StatTile> {
                float work_cost_month (label="spent", font-size=26px, prefix="$", precision=2, font-color=$(theme/accent)) = $(WorkDays.cells().sum(|d| d/cost) + /project/Items.sum(|x|
                    x/Agents/Work.filter(|r| millis_since_epoch(r/at) >= millis_since_epoch(/project/work_since)).sum(|r| r/cost)))
            }
            <StatTile> {
                int work_output_month (label="output", font-size=26px, suffix="k") = $(WorkDays.cells().sum(|d| d/output) + /project/Items.sum(|x|
                    x/Agents/Work.filter(|r| millis_since_epoch(r/at) >= millis_since_epoch(/project/work_since)).sum(|r| r/output)))
            }
            <StatTile> {
                int work_read_month (label="read", font-size=26px, suffix="k") = $(WorkDays.cells().sum(|d| d/read) + /project/Items.sum(|x|
                    x/Agents/Work.filter(|r| millis_since_epoch(r/at) >= millis_since_epoch(/project/work_since)).sum(|r| r/read)))
            }
            <StatTile> {
                int work_written_month (label="written", font-size=26px, suffix="k") = $(WorkDays.cells().sum(|d| d/written) + /project/Items.sum(|x|
                    x/Agents/Work.filter(|r| millis_since_epoch(r/at) >= millis_since_epoch(/project/work_since)).sum(|r| r/written)))
            }
            <StatTile> {
                float work_hours_month (label="time", font-size=26px, suffix=" h", precision=1) = $(work_minutes_month / 60)
            }
        }

        text tags_section_header (markdown=true) = "### <icon=tags> Tags and milestones"
        <StatCard> {
            text tags_header (markdown=true) = "**How much of each tag is done**"
            chart tag_progress (kind="bar", height=220px, width=100%) {
                plot each (source=$(/project/Labels), label=$(|t| t/name == "" ? t/tag : t/name), color=$(|t| t/colour),
                           amount=$(|t| t/points_done), limit=$(|t| t/points_total), suffix=" pts")
            }
        }

        div (layout="flow", min-width=280px, spacing=12) {
            <StatCard> {
                text milestones_progress_header (markdown=true) = "**How far each milestone is**"
                chart milestone_progress (kind="bar", height=180px) {
                    plot each (source=$(/project/Milestones), label=$(|t| t/name == "" ? t/tag : t/name), color=$(|t| t/colour),
                               amount=$(|t| t/points_done), limit=$(|t| t/points_total), suffix=" pts")
                }
            }

            <StatCard> {
                text tags_done_header (markdown=true) = "**Points finished by tag**"
                chart tag_done (kind="pie", height=180px) {
                    plot each (source=$(/project/Labels), label=$(|t| t/name == "" ? t/tag : t/name), color=$(|t| t/colour),
                               amount=$(|t| t/points_done), suffix=" pts")
                }
            }

            <StatCard> {
                text tags_cost_header (markdown=true) = "**$ spent by tag**"
                chart tag_cost (kind="pie", height=180px) {
                    plot each (source=$(/project/Labels), label=$(|t| t/name == "" ? t/tag : t/name), color=$(|t| t/colour),
                               amount=$(|t| t/work_cost), suffix=" $")
                }
            }
        }
    }

    tab (label="Definition", hover-text="What the project is, its tags and milestones, and how the tracker works") {
        mount definition (source="definition.os/definition", mutable=false) { }

        text labels_header (markdown=true) = "## Tags"

        list Labels (entry=<Tag>, key="tag", layout="horizontal", spacing=6, border-style=none, padding=0) { }

        div NewTag (layout="horizontal", margin=0, spacing=6, alignment="center", border-style=none, padding=0) {
            textbox tag (label="", placeholder="tag", width=120px) = ""
            textbox name (label="", placeholder="shown as", width=200px) = ""
            textbox colour (label="", kind="color", width=56px) = "#6b7280"
            button add (icon="plus", label="tag", margin=0) {
                on click {
                    if (cond=$(../tag == "")) {
                        refuse = "say a tag"
                    }
                    if (cond=$(/project/Labels.filter(|x| x/tag == ../tag).count() > 0)) {
                        refuse = "this tag is there already"
                    }
                    append (list="/project/Labels", from="..")
                }
            }
        }

        text milestones_header (markdown=true) = "## Milestones"

        list Milestones (entry=<Milestone>, key="tag", layout="horizontal", spacing=6, border-style=none, padding=0) { }

        div NewMilestone (layout="horizontal", margin=0, spacing=6, alignment="center", border-style=none, padding=0) {
            textbox tag (label="", placeholder="milestone", width=120px) = ""
            textbox name (label="", placeholder="shown as", width=200px) = ""
            textbox colour (label="", kind="color", width=56px) = "#6b7280"
            button add (icon="plus", label="milestone", margin=0) {
                on click {
                    if (cond=$(../tag == "")) {
                        refuse = "say a milestone"
                    }
                    if (cond=$(/project/Milestones.filter(|x| x/tag == ../tag).count() > 0)) {
                        refuse = "this milestone is there already"
                    }
                    append (list="/project/Milestones", from="..")
                }
            }
        }

        text help (markdown=true, font-size=14px, mutable=false) = "**Items:** the open list is in the owner's order: new items go on top, and an item can be dragged to another place. A pinned item stays on top, where it cannot be dragged, and the rest are dragged among themselves below it. A heading with a sort icon sorts the list by its column while the page is open, without changing the file, pinned items still first; a sorted list cannot be dragged. A larger piece of work is the parent of smaller ones, which stand under it, indented, and fold away under its chevron; an item dragged carries its parts with it, and carried right or left as it is let go it goes under another item or out from under one; a parent reads how much of its parts is done; points say how big each is, 1 the smallest thing worth writing down and 8 most of a day; think says how much thinking building it takes, from trivial, straightforward however much of it, through low and medium to high, which wants a good understanding of several parts of the project - unassigned until it is ready; after names the items one waits on, and it is drawn grey while any of them is still open; by names whose item it is - whoever added it, and for an item an agent filed, the owner of the item it came from. for names who is to work it, usually a bot account; either can be emptied again from its picker. An item an agent loop is working on has a green border and a turning green icon by its id, and a line over it naming the account at work, as its chip, and the model it runs; other loops leave it alone. **Stages:** an item starts filed; planning means its plan is being worked out; ready that nothing is left to decide; testing that it is done and waits to be confirmed; parts that nothing is left on it but its parts, and once the last of them is finished or cancelled it moves to testing by itself; later that it is decided against for now. Picking finished moves it to Closed, from where it reaches the task history. Picking cancelled moves it there too, marked cancelled: it counts as neither work done nor an open part of its parent, and never reaches the task history. Neither can be picked while any part of the item is still open. **Flags:** the same in every project, unlike the tags. manual marks an item for a person to work, which agents leave alone - an item added here starts with it, and taking it off hands the item to them; auto one an agent filed of its own accord; first one to take before anything else, whatever its stage; asked one whose note holds a question for the owner - once it is answered there, the flag comes off and work goes on; discuss one the owner wants an opinion on before anything is decided, which an agent gives in the note, moving the item at most to planning and flagging it asked. trusted marks one the owner trusts to be done right: the agent loop finishes it as soon as it reaches testing, with nobody confirming it. **Milestones:** the releases or goals an item counts towards, picked from the project's list of them as tags are, and drawn leaning so they read apart from the tags beside them. **Discussion:** anything to talk over that is neither an item nor a feature - a question, an idea not yet a plan, a note for everyone. A topic has an id, whose it is, a title and its comments; it is pinned and dropped as an item is, and its id is never one an item has or had, so a chip naming it is never ambiguous. **Features:** what the project could become, kept apart from the plan: each is a card with a handle in capitals, so it is never taken for an item's, a line saying what it is, its description, pros and cons, and the features it needs, excludes or goes with - the link button on the card opens their pickers, and those that are set stay on the card - which the area draws as lines between the cards: an arrow to a feature it needs, a red dashed line to one it excludes, a green one to one it goes with. Its standing - proposed, wanted, accepted or discarded - colours the card. Cards are placed by dragging them anywhere on the area, so features that belong together can stand together; a new one starts at the area's top left corner, to be dragged where it belongs. Text names a feature as <#HANDLE>, which draws its chip. Text names a person as <@username>, which draws their chip, and an item or a topic as <#handle>. **Stats:** the project added up, worked out from the lists and writing nothing: how many tasks and points there are, open and finished, and the share finished; what holds open items up - other items they wait on, a question to the owner, a check at testing; the open items by stage and by how much thinking they take; the open items by owner, and by who wrote the last comment on them, where every comment not signed by a person counts as the agents'; the closed items by status, finished or cancelled, and the finished ones by thinking, by owner and by who wrote last, counted the same way; the points finished in each of the last eight weeks, against the best of them; the points finished on each of the last year's days, the darker the more; what the agents' work on the project cost - its sessions, the dollars spent, the thousands of tokens written out, read from the cache and written to it, and the hours worked - all told and in the last thirty days, where the thirty days count the runs on open items made in them and every run on an item finished in them; and how much of each tag is done, its points finished out of all its points, open and finished - an item with several tags counts in each; how far each milestone is, the same way; and the points finished by tag, where an item with several tags counts in each slice, so a slice is that tag's share of the tagged work rather than of all the points; and the dollars spent by tag, open and closed items alike, counted the same way. **Tabs:** the plan has two pages, Open, what is still to do, and Closed, what was finished or cancelled; Discussion is for what is neither an item nor a feature; Features is what the project could become; Stats is the project added up; Definition is what the project is, as the document beside the tracker says it, the tags and milestones items are given, where new ones are added, and this help. A tab beside the plan is the project's own."
    }
}
