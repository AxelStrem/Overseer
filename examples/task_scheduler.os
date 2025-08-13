tab Tasks {
    div {
        div ActiveTask (padding=6, background-color=$(
        effective_priority > 100 ? "#320505ff" :
        (effective_priority > 50 ? "#2c2920ff" : "inherit")
        ), spacing=0, layout="horizontal") {
            int id (hidden=true) = 0
            int rid (hidden=true) = 0
            string description (margin=0, font-size=18px, label="") = ""
            string comment (label="", margin=0) = ""
            int points (label="Points", margin=0) = 1
            int priority (margin=0, label="Priority") = 0
            int priority_gain (margin=0, label="Daily Gain") = 0
            timestamp priority_start (mode="elapsed", margin=0, label="Since") = $(now())
            int effective_priority (hidden=true) = $(priority + priority_gain * days_since(priority_start))
            button Done (margin=0, label="Done") {
                on click {
                    set (path="/Tasks/CompletedSample/description", mode="value") = $(../description)
                    set (path="/Tasks/CompletedSample/points", mode="value") = $(../points)
                    set_now_ts (path="/Tasks/CompletedSample/completed_at")
                    append (template="<CompletedSample>", list="/Tasks/Completed")
                    remove (keyField="id", keyValue=$(../id), from="/Tasks/Active")
                }
            }
        }
        div CompletedRecord (spacing=8, layout="horizontal", padding=4) {
            string description (label="", margin=0) = ""
            int points (label="", margin=0) = 1
            timestamp completed_at (margin=0, mode="elapsed", format="long", label="Completed")
        }
        <ActiveTask> ActiveSample {
        }
        <CompletedRecord> CompletedSample {
        }
        div RecurringTask (layout="horizontal", padding=6, spacing=4) {
            int rid (margin=0, label="Recurrence Id") = 1
            string description (margin=0, label="Task") = ""
            string comment (label="Comment", margin=0) = ""
            int points (label="Points", margin=0) = 1
            int base_priority (label="Base Priority", margin=0) = 0
            int priority_gain (margin=0, label="Daily Gain") = 0
            string mode (label="Mode", margin=0) = "interval"
            string interval (label="Every", margin=0) = "7d"
            checkbox pause_when_active (label="Pause When Active", margin=0) = true
            timestamp last_triggered_at (mode="elapsed", label="Since", margin=0) = $(now())
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
                        - priority_start = $(now())
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
            button pause (margin=0, label="Pause") {
                on click {
                    set (mode="value", path="/Tasks/Recurring/generator/active") = false
                }
            }
        }
    }
    div State (hidden=true) {
        int next_id = 4
    }
    div NewTask (padding=6, spacing=6, layout="horizontal") {
        string description (margin=0, label="Task") = ""
        string commentary (label="Comment", margin=0) = ""
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
                    - priority_start = $(now())
                }
                inc (path="/Tasks/State/next_id")
                set (path="/Tasks/NewTask/description", mode="value") = ""
                set (mode="value", path="/Tasks/NewTask/commentary") = ""
                set (path="/Tasks/NewTask/points", mode="value") = 1
                set (mode="value", path="/Tasks/NewTask/priority") = 0
                set (mode="value", path="/Tasks/NewTask/priority_gain") = 0
            }
        }
    }
    list Active (spacing=6, entry=<ActiveTask>, sort_by=$(|x| 0 - x/effective_priority), layout="vertical", key="id") {
        - {
            int id = 2
            int rid = 0
            string description = "Task A"
            string comment = "CCCC"
            int points = 1
            int priority = 0
            int priority_gain = 0
            timestamp priority_start = "2025-08-13T10:22:06.027892400+00:00"
        }
    }
    list Completed (layout="vertical", spacing=4, entry=<CompletedRecord>) {
        - {
        }
    }
    list Recurring (spacing=8, layout="vertical", key="rid", entry=<RecurringTask>) {
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
