tab Tasks {
    div (hidden=true) {
        div ActiveTask (background-color=$(
            effective_priority > 100 ? "#320505ff" :
            (effective_priority > 50 ? "#2c2920ff" : "inherit")
            ), padding=6, spacing=0, layout="vertical") {
            int id (hidden=true) = 0
            int rid (hidden=true) = 0
            string description (margin=0, label="", font-size=18px) = ""
            string comment (label="", margin=0) = ""
            int points (label="Points", margin=0) = 1
            int priority (hidden=true, margin=0) = 0
            int priority_gain (label="Daily Gain", margin=0) = 0
            timestamp task_created (margin=0, label="Since", mode="elapsed", hidden=true) = $(now())
            int effective_priority (label="Priority") = $(priority + priority_gain * days_since(task_created))
            button Done (label="Done", margin=0) {
                on click {
                    append (list="/Tasks/Completed", template="<CompletedRecord>") {
                        - description = $(../description)
                        - points = $(../points)
                        - completed_at = $(now())
                    }
                    remove (keyValue=$(../id), from="/Tasks/Active", keyField="id")
                }
            }
        }
        div CompletedRecord (spacing=8, layout="horizontal", padding=4) {
            string description (label="", margin=0) = ""
            int points (label="", margin=0) = 1
            timestamp completed_at (label="Completed", margin=0, format="long")
        }
        <ActiveTask> ActiveSample {
        }
        <CompletedRecord> CompletedSample {
        }
        div RecurringTask (layout="horizontal", spacing=4, padding=6) {
            int rid (label="Recurrence Id", margin=0) = 1
            string description (margin=0, label="Task") = ""
            string comment (label="Comment", margin=0) = ""
            int points (margin=0, label="Points") = 1
            int base_priority (label="Base Priority", margin=0) = 0
            int priority_gain (label="Daily Gain", margin=0) = 0
            string mode (margin=0, label="Mode") = "interval"
            string interval (margin=0, label="Every") = "7d"
            checkbox pause_when_active (margin=0, label="Pause When Active") = true
            timestamp last_triggered_at (margin=0, mode="elapsed", label="Since") = $(now())
            timer generator (active=$(mode == "interval"), offset=$(interval), one_shot=$(../pause_when_active), at=$(../last_triggered_at)) {
                on timeout {
                    append (list="/Tasks/Active", template="<ActiveTask>") {
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
            button start (margin=0, label="Start / Resume") {
                on click {
                    set_now_ts (path="../last_triggered_at")
                    set (mode="value", path="../generator.active") = true
                }
            }
            button pause (label="Pause", margin=0) {
                on click {
                    set (path="../generator.active", mode="value") = false
                }
            }
        }
    }
    div State (hidden=true) {
        int next_id = 15
    }
    div NewTask (layout="horizontal", spacing=6, padding=6) {
        string description (margin=0, label="Task") = ""
        string commentary (label="Comment", margin=0) = ""
        int points (label="Points", margin=0) = 1
        int priority (label="Priority", margin=0) = 0
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
                set (path="/Tasks/NewTask/description", mode="value") = ""
                set (mode="value", path="/Tasks/NewTask/commentary") = ""
                set (mode="value", path="/Tasks/NewTask/points") = 1
                set (mode="value", path="/Tasks/NewTask/priority") = 0
                set (path="/Tasks/NewTask/priority_gain", mode="value") = 0
            }
        }
    }
    list Active (sort_by=$(|x| 0 - x/effective_priority), spacing=6, layout="vertical", key="id", entry=<ActiveTask>) {
        - {
            int id = 2
            string description = "Clean the balcony"
            int points = 2
            timestamp task_created = "2025-08-13T10:48:25.886134500+00:00"
        }
        - {
            int id = 4
            string description = "Move the plants to bigger pots"
            string comment = "strawberries, peppers, rosemary"
            int points = 12
            timestamp task_created = "2025-08-15T10:00:32.753844700+00:00"
        }
        - {
            int id = 9
            int rid = 3
            string description = "Morning routine"
            string comment = "3D, acid"
            int points = 1
            int priority = 10
            int priority_gain = 0
            timestamp task_created = "2025-08-15T16:18:01.451736+00:00"
        }
        - {
            int id = 11
            int rid = 13
            string description = "Clean the kitchen sink + area"
            string comment = ""
            int points = 10
            int priority = 5
            int priority_gain = 0
            timestamp task_created = "2025-08-15T18:26:39.002022400+00:00"
        }
        - {
            int id = 12
            int rid = 14
            string description = "Clean the kitchen table"
            string comment = ""
            int points = 5
            int priority = 5
            int priority_gain = 0
            timestamp task_created = "2025-08-15T18:26:39.004381900+00:00"
        }
        - {
            int id = 13
            int rid = 15
            string description = "Vacuum the floors"
            string comment = ""
            int points = 5
            int priority = 5
            int priority_gain = 0
            timestamp task_created = "2025-08-15T18:26:39.006659600+00:00"
        }
        - {
            int id = 14
            int rid = 16
            string description = "Wash the floors"
            string comment = ""
            int points = 10
            int priority = 5
            int priority_gain = 0
            timestamp task_created = "2025-08-15T18:26:39.008926700+00:00"
        }
    }
    list Completed (entry=<CompletedRecord>, layout="vertical", spacing=4) {
        - {
            string description = "T2"
            int points = 1
            timestamp completed_at = "2025-08-15T16:56:40.197743400+00:00"
        }
        - {
            string description = "Test Task"
            int points = 1
            timestamp completed_at = "2025-08-15T17:38:58.018145900+00:00"
        }
        - {
            string description = "Back up photos"
            int points = 2
            timestamp completed_at = "2025-08-15T17:56:22.811095+00:00"
        }
    }
    list Recurring (spacing=8, key="rid", layout="vertical", entry=<RecurringTask>) {
        - {
            int rid = 1
            string description = "Wash clothes"
            string comment = "Laundry day"
            int points = 1
            int base_priority = 0
            int priority_gain = 1
            string mode = "interval"
            string interval = "7d"
            checkbox pause_when_active = true
            timestamp last_triggered_at = "2025-08-13T10:48:25.886134500+00:00"
        }
        - {
            int rid = 2
            string description = "Back up photos"
            string comment = "External drive"
            int points = 2
            int base_priority = 10
            int priority_gain = 2
            string mode = "interval"
            string interval = "2d"
            checkbox pause_when_active = false
            timestamp last_triggered_at = "2025-08-15T16:18:01.448014200+00:00"
        }
        - {
            int rid = 3
            string description = "Morning routine"
            string comment = "3D, acid"
            int points = 1
            int base_priority = 10
            string mode = "interval"
            string interval = "1d"
            checkbox pause_when_active = false
            timestamp last_triggered_at = "2025-08-15T16:18:01.454434900+00:00"
        }
        - {
            int rid = 4
            string description = "Evening routine"
            string comment = "3D, BT"
            int points = 1
            int base_priority = 10
            string mode = "interval"
            string interval = "1d"
            checkbox pause_when_active = false
            timestamp last_triggered_at = "2025-08-15T16:18:01.454434900+00:00"
        }
        - {
            int rid = 5
            string description = "Language practice"
            string comment = "Duolingo, Anki, Speaking"
            int points = 2
            int base_priority = 10
            string mode = "interval"
            string interval = "1d"
            checkbox pause_when_active = false
            timestamp last_triggered_at = "2025-08-15T16:18:01.454434900+00:00"
        }
        - {
            int rid = 6
            string description = "Drum practice"
            string comment = ""
            int points = 2
            int base_priority = 5
            string mode = "interval"
            string interval = "1d"
            checkbox pause_when_active = false
            timestamp last_triggered_at = "2025-08-15T00:00:00.0+00:00"
        }
        - {
            int rid = 7
            string description = "Guitar practice"
            string comment = "basic riffs"
            int points = 2
            int base_priority = 3
            string mode = "interval"
            string interval = "1d"
            checkbox pause_when_active = false
            timestamp last_triggered_at = "2025-08-15T00:00:00.0+00:00"
        }
        - {
            int rid = 8
            string description = "Work on project: VD Roguelike"
            string comment = ""
            int points = 5
            int base_priority = 8
            string mode = "interval"
            string interval = "1d"
            checkbox pause_when_active = true
            timestamp last_triggered_at = "2025-08-15T00:00:00.0+00:00"
        }
        - {
            int rid = 9
            string description = "Work on project: Stream"
            string comment = ""
            int points = 5
            int base_priority = 8
            string mode = "interval"
            string interval = "1d"
            checkbox pause_when_active = true
            timestamp last_triggered_at = "2025-08-15T00:00:00.0+00:00"
        }
        - {
            int rid = 10
            string description = "Work on project: Overseer"
            string comment = ""
            int points = 5
            int base_priority = 8
            string mode = "interval"
            string interval = "1d"
            checkbox pause_when_active = true
            timestamp last_triggered_at = "2025-08-15T00:00:00.0+00:00"
        }
        - {
            int rid = 11
            string description = "Work on project: Godot Line3D"
            string comment = ""
            int points = 5
            int base_priority = 8
            string mode = "interval"
            string interval = "1d"
            checkbox pause_when_active = true
            timestamp last_triggered_at = "2025-08-15T00:00:00.0+00:00"
        }
        - {
            int rid = 12
            string description = "Wash clothes"
            string comment = ""
            int points = 5
            int base_priority = 2
            string mode = "interval"
            string interval = "2d"
            checkbox pause_when_active = true
            timestamp last_triggered_at = "2025-08-15T00:00:00.0+00:00"
        }
        - {
            int rid = 13
            string description = "Clean the kitchen sink + area"
            string comment = ""
            int points = 10
            int base_priority = 5
            string mode = "interval"
            string interval = "14d"
            checkbox pause_when_active = true
            timestamp last_triggered_at = "2025-08-15T18:26:39.002714700+00:00"
        }
        - {
            int rid = 14
            string description = "Clean the kitchen table"
            string comment = ""
            int points = 5
            int base_priority = 5
            string mode = "interval"
            string interval = "5d"
            checkbox pause_when_active = true
            timestamp last_triggered_at = "2025-08-15T18:26:39.005073400+00:00"
        }
        - {
            int rid = 15
            string description = "Vacuum the floors"
            string comment = ""
            int points = 5
            int base_priority = 5
            string mode = "interval"
            string interval = "7d"
            checkbox pause_when_active = true
            timestamp last_triggered_at = "2025-08-15T18:26:39.007361100+00:00"
        }
        - {
            int rid = 16
            string description = "Wash the floors"
            string comment = ""
            int points = 10
            int base_priority = 5
            string mode = "interval"
            string interval = "14d"
            checkbox pause_when_active = true
            timestamp last_triggered_at = "2025-08-15T18:26:39.009665900+00:00"
        }
        - {
            int rid = 17
            string description = "Motoric practice"
            string comment = ""
            int points = 1
            int base_priority = 3
            string mode = "interval"
            string interval = "1d"
            checkbox pause_when_active = true
            timestamp last_triggered_at = "2025-08-15T00:00:00.009665900+00:00"
        }
    }
}
