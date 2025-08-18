tab Tasks {
    div (hidden=true) {
        div ActiveTask (background-color=$(
            effective_priority > 100 ? "#320505ff" :
            (effective_priority > 50 ? "#2c2920ff" : "inherit")
            ), padding=6, spacing=0, layout="horizontal") {
            int id (hidden=true) = 0
            int rid (hidden=true) = 0
            string description (label="", font-size=18px, margin=0) = ""
            string comment (margin=0, label="") = ""
            int points (label="Points", margin=0) = 1
            int priority (margin=0, hidden=true) = 0
            int priority_gain (margin=0, label="Daily Gain") = 0
            timestamp task_created (margin=0, hidden=true, mode="elapsed", label="Since") = $(now())
            int effective_priority (label="Priority") = $(priority + priority_gain * days_since(task_created))
            button Done (margin=0, label="Done") {
                on click {
                    append (list="/Tasks/Completed", template="<CompletedRecord>") {
                        - description = $(../description)
                        - points = $(../points)
                        - completed_at = $(now())
                    }
                    remove (from="/Tasks/Active", keyField="id", keyValue=$(../id))
                }
            }
        }
        div CompletedRecord (spacing=8, padding=4, layout="horizontal") {
            string description (label="", margin=0) = ""
            int points (margin=0, label="") = 1
            timestamp completed_at (format="long", margin=0, label="Completed")
        }
        <ActiveTask> ActiveSample {
        }
        <CompletedRecord> CompletedSample {
        }
        div RecurringTask (layout="horizontal", padding=6, spacing=4) {
            int rid (margin=0, label="Recurrence Id") = 1
            string description (label="Task", margin=0) = ""
            string comment (label="Comment", margin=0) = ""
            int points (margin=0, label="Points") = 1
            int base_priority (margin=0, label="Base Priority") = 0
            int priority_gain (margin=0, label="Daily Gain") = 0
            string mode (margin=0, label="Mode") = "interval"
            string interval (margin=0, label="Every") = "7d"
            checkbox pause_when_active (label="Pause When Active", margin=0) = true
            timestamp last_triggered_at (mode="elapsed", margin=0, label="Since") = $(now())
            timer generator (at=$(../last_triggered_at), active=$(mode == "interval"), offset=$(interval), one_shot=$(../pause_when_active)) {
                on timeout {
                    append (template="<ActiveTask>", list="/Tasks/Active") {
                        - description = $(../description)
                        - comment = $(../comment)
                        - points = $(../points)
                        - priority = $(../base_priority)
                        - priority_gain = $(../priority_gain)
                        - rid = $(../rid)
                        - id = $(/Tasks/State/next_id)
                        - task_created = $(now())
                    }
                    inc (path="/Tasks/State/next_id")
                    set_now_ts (path="../last_triggered_at")
                }
            }
            button start (label="Start / Resume", margin=0) {
                on click {
                    set_now_ts (path="../last_triggered_at")
                    set (path="../generator.active", mode="value") = true
                }
            }
            button pause (label="Pause", margin=0) {
                on click {
                    set (mode="value", path="../generator.active") = false
                }
            }
        }
    }
    div State (hidden=true) {
        int next_id = 48
    }
    div NewTask (padding=6, layout="horizontal", spacing=6) {
        string description (label="Task", margin=0) = ""
        string commentary (margin=0, label="Comment") = ""
        int points (label="Points", margin=0) = 1
        int priority (margin=0, label="Priority") = 0
        int priority_gain (margin=0, label="Daily Gain") = 0
        button Create (label="Create Task", margin=0) {
            on click {
                append (template="<ActiveSample>", list="/Tasks/Active") {
                    - description = $(../description)
                    - comment = $(../commentary)
                    - points = $(../points)
                    - priority = $(../priority)
                    - priority_gain = $(../priority_gain)
                    - rid = 0
                    - id = $(/Tasks/State/next_id)
                    - task_created = $(now())
                }
                inc (path="/Tasks/State/next_id")
                set (mode="value", path="/Tasks/NewTask/description") = ""
                set (mode="value", path="/Tasks/NewTask/commentary") = ""
                set (path="/Tasks/NewTask/points", mode="value") = 1
                set (mode="value", path="/Tasks/NewTask/priority") = 0
                set (mode="value", path="/Tasks/NewTask/priority_gain") = 0
            }
        }
    }
    list Active (layout="vertical", spacing=6, entry=<ActiveTask>, sort_by=$(|x| 0 - x/effective_priority), key="id") {
        - {
            - id = 2
            - description = "Clean the balcony"
            - points = 2
            - task_created = "2025-08-13T10:48:25.886134500+00:00"
        }
        - {
            - id = 4
            - description = "Move the plants to bigger pots"
            - comment = "strawberries, peppers, rosemary"
            - points = 12
            - task_created = "2025-08-15T10:00:32.753844700+00:00"
        }
        - {
            - id = 11
            - rid = 13
            - description = "Clean the kitchen sink + area"
            - comment = ""
            - points = 10
            - priority = 5
            - priority_gain = 1
            - task_created = "2025-08-15T18:26:39.002022400+00:00"
        }
        - {
            - id = 12
            - rid = 14
            - description = "Clean the kitchen table"
            - comment = ""
            - points = 5
            - priority = 5
            - priority_gain = 0
            - task_created = "2025-08-15T18:26:39.004381900+00:00"
        }
        - {
            - id = 13
            - rid = 15
            - description = "Vacuum the floors"
            - comment = ""
            - points = 5
            - priority = 5
            - priority_gain = 0
            - task_created = "2025-08-15T18:26:39.006659600+00:00"
        }
        - {
            - id = 14
            - rid = 16
            - description = "Wash the floors"
            - comment = ""
            - points = 10
            - priority = 5
            - priority_gain = 0
            - task_created = "2025-08-15T18:26:39.008926700+00:00"
        }
        - {
            - id = 17
            - rid = 8
            - description = "Work on project: VD Roguelike"
            - comment = ""
            - points = 5
            - priority = 8
            - priority_gain = 0
            - task_created = "2025-08-16T07:25:06.479807300+00:00"
        }
        - {
            - id = 18
            - rid = 9
            - description = "Work on project: Stream"
            - comment = ""
            - points = 5
            - priority = 8
            - priority_gain = 0
            - task_created = "2025-08-16T07:25:06.482365400+00:00"
        }
        - {
            - id = 20
            - rid = 11
            - description = "Work on project: Godot Line3D"
            - comment = ""
            - points = 5
            - priority = 8
            - priority_gain = 0
            - task_created = "2025-08-16T07:25:06.487650500+00:00"
        }
        - {
            - id = 22
            - description = "Verify the account"
            - comment = "(exchange)"
            - task_created = "2025-08-16T14:42:35.917470800+00:00"
            - effective_priority = $(priority + priority_gain * days_since(task_created))
        }
        - {
            - id = 23
            - description = "Start 3D treatment"
            - priority = 10
            - task_created = "2025-08-16T14:45:29.984214400+00:00"
        }
        - {
            - id = 24
            - description = "Practice and record some vocals"
            - priority = 10
            - task_created = "2025-08-16T18:26:00.088930300+00:00"
        }
        - {
            - id = 25
            - description = "Shop: Sportmaster.ge"
            - priority = 7
            - task_created = "2025-08-16T20:49:28.301884+00:00"
        }
        - {
            - id = 26
            - description = "Shop: Musicroom.ge"
            - priority = 8
            - task_created = "2025-08-16T20:49:45.297332200+00:00"
        }
        - {
            - id = 33
            - rid = 4
            - description = "Evening routine"
            - comment = "3D, BT, shower"
            - points = 1
            - priority = 10
            - priority_gain = 0
            - task_created = "2025-08-18T18:57:35.143192500+00:00"
        }
        - {
            - id = 36
            - rid = 5
            - description = "Language practice"
            - comment = "Duolingo, Anki, Speaking"
            - points = 2
            - priority = 10
            - priority_gain = 0
            - task_created = "2025-08-18T18:57:36.885703200+00:00"
        }
        - {
            - id = 40
            - rid = 7
            - description = "Guitar practice"
            - comment = "basic riffs"
            - points = 2
            - priority = 3
            - priority_gain = 0
            - task_created = "2025-08-18T18:57:39.464182500+00:00"
        }
        - {
            - id = 41
            - rid = 8
            - description = "Work on project: VD Roguelike"
            - comment = ""
            - points = 5
            - priority = 8
            - priority_gain = 0
            - task_created = "2025-08-18T18:57:40.155032700+00:00"
        }
        - {
            - id = 42
            - rid = 9
            - description = "Work on project: Stream"
            - comment = ""
            - points = 5
            - priority = 8
            - priority_gain = 0
            - task_created = "2025-08-18T18:57:40.865218+00:00"
        }
        - {
            - id = 43
            - rid = 10
            - description = "Work on project: Overseer"
            - comment = ""
            - points = 5
            - priority = 8
            - priority_gain = 0
            - task_created = "2025-08-18T18:57:41.603760+00:00"
        }
        - {
            - id = 44
            - rid = 10
            - description = "Work on project: Overseer"
            - comment = ""
            - points = 5
            - priority = 8
            - priority_gain = 0
            - task_created = "2025-08-18T18:57:43.728424100+00:00"
        }
        - {
            - id = 45
            - rid = 11
            - description = "Work on project: Godot Line3D"
            - comment = ""
            - points = 5
            - priority = 8
            - priority_gain = 0
            - task_created = "2025-08-18T18:57:44.494464300+00:00"
        }
        - {
            - id = 46
            - rid = 12
            - description = "Wash clothes"
            - comment = ""
            - points = 5
            - priority = 2
            - priority_gain = 0
            - task_created = "2025-08-18T18:57:45.279816900+00:00"
        }
        - {
            - id = 47
            - rid = 17
            - description = "Motoric practice"
            - comment = ""
            - points = 1
            - priority = 3
            - priority_gain = 0
            - task_created = "2025-08-18T18:57:46.093771800+00:00"
        }
    }
    list Completed (layout="vertical", entry=<CompletedRecord>, spacing=4) {
        - {
            - description = "T2"
            - points = 1
            - completed_at = "2025-08-15T16:56:40.197743400+00:00"
        }
        - {
            - description = "Test Task"
            - points = 1
            - completed_at = "2025-08-15T17:38:58.018145900+00:00"
        }
        - {
            - description = "Back up photos"
            - points = 2
            - completed_at = "2025-08-15T17:56:22.811095+00:00"
        }
        - {
            - description = "Morning routine"
            - points = 1
            - completed_at = "2025-08-16T07:25:35.201689200+00:00"
        }
        - {
            - description = "Work on project: Overseer"
            - points = 5
            - completed_at = "2025-08-16T14:41:00.745245400+00:00"
        }
        - {
            - description = "Drum practice"
            - points = 2
            - completed_at = "2025-08-16T18:24:03.447655200+00:00"
        }
        - {
            - description = "Guitar practice"
            - points = 2
            - completed_at = "2025-08-16T18:24:04.974514800+00:00"
        }
        - {
            - description = "Morning routine"
            - points = 1
            - completed_at = "2025-08-18T18:59:32.336328800+00:00"
        }
        - {
            - description = "Morning routine"
            - points = 1
            - completed_at = "2025-08-18T18:59:34.351401+00:00"
        }
        - {
            - description = "Morning routine"
            - points = 1
            - completed_at = "2025-08-18T18:59:36.004017100+00:00"
        }
        - {
            - description = "Evening routine"
            - points = 1
            - completed_at = "2025-08-18T18:59:37.874396900+00:00"
        }
        - {
            - description = "Evening routine"
            - points = 1
            - completed_at = "2025-08-18T18:59:39.540504900+00:00"
        }
        - {
            - description = "Back up photos"
            - points = 2
            - completed_at = "2025-08-18T18:59:42.296195300+00:00"
        }
        - {
            - description = "Language practice"
            - points = 2
            - completed_at = "2025-08-18T18:59:43.845835400+00:00"
        }
        - {
            - description = "Language practice"
            - points = 2
            - completed_at = "2025-08-18T18:59:45.392625700+00:00"
        }
        - {
            - description = "Guitar practice"
            - points = 2
            - completed_at = "2025-08-18T18:59:53.612770500+00:00"
        }
        - {
            - description = "Motoric practice"
            - points = 1
            - completed_at = "2025-08-18T18:59:55.137938200+00:00"
        }
        - {
            - description = "Drum practice"
            - points = 2
            - completed_at = "2025-08-18T18:59:56.688232+00:00"
        }
        - {
            - description = "Drum practice"
            - points = 2
            - completed_at = "2025-08-18T18:59:58.078806500+00:00"
        }
    }
    list Recurring (key="rid", layout="vertical", spacing=8, entry=<RecurringTask>) {
        - {
            - rid = 1
            - description = "Wash clothes"
            - comment = "Laundry day"
            - points = 1
            - base_priority = 0
            - priority_gain = 1
            - mode = "interval"
            - interval = "7d"
            - pause_when_active = true
            - last_triggered_at = "2025-08-13T10:48:25.886134500+00:00"
        }
        - {
            - rid = 2
            - description = "Back up photos"
            - comment = "External drive"
            - points = 2
            - base_priority = 10
            - priority_gain = 2
            - mode = "interval"
            - interval = "2d"
            - pause_when_active = false
            - last_triggered_at = "2025-08-18T18:57:32.473568900+00:00"
        }
        - {
            - rid = 3
            - description = "Morning routine"
            - comment = "3D, acid"
            - points = 1
            - base_priority = 10
            - mode = "interval"
            - interval = "1d"
            - pause_when_active = false
            - last_triggered_at = "2025-08-18T18:57:33.911434800+00:00"
        }
        - {
            - rid = 4
            - description = "Evening routine"
            - comment = "3D, BT, shower"
            - points = 1
            - base_priority = 10
            - mode = "interval"
            - interval = "1d"
            - pause_when_active = false
            - last_triggered_at = "2025-08-18T18:57:35.519518+00:00"
        }
        - {
            - rid = 5
            - description = "Language practice"
            - comment = "Duolingo, Anki, Speaking"
            - points = 2
            - base_priority = 10
            - mode = "interval"
            - interval = "1d"
            - pause_when_active = false
            - last_triggered_at = "2025-08-18T18:57:37.291904300+00:00"
        }
        - {
            - rid = 6
            - description = "Drum practice"
            - comment = ""
            - points = 2
            - base_priority = 5
            - mode = "interval"
            - interval = "1d"
            - pause_when_active = false
            - last_triggered_at = "2025-08-18T18:57:38.568178200+00:00"
        }
        - {
            - rid = 7
            - description = "Guitar practice"
            - comment = "basic riffs"
            - points = 2
            - base_priority = 3
            - mode = "interval"
            - interval = "1d"
            - pause_when_active = false
            - last_triggered_at = "2025-08-18T18:57:39.925705800+00:00"
        }
        - {
            - rid = 8
            - description = "Work on project: VD Roguelike"
            - comment = ""
            - points = 5
            - base_priority = 8
            - mode = "interval"
            - interval = "1d"
            - pause_when_active = true
            - last_triggered_at = "2025-08-18T18:57:40.626404900+00:00"
        }
        - {
            - rid = 9
            - description = "Work on project: Stream"
            - comment = ""
            - points = 5
            - base_priority = 8
            - mode = "interval"
            - interval = "1d"
            - pause_when_active = true
            - last_triggered_at = "2025-08-18T18:57:41.357437200+00:00"
        }
        - {
            - rid = 10
            - description = "Work on project: Overseer"
            - comment = ""
            - points = 5
            - base_priority = 8
            - mode = "interval"
            - interval = "1d"
            - pause_when_active = true
            - last_triggered_at = "2025-08-18T18:57:44.238395900+00:00"
        }
        - {
            - rid = 11
            - description = "Work on project: Godot Line3D"
            - comment = ""
            - points = 5
            - base_priority = 8
            - mode = "interval"
            - interval = "1d"
            - pause_when_active = true
            - last_triggered_at = "2025-08-18T18:57:45.013711+00:00"
        }
        - {
            - rid = 12
            - description = "Wash clothes"
            - comment = ""
            - points = 5
            - base_priority = 2
            - mode = "interval"
            - interval = "2d"
            - pause_when_active = true
            - last_triggered_at = "2025-08-18T18:57:45.826265900+00:00"
        }
        - {
            - rid = 13
            - description = "Clean the kitchen sink + area"
            - comment = ""
            - points = 10
            - base_priority = 5
            - mode = "interval"
            - interval = "14d"
            - pause_when_active = true
            - last_triggered_at = "2025-08-15T18:26:39.002714700+00:00"
        }
        - {
            - rid = 14
            - description = "Clean the kitchen table"
            - comment = ""
            - points = 5
            - base_priority = 5
            - mode = "interval"
            - interval = "5d"
            - pause_when_active = true
            - last_triggered_at = "2025-08-15T18:26:39.005073400+00:00"
        }
        - {
            - rid = 15
            - description = "Vacuum the floors"
            - comment = ""
            - points = 5
            - base_priority = 5
            - mode = "interval"
            - interval = "7d"
            - pause_when_active = true
            - last_triggered_at = "2025-08-15T18:26:39.007361100+00:00"
        }
        - {
            - rid = 16
            - description = "Wash the floors"
            - comment = ""
            - points = 10
            - base_priority = 5
            - mode = "interval"
            - interval = "14d"
            - pause_when_active = true
            - last_triggered_at = "2025-08-15T18:26:39.009665900+00:00"
        }
        - {
            - rid = 17
            - description = "Motoric practice"
            - comment = ""
            - points = 1
            - base_priority = 3
            - mode = "interval"
            - interval = "1d"
            - pause_when_active = true
            - last_triggered_at = "2025-08-18T18:57:46.645244500+00:00"
        }
    }
}
