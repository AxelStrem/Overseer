tab Tasks {
    div {
        div ActiveTask (padding=6, background-color=$(
        effective_priority > 100 ? "#320505ff" :
        (effective_priority > 50 ? "#2c2920ff" : "inherit")
        ), layout="horizontal", spacing=0) {
            int id (hidden=true) = 0
            int rid (hidden=true) = 0
            string description (font-size=18px, label="", margin=0) = ""
            string comment (margin=0, label="") = ""
            int points (margin=0, label="Points") = 1
            int priority (hidden=true, margin=0) = 0
            int priority_gain (margin=0, label="Daily Gain") = 0
            timestamp task_created (margin=0, hidden=true, mode="elapsed", label="Since") = $(now())
            int effective_priority (label="Priority") = $(priority + priority_gain * days_since(task_created))
            button Done (label="Done", margin=0) {
                on click {
                    set (path="/Tasks/CompletedSample/description", mode="value") = $(../description)
                    set (mode="value", path="/Tasks/CompletedSample/points") = $(../points)
                    set_now_ts (path="/Tasks/CompletedSample/completed_at")
                    append (list="/Tasks/Completed", template="<CompletedSample>")
                    remove (keyField="id", from="/Tasks/Active", keyValue=$(../id))
                }
            }
        }
        div CompletedRecord (padding=4, spacing=8, layout="horizontal") {
            string description (label="", margin=0) = ""
            int points (margin=0, label="") = 1
            timestamp completed_at (format="long", label="Completed", margin=0, mode="elapsed")
        }
        <ActiveTask> ActiveSample {
        }
        <CompletedRecord> CompletedSample {
        }
        div RecurringTask (layout="horizontal", spacing=4, padding=6) {
            int rid (margin=0, label="Recurrence Id") = 1
            string description (label="Task", margin=0) = ""
            string comment (margin=0, label="Comment") = ""
            int points (margin=0, label="Points") = 1
            int base_priority (margin=0, label="Base Priority") = 0
            int priority_gain (label="Daily Gain", margin=0) = 0
            string mode (label="Mode", margin=0) = "interval"
            string interval (label="Every", margin=0) = "7d"
            checkbox pause_when_active (label="Pause When Active", margin=0) = true
            timestamp last_triggered_at (label="Since", margin=0, mode="elapsed") = $(now())
            timer generator (at=$(../last_triggered_at), active=$(mode == "interval"), offset=$(interval)) {
                on timeout {
                    append (list="/Tasks/Active", template="<ActiveSample>") {
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
                    set_now_ts (path="/Tasks/Recurring/last_triggered_at")
                    set (path="/Tasks/Recurring/generator/active", mode="value") = $( !../pause_when_active )
                }
            }
            button start (label="Start / Resume", margin=0) {
                on click {
                    set_now_ts (path="/Tasks/Recurring/last_triggered_at")
                    set (mode="value", path="/Tasks/Recurring/generator/active") = true
                }
            }
            button pause (label="Pause", margin=0) {
                on click {
                    set (mode="value", path="/Tasks/Recurring/generator/active") = false
                }
            }
        }
    }
    div State (hidden=true) {
        int next_id = 5
    }
    div NewTask (spacing=6, layout="horizontal", padding=6) {
        string description (label="Task", margin=0) = ""
        string commentary (margin=0, label="Comment") = ""
        int points (margin=0, label="Points") = 1
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
                set (path="/Tasks/NewTask/description", mode="value") = ""
                set (path="/Tasks/NewTask/commentary", mode="value") = ""
                set (mode="value", path="/Tasks/NewTask/points") = 1
                set (path="/Tasks/NewTask/priority", mode="value") = 0
                set (path="/Tasks/NewTask/priority_gain", mode="value") = 0
            }
        }
    }
    list Active (spacing=6, sort_by=$(|x| 0 - x/effective_priority), entry=<ActiveTask>, layout="vertical", key="id") {
        - {
            int id = 2
            int rid = 0
            string description = "Clean the balcony"
            int points = 2
            timestamp task_created = "2025-08-13T10:48:25.886134500+00:00"
        }
        - {
            int id = 4
            int rid = 0
            string description = "Move the plants to bigger pots"
            string comment = "strawberries, peppers, rosemary"
            int points = 12
            timestamp task_created = "2025-08-15T10:00:32.753844700+00:00"
        }
    }
    list Completed (spacing=4, entry=<CompletedRecord>, layout="vertical") {
        - {
        }
    }
    list Recurring (key="rid", spacing=8, layout="vertical", entry=<RecurringTask>) {
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
            string interval = "30m"
            checkbox pause_when_active = false
            timestamp last_triggered_at = "2025-08-13T10:48:25.886134500+00:00"
        }
    }
}
