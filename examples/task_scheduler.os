tab Tasks {
    div (hidden = true) {
        div ActiveTask (spacing=0, layout="horizontal", padding=6, background-color=$(
            effective_priority > 100 ? "#320505ff" :
            (effective_priority > 50 ? "#2c2920ff" : "inherit")
            )) {
            int id (hidden=true) = 0
            int rid (hidden=true) = 0
            string description (font-size=18px, label="", margin=0) = ""
            string comment (label="", margin=0) = ""
            int points (label="Points", margin=0) = 1
            int priority (margin=0, hidden=true) = 0
            int priority_gain (label="Daily Gain", margin=0) = 0
            timestamp task_created (hidden=true, label="Since", mode="elapsed", margin=0) = $(now())
            int effective_priority (label="Priority") = $(priority + priority_gain * days_since(task_created))
            button Done (label="Done", margin=0) {
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
        div CompletedRecord (padding=4, spacing=8, layout="horizontal") {
            string description (margin=0, label="") = ""
            int points (label="", margin=0) = 1
            timestamp completed_at (format="long", label="Completed", margin=0)
        }
        <ActiveTask> ActiveSample {
        }
        <CompletedRecord> CompletedSample {
        }
        div RecurringTask (spacing=4, padding=6, layout="horizontal") {
            int rid (margin=0, label="Recurrence Id") = 1
            string description (label="Task", margin=0) = ""
            string comment (margin=0, label="Comment") = ""
            int points (label="Points", margin=0) = 1
            int base_priority (label="Base Priority", margin=0) = 0
            int priority_gain (label="Daily Gain", margin=0) = 0
            string mode (label="Mode", margin=0) = "interval"
            string interval (margin=0, label="Every") = "7d"
            checkbox pause_when_active (margin=0, label="Pause When Active") = true
            timestamp last_triggered_at (margin=0, mode="elapsed", label="Since") = $(now())
            timer generator (active=$(mode == "interval"), one_shot=$(../pause_when_active), at=$(../last_triggered_at), offset=$(interval)) {
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
            button start (label="Start / Resume", margin=0) {
                on click {
                    set_now_ts (path="../last_triggered_at")
                    set (path="../generator.active", mode="value") = true
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
        int next_id = 11
    }
    div NewTask (layout="horizontal", spacing=6, padding=6) {
        string description (margin=0, label="Task") = ""
        string commentary (margin=0, label="Comment") = ""
        int points (margin=0, label="Points") = 1
        int priority (margin=0, label="Priority") = 0
        int priority_gain (label="Daily Gain", margin=0) = 0
        button Create (margin=0, label="Create Task") {
            on click {
                append (list="/Tasks/Active", template="<ActiveSample>") {
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
                set (path="/Tasks/NewTask/priority_gain", mode="value") = 0
            }
        }
    }
    list Active (entry=<ActiveTask>, layout="vertical", key="id", sort_by=$(|x| 0 - x/effective_priority), spacing=6) {
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
            int id = 5
            int rid = 0
            string description = "Test Task"
            string comment = ""
            int points = 1
            int priority = 0
            int priority_gain = 0
            timestamp task_created = "2025-08-15T12:38:17.231098300+00:00"
        }
        - {
            int id = 8
            int rid = 2
            string description = "Back up photos"
            string comment = "External drive"
            int points = 2
            int priority = 10
            int priority_gain = 2
            timestamp task_created = "2025-08-15T16:18:01.446964500+00:00"
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
    }
    list Completed (entry=<CompletedRecord>, layout="vertical", spacing=4) {
        - {
            string description = "T2"
            int points = 1
            timestamp completed_at = "2025-08-15T16:56:40.197743400+00:00"
        }
    }
    list Recurring (layout="vertical", entry=<RecurringTask>, key="rid", spacing=8) {
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
    }
}
