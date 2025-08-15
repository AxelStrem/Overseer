tab Tasks {
    div {
        div ActiveTask (background-color=$(
        effective_priority > 100 ? "#320505ff" :
        (effective_priority > 50 ? "#2c2920ff" : "inherit")
        ), spacing=0, padding=6, layout="horizontal") {
            int id (hidden=true) = 0
            int rid (hidden=true) = 0
            string description (margin=0, label="", font-size=18px) = ""
            string comment (label="", margin=0) = ""
            int points (margin=0, label="Points") = 1
            int priority (hidden=true, margin=0) = 0
            int priority_gain (label="Daily Gain", margin=0) = 0
            timestamp task_created (label="Since", mode="elapsed", hidden=true, margin=0) = $(now())
            int effective_priority (label="Priority") = $(priority + priority_gain * days_since(task_created))
            button Done (label="Done", margin=0) {
                on click {
                    set (path="/Tasks/CompletedSample/description", mode="value") = $(../description)
                    set (mode="value", path="/Tasks/CompletedSample/points") = $(../points)
                    set_now_ts (path="/Tasks/CompletedSample/completed_at")
                    append (template="<CompletedSample>", list="/Tasks/Completed")
                    remove (keyField="id", from="/Tasks/Active", keyValue=$(../id))
                }
            }
        }
        div CompletedRecord (spacing=8, padding=4, layout="horizontal") {
            string description (label="", margin=0) = ""
            int points (label="", margin=0) = 1
            timestamp completed_at (format="long", label="Completed", mode="elapsed", margin=0)
        }
        <ActiveTask> ActiveSample {
        }
        <CompletedRecord> CompletedSample {
        }
        div RecurringTask (spacing=4, padding=6, layout="horizontal") {
            int rid (label="Recurrence Id", margin=0) = 1
            string description (label="Task", margin=0) = ""
            string comment (label="Comment", margin=0) = ""
            int points (margin=0, label="Points") = 1
            int base_priority (label="Base Priority", margin=0) = 0
            int priority_gain (label="Daily Gain", margin=0) = 0
            string mode (label="Mode", margin=0) = "interval"
            string interval (margin=0, label="Every") = "7d"
            checkbox pause_when_active (label="Pause When Active", margin=0) = true
            timestamp last_triggered_at (mode="elapsed", margin=0, label="Since") = $(now())
            timer generator (at=$(../last_triggered_at), active=$(mode == "interval"), offset=$(interval)) {
                on timeout {
                    append (template="<ActiveSample>", list="/Tasks/Active") {
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
                    set (mode="value", path="/Tasks/Recurring/generator/active") = $( !../pause_when_active )
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
                    set (path="/Tasks/Recurring/generator/active", mode="value") = false
                }
            }
        }
    }
    div State (hidden=true) {
        int next_id = 5
    }
    div NewTask (padding=6, layout="horizontal", spacing=6) {
        string description (label="Task", margin=0) = ""
        string commentary (margin=0, label="Comment") = ""
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
                set (mode="value", path="/Tasks/NewTask/priority_gain") = 0
            }
        }
    }
    list Active (spacing=6, sort_by=$(|x| 0 - x/effective_priority), layout="vertical", key="id", entry=<ActiveTask>) {
        - {
            int id = 2
            int rid = 0
            string description = "Clean the balcony"
            timestamp task_created = "2025-08-13T10:48:25.886134500+00:00"
        }
        - {
            int id = 4
            int rid = 0
            string description = "Move the plants to bigger pots"
            string comment = "strawberries, peppers, rosemary"
            int points = 10
            timestamp task_created = "2025-08-15T10:00:32.753844700+00:00"
        }
    }
    list Completed (layout="vertical", spacing=4, entry=<CompletedRecord>) {
        - {
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
            string interval = "30m"
            checkbox pause_when_active = false
            timestamp last_triggered_at = "2025-08-13T10:48:25.886134500+00:00"
        }
    }
}
