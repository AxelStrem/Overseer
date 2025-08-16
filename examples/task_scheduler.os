tab Tasks {
    div (hidden=true) {
        div ActiveTask (background-color=$(
            effective_priority > 100 ? "#320505ff" :
            (effective_priority > 50 ? "#2c2920ff" : "inherit")
            ), spacing=0, layout="horizontal", padding=6) {
            int id (hidden=true) = 0
            int rid (hidden=true) = 0
            string description (margin=0, font-size=18px, label="") = ""
            string comment (label="", margin=0) = ""
            int points (margin=0, label="Points") = 1
            int priority (hidden=true, margin=0) = 0
            int priority_gain (margin=0, label="Daily Gain") = 0
            timestamp task_created (hidden=true, mode="elapsed", margin=0, label="Since") = $(now())
            int effective_priority (label="Priority") = $(priority + priority_gain * days_since(task_created))
            button Done (label="Done", margin=0) {
                on click {
                    append (template="<CompletedRecord>", list="/Tasks/Completed") {
                        - description = $(../description)
                        - points = $(../points)
                        - completed_at = $(now())
                    }
                    remove (keyValue=$(../id), from="/Tasks/Active", keyField="id")
                }
            }
        }
        div CompletedRecord (layout="horizontal", spacing=8, padding=4) {
            string description (margin=0, label="") = ""
            int points (margin=0, label="") = 1
            timestamp completed_at (format="long", margin=0, label="Completed")
        }
        <ActiveTask> ActiveSample {
        }
        <CompletedRecord> CompletedSample {
        }
        div RecurringTask (padding=6, layout="horizontal", spacing=4) {
            int rid (label="Recurrence Id", margin=0) = 1
            string description (margin=0, label="Task") = ""
            string comment (margin=0, label="Comment") = ""
            int points (margin=0, label="Points") = 1
            int base_priority (label="Base Priority", margin=0) = 0
            int priority_gain (margin=0, label="Daily Gain") = 0
            string mode (margin=0, label="Mode") = "interval"
            string interval (label="Every", margin=0) = "7d"
            checkbox pause_when_active (label="Pause When Active", margin=0) = true
            timestamp last_triggered_at (label="Since", margin=0, mode="elapsed") = $(now())
            timer generator (offset=$(interval), at=$(../last_triggered_at), active=$(mode == "interval"), one_shot=$(../pause_when_active)) {
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
            button pause (margin=0, label="Pause") {
                on click {
                    set (mode="value", path="../generator.active") = false
                }
            }
        }
    }
    div State (hidden=true) {
        int next_id = 22
    }
    div NewTask (spacing=6, padding=6, layout="horizontal") {
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
                set (mode="value", path="/Tasks/NewTask/description") = ""
                set (mode="value", path="/Tasks/NewTask/commentary") = ""
                set (mode="value", path="/Tasks/NewTask/points") = 1
                set (mode="value", path="/Tasks/NewTask/priority") = 0
                set (path="/Tasks/NewTask/priority_gain", mode="value") = 0
            }
        }
    }
    list Active (layout="vertical", spacing=6, sort_by=$(|x| 0 - x/effective_priority), entry=<ActiveTask>, key="id") {
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
        - {
            int id = 15
            int rid = 6
            string description = "Drum practice"
            string comment = ""
            int points = 2
            int priority = 5
            int priority_gain = 0
            timestamp task_created = "2025-08-16T07:25:06.474571400+00:00"
        }
        - {
            int id = 16
            int rid = 7
            string description = "Guitar practice"
            string comment = "basic riffs"
            int points = 2
            int priority = 3
            int priority_gain = 0
            timestamp task_created = "2025-08-16T07:25:06.477242900+00:00"
        }
        - {
            int id = 17
            int rid = 8
            string description = "Work on project: VD Roguelike"
            string comment = ""
            int points = 5
            int priority = 8
            int priority_gain = 0
            timestamp task_created = "2025-08-16T07:25:06.479807300+00:00"
        }
        - {
            int id = 18
            int rid = 9
            string description = "Work on project: Stream"
            string comment = ""
            int points = 5
            int priority = 8
            int priority_gain = 0
            timestamp task_created = "2025-08-16T07:25:06.482365400+00:00"
        }
        - {
            int id = 19
            int rid = 10
            string description = "Work on project: Overseer"
            string comment = ""
            int points = 5
            int priority = 8
            int priority_gain = 0
            timestamp task_created = "2025-08-16T07:25:06.484988600+00:00"
        }
        - {
            int id = 20
            int rid = 11
            string description = "Work on project: Godot Line3D"
            string comment = ""
            int points = 5
            int priority = 8
            int priority_gain = 0
            timestamp task_created = "2025-08-16T07:25:06.487650500+00:00"
        }
        - {
            int id = 21
            int rid = 17
            string description = "Motoric practice"
            string comment = ""
            int points = 1
            int priority = 3
            int priority_gain = 0
            timestamp task_created = "2025-08-16T07:25:06.490467500+00:00"
        }
    }
    list Completed (entry=<CompletedRecord>, spacing=4, layout="vertical") {
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
        - {
            string description = "Morning routine"
            int points = 1
            timestamp completed_at = "2025-08-16T07:25:35.201689200+00:00"
        }
    }
    list Recurring (entry=<RecurringTask>, spacing=8, key="rid", layout="vertical") {
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
            timestamp last_triggered_at = "2025-08-16T07:25:06.475433500+00:00"
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
            timestamp last_triggered_at = "2025-08-16T07:25:06.478066900+00:00"
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
            timestamp last_triggered_at = "2025-08-16T07:25:06.480604900+00:00"
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
            timestamp last_triggered_at = "2025-08-16T07:25:06.483164900+00:00"
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
            timestamp last_triggered_at = "2025-08-16T07:25:06.485858300+00:00"
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
            timestamp last_triggered_at = "2025-08-16T07:25:06.488520700+00:00"
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
            timestamp last_triggered_at = "2025-08-16T07:25:06.491367700+00:00"
        }
    }
}
