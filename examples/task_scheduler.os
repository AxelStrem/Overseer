tab Tasks {
    div {
        div ActiveTask (layout="horizontal", background-color=$(
            effective_priority > 100 ? "#320505ff" :
            (effective_priority > 50 ? "#2c2920ff" : "inherit")
            ), spacing=0, padding=6) {
            int id (hidden=true) = 0
            int rid (hidden=true) = 0
            string description (label="", margin=0, font-size=18px) = ""
            string comment (margin=0, label="") = ""
            int points (label="Points", margin=0) = 1
            int priority (margin=0, hidden=true) = 0
            int priority_gain (label="Daily Gain", margin=0) = 0
            timestamp task_created (hidden=true, label="Since", margin=0, mode="elapsed") = $(now())
            int effective_priority (label="Priority") = $(priority + priority_gain * days_since(task_created))
            button Done (margin=0, label="Done") {
                on click {
                    set (mode="value", path="/Tasks/CompletedSample/description") = $(../description)
                    set (path="/Tasks/CompletedSample/points", mode="value") = $(../points)
                    set_now_ts (path="/Tasks/CompletedSample/completed_at")
                    append (template="<CompletedSample>", list="/Tasks/Completed")
                    remove (keyField="id", from="/Tasks/Active", keyValue=$(../id))
                }
            }
        }
        div CompletedRecord (padding=4, spacing=8, layout="horizontal") {
            string description (margin=0, label="") = ""
            int points (label="", margin=0) = 1
            timestamp completed_at (mode="elapsed", format="long", label="Completed", margin=0)
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
            int priority_gain (label="Daily Gain", margin=0) = 0
            string mode (label="Mode", margin=0) = "interval"
            string interval (label="Every", margin=0) = "7d"
            checkbox pause_when_active (label="Pause When Active", margin=0) = true
            timestamp last_triggered_at (mode="elapsed", label="Since", margin=0) = $(now())
            timer generator (at=$(../last_triggered_at), active=$(mode == "interval"), offset=$(interval)) {
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
                    set (path=".active", mode="value") = $( !../pause_when_active )
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
        int next_id = 8
    }
    div NewTask (spacing=6, padding=6, layout="horizontal") {
        string description (margin=0, label="Task") = ""
        string commentary (label="Comment", margin=0) = ""
        int points (margin=0, label="Points") = 1
        int priority (margin=0, label="Priority") = 0
        int priority_gain (label="Daily Gain", margin=0) = 0
        button Create (margin=0, label="Create Task") {
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
                set (path="/Tasks/NewTask/commentary", mode="value") = ""
                set (path="/Tasks/NewTask/points", mode="value") = 1
                set (path="/Tasks/NewTask/priority", mode="value") = 0
                set (mode="value", path="/Tasks/NewTask/priority_gain") = 0
            }
        }
    }
    list Active (key="id", sort_by=$(|x| 0 - x/effective_priority), layout="vertical", entry=<ActiveTask>, spacing=6) {
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
            int id = 6
            int rid = 0
            string description = "T2"
            string comment = "xx"
            int points = 1
            int priority = 0
            int priority_gain = 0
            timestamp task_created = "2025-08-15T12:43:18.145524500+00:00"
        }
        - {
            int id = 7
            string description = "Test"
            timestamp task_created = "2025-08-15T13:11:47.686693800+00:00"
        }
    }
    list Completed (entry=<CompletedRecord>, layout="vertical", spacing=4) {
        - {
        }
    }
    list Recurring (spacing=8, entry=<RecurringTask>, key="rid", layout="vertical") {
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
