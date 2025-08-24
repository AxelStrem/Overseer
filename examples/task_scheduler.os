tab Tasks {
    div (hidden=true) {
        div ActiveTask (background-color=$(
            effective_priority > 100 ? "#320505ff" :
            (effective_priority > 50 ? "#2c2920ff" : "inherit")
            ), layout="horizontal", padding=6, spacing=0) {
            int id (hidden=true) = 0
            int rid (hidden=true) = 0
            string description (font-size=18px, label="", margin=0) = ""
            string comment (label="", margin=0) = ""
            int points (label="Points", margin=0) = 1
            int priority (hidden=true, margin=0) = 0
            int priority_gain (label="Daily Gain", margin=0) = 0
            timestamp task_created (hidden=true, label="Since", margin=0, mode="elapsed") = $(now())
            int effective_priority (label="Priority") = $(priority + priority_gain * days_since(task_created))
            button Done (label="Done", margin=0) {
                on click {
                    append (list="/Tasks/Completed", template="<CompletedRecord>") {
                        - description = $(../description)
                        - points = $(../points)
                        - completed_at = $(now())
                    }
                    if (cond=$(../rid > 0)) {
                        set_in_list (field="active", keyField="rid", keyValue=$(../rid), list="/Tasks/Recurring") = false
                        set_in_list (field="last_triggered_at", keyField="rid", keyValue=$(../rid), list="/Tasks/Recurring") = $(
                                /Tasks/Recurring.filter(|x| x/rid == ../rid).first()/reset_on_unpause
                                    ? now()
                                    : /Tasks/Recurring.filter(|x| x/rid == ../rid).first()/last_triggered_at
                            )
                    }
                    remove (from="/Tasks/Active", keyField="id", keyValue=$(../id))
                }
            }
            button Cancel (label="Cancel", margin=0) {
                on click {
                    if (cond=$(../rid > 0)) {
                        set_in_list (field="last_triggered_at", keyField="rid", keyValue=$(../rid), list="/Tasks/Recurring") = $(
                                /Tasks/Recurring.filter(|x| x/rid == ../rid).first()/reset_on_unpause
                                    ? now()
                                    : /Tasks/Recurring.filter(|x| x/rid == ../rid).first()/last_triggered_at
                            )
                    }
                    remove (from="/Tasks/Active", keyField="id", keyValue=$(../id))
                }
            }
        }
        div CompletedRecord (layout="horizontal", padding=4, spacing=8) {
            string description (label="", margin=0) = ""
            int points (label="", margin=0) = 1
            timestamp completed_at (format="long", label="Completed", margin=0)
        }
        <ActiveTask> ActiveSample = <CompletedRecord>
        CompletedSample div
        RecurringTask (layout="horizontal", padding=6, spacing=4) {
            int rid (label="Recurrence Id", margin=0) = 1
            string description (label="Task", margin=0) = ""
            string comment (label="Comment", margin=0) = ""
            int points (label="Points", margin=0) = 1
            int base_priority (label="Base Priority", margin=0) = 0
            int priority_gain (label="Daily Gain", margin=0) = 0
            string mode (label="Mode", margin=0) = "interval"
            string interval (label="Every", margin=0) = "7d"
            checkbox pause_when_active (label="Pause When Active", margin=0) = true
            checkbox reset_on_unpause (label="Reset on Unpause", margin=0) = false
            bool active (hidden=true) = false
            bool manual_pause (hidden=true) = false
            timestamp last_triggered_at (label="Since", margin=0, mode="elapsed") = $(now())
            timer generator (active=$(
                    ../mode == "interval"
                    && (../manual_pause == false)
                    && (../pause_when_active == false || ../../../Active.filter(|x| x/rid == ../rid).count() == 0)
                ), at=$(../last_triggered_at), offset=$(../interval), one_shot=false) {
                on timeout {
                    // Append a new active task (firing is already gated by the timer's active formula)
                    append (list="../../../Active", template="<ActiveTask>") {
                        - description = $(../description)
                        - comment = $(../comment)
                        - points = $(../points)
                        - priority = $(../base_priority)
                        - priority_gain = $(../priority_gain)
                        - rid = $(../rid)
                        - id = $(../../../State/next_id)
                        - task_created = $(now())
                    }
                    inc (path="../../../State/next_id")
                    set_now_ts (path="../last_triggered_at")
                }
            }
            button start (label="Start / Resume", margin=0) {
                on click {
                    set_now_ts (path="../last_triggered_at")
                    set (mode="value", path="../manual_pause") = false
                }
            }
            button pause (label="Pause", margin=0) {
                on click {
                    set (mode="value", path="../manual_pause") = true
                }
            }
        }
    }

    div State (hidden=true) {
        int next_id = 99
    }
    div NewTask (layout="horizontal", padding=6, spacing=6) {
        string description (label="Task", margin=0) = ""
        string commentary (label="Comment", margin=0) = ""
        int points (label="Points", margin=0) = 1
        int priority (label="Priority", margin=0) = 0
        int priority_gain (label="Daily Gain", margin=0) = 0
        button Create (label="Create Task", margin=0) {
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
                set (mode="value", path="/Tasks/NewTask/points") = 1
                set (mode="value", path="/Tasks/NewTask/priority") = 0
                set (mode="value", path="/Tasks/NewTask/priority_gain") = 0
            }
        }
    }
    list Active (entry=<ActiveTask>, key="id", layout="vertical", sort_by=$(|x| 0 - x/effective_priority), spacing=6) {
        
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
            - id = 24
            - description = "Practice and record some vocals"
            - priority = 10
            - task_created = "2025-08-16T18:26:00.088930300+00:00"
        }
        - {
            - id = 54
            - rid = 18
            - description = "Take care of the cat"
            - comment = "toilet, food, water"
            - points = 3
            - priority = 6
            - priority_gain = 0
            - task_created = "2025-08-20T00:00:00+00:00"
        }
        - {
            - id = 71
            - rid = 9
            - description = "Work on project: Stream"
            - comment = ""
            - points = 5
            - priority = 8
            - priority_gain = 0
            - task_created = "2025-08-21T07:25:06.483164900+00:00"
        }
        - {
            - id = 80
            - rid = 2
            - description = "Back up photos"
            - comment = "External drive"
            - points = 2
            - priority = 10
            - priority_gain = 2
            - task_created = "2025-08-22T20:27:30.827047100+00:00"
        }
        - {
            - id = 87
            - rid = 12
            - description = "Wash clothes"
            - comment = ""
            - points = 5
            - priority = 2
            - priority_gain = 0
            - task_created = "2025-08-22T20:27:40.915316400+00:00"
        }
        - {
            - id = 91
            - rid = 4
            - description = "Evening routine"
            - comment = "3D, BT, shower"
            - points = 1
            - priority = 10
            - priority_gain = 0
            - task_created = "2025-08-23T20:27:34.301520400+00:00"
        }
        - {
            - id = 92
            - rid = 5
            - description = "Language practice"
            - comment = "Duolingo, Anki, Speaking"
            - points = 2
            - priority = 10
            - priority_gain = 0
            - task_created = "2025-08-23T20:27:36.310847300+00:00"
        }
        - {
            - id = 94
            - rid = 7
            - description = "Guitar practice"
            - comment = "basic riffs"
            - points = 2
            - priority = 3
            - priority_gain = 0
            - task_created = "2025-08-23T20:27:39.282424500+00:00"
        }
        - {
            - id = 95
            - rid = 10
            - description = "Work on project: Overseer"
            - comment = ""
            - points = 5
            - priority = 8
            - priority_gain = 0
            - task_created = "2025-08-23T20:27:40.082313100+00:00"
        }
        - {
            - id = 98
            - description = "Выписка из банка"
            - points = 5
            - priority = 15
            - task_created = "2025-08-24T11:55:42.561361500+00:00"
        }
    }
    list Completed (entry=<CompletedRecord>, layout="vertical", spacing=4) {
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
            - description = "Back up photos"
            - points = 2
            - completed_at = "2025-08-18T20:28:08.269254800+00:00"
        }
        - {
            - description = "Morning routine"
            - points = 1
            - completed_at = "2025-08-18T20:28:10.409209300+00:00"
        }
        - {
            - description = "Morning routine"
            - points = 1
            - completed_at = "2025-08-18T20:28:12.033617700+00:00"
        }
        - {
            - description = "Morning routine"
            - points = 1
            - completed_at = "2025-08-18T20:28:13.592098400+00:00"
        }
        - {
            - description = "Evening routine"
            - points = 1
            - completed_at = "2025-08-18T20:28:15.611654400+00:00"
        }
        - {
            - description = "Evening routine"
            - points = 1
            - completed_at = "2025-08-18T20:28:17.148486300+00:00"
        }
        - {
            - description = "Language practice"
            - points = 2
            - completed_at = "2025-08-18T20:28:19.123828700+00:00"
        }
        - {
            - description = "Language practice"
            - points = 2
            - completed_at = "2025-08-18T20:28:20.563030100+00:00"
        }
        - {
            - description = "Language practice"
            - points = 2
            - completed_at = "2025-08-18T20:28:22.047829600+00:00"
        }
        - {
            - description = "Drum practice"
            - points = 2
            - completed_at = "2025-08-18T20:28:30.458172300+00:00"
        }
        - {
            - description = "Drum practice"
            - points = 2
            - completed_at = "2025-08-18T20:28:32.074815300+00:00"
        }
        - {
            - description = "Motoric practice"
            - points = 1
            - completed_at = "2025-08-18T20:28:33.753399600+00:00"
        }
        - {
            - description = "Guitar practice"
            - points = 2
            - completed_at = "2025-08-18T20:28:38.826517900+00:00"
        }
        - {
            - description = "Guitar practice"
            - points = 2
            - completed_at = "2025-08-18T20:28:42.397834700+00:00"
        }
        - {
            - description = "Motoric practice"
            - points = 1
            - completed_at = "2025-08-18T20:28:44.849951400+00:00"
        }
        - {
            - description = "Start 3D treatment"
            - points = 1
            - completed_at = "2025-08-19T05:42:47.093962100+00:00"
        }
        - {
            - description = "Take care of the cat"
            - points = 3
            - completed_at = "2025-08-19T13:34:48.945780600+00:00"
        }
        - {
            - description = "Morning routine"
            - points = 1
            - completed_at = "2025-08-19T13:35:35.591987600+00:00"
        }
        - {
            - description = "Take care of the cat"
            - points = 3
            - completed_at = "2025-08-19T13:37:04.819313600+00:00"
        }
        - {
            - description = "Shop: Musicroom.ge"
            - points = 1
            - completed_at = "2025-08-19T13:57:52.877193200+00:00"
        }
        - {
            - description = "Motoric practice"
            - points = 1
            - completed_at = "2025-08-19T14:03:07.742231700+00:00"
        }
        - {
            - description = "Wash clothes"
            - points = 5
            - completed_at = "2025-08-19T14:03:10.645791400+00:00"
        }
        - {
            - description = "Shop: Sportmaster.ge"
            - points = 1
            - completed_at = "2025-08-19T14:03:16.196387900+00:00"
        }
        - {
            - description = "Evening routine"
            - points = 1
            - completed_at = "2025-08-20T04:29:48.448065500+00:00"
        }
        - {
            - description = "Leetcode practice"
            - points = 1
            - completed_at = "2025-08-20T06:03:13.853479+00:00"
        }
        - {
            - description = "Morning routine"
            - points = 1
            - completed_at = "2025-08-20T09:26:29.137241300+00:00"
        }
        - {
            - description = "Work on project: Stream"
            - points = 5
            - completed_at = "2025-08-20T09:26:35.455891800+00:00"
        }
        - {
            - description = "Work on project: Overseer"
            - points = 5
            - completed_at = "2025-08-20T18:43:45.936517500+00:00"
        }
        - {
            - description = "Motoric practice"
            - points = 1
            - completed_at = "2025-08-20T18:44:12.706537500+00:00"
        }
        - {
            - description = "Guitar practice"
            - points = 2
            - completed_at = "2025-08-20T18:44:17.809198100+00:00"
        }
        - {
            - description = "Drum practice"
            - points = 2
            - completed_at = "2025-08-20T18:46:54.822706100+00:00"
        }
        - {
            - description = "Evening routine"
            - points = 1
            - completed_at = "2025-08-20T20:28:05.230395900+00:00"
        }
        - {
            - description = "Language practice"
            - points = 2
            - completed_at = "2025-08-20T20:28:10.406598500+00:00"
        }
        - {
            - description = "Leetcode practice"
            - points = 1
            - completed_at = "2025-08-21T05:03:02.477722800+00:00"
        }
        - {
            - description = "Morning routine"
            - points = 1
            - completed_at = "2025-08-21T20:21:59.272865200+00:00"
        }
        - {
            - description = "Drum practice"
            - points = 2
            - completed_at = "2025-08-21T20:22:08.941829700+00:00"
        }
        - {
            - description = "Motoric practice"
            - points = 1
            - completed_at = "2025-08-21T20:22:11.983641400+00:00"
        }
        - {
            - description = "Leetcode practice"
            - points = 1
            - completed_at = "2025-08-22T05:51:51.335201800+00:00"
        }
        - {
            - description = "Morning routine"
            - points = 1
            - completed_at = "2025-08-22T06:29:31.376576300+00:00"
        }
        - {
            - description = "Motoric practice"
            - points = 1
            - completed_at = "2025-08-22T06:29:40.260107900+00:00"
        }
        - {
            - description = "Evening routine"
            - points = 1
            - completed_at = "2025-08-23T20:09:07.505922900+00:00"
        }
        - {
            - description = "Language practice"
            - points = 2
            - completed_at = "2025-08-23T20:09:10.489651900+00:00"
        }
        - {
            - description = "Work on project: Overseer"
            - points = 5
            - completed_at = "2025-08-23T20:09:16.316171900+00:00"
        }
        - {
            - description = "Morning routine"
            - points = 1
            - completed_at = "2025-08-23T20:09:41.332327100+00:00"
        }
        - {
            - description = "Leetcode practice"
            - points = 1
            - completed_at = "2025-08-23T20:10:02.048213700+00:00"
        }
        - {
            - description = "Drum practice"
            - points = 2
            - completed_at = "2025-08-23T20:10:06.517600100+00:00"
        }
        - {
            - description = "Guitar practice"
            - points = 2
            - completed_at = "2025-08-23T20:10:11.843351300+00:00"
        }
        - {
            - description = "Motoric practice"
            - points = 1
            - completed_at = "2025-08-23T20:10:17.011542800+00:00"
        }
        - {
            - description = "Morning routine"
            - points = 1
            - completed_at = "2025-08-24T11:20:23.338078800+00:00"
        }
        - {
            - description = "Leetcode practice"
            - points = 1
            - completed_at = "2025-08-24T11:20:31.614557700+00:00"
        }
        - {
            - description = "Motoric practice"
            - points = 1
            - completed_at = "2025-08-24T11:27:15.648957500+00:00"
        }
        - {
            - description = "Drum practice"
            - points = 2
            - completed_at = "2025-08-24T11:55:02.522538500+00:00"
        }
    }
    list Recurring (entry=<RecurringTask>, key="rid", layout="vertical", spacing=8) {
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
            - last_triggered_at = "2025-08-20T10:48:25.886134500+00:00"
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
            - last_triggered_at = "2025-08-22T20:27:30.827047100+00:00"
        }
        - {
            - rid = 3
            - description = "Morning routine"
            - comment = "3D, acid"
            - points = 1
            - base_priority = 10
            - mode = "interval"
            - interval = "1d"
            - pause_when_active = true
            - active = false
            - last_triggered_at = "2025-08-24T00:00:00+00:00"
        }
        - {
            - rid = 4
            - description = "Evening routine"
            - comment = "3D, BT, shower"
            - points = 1
            - base_priority = 10
            - mode = "interval"
            - interval = "1d"
            - pause_when_active = true
            - active = false
            - last_triggered_at = "2025-08-23T20:27:34.301520400+00:00"
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
            - active = false
            - last_triggered_at = "2025-08-23T20:27:36.310847300+00:00"
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
            - active = false
            - last_triggered_at = "2025-08-23T20:27:37.749113200+00:00"
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
            - active = false
            - last_triggered_at = "2025-08-23T20:27:39.282424500+00:00"
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
            - last_triggered_at = "2025-08-16T07:25:06.480604900+00:00"
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
            - active = false
            - last_triggered_at = "2025-08-21T07:25:06.483164900+00:00"
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
            - active = false
            - last_triggered_at = "2025-08-23T20:27:40.082313100+00:00"
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
            - last_triggered_at = "2025-08-16T07:25:06.488520700+00:00"
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
            - active = false
            - last_triggered_at = "2025-08-22T20:27:40.915316400+00:00"
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
            - active = false
            - last_triggered_at = "2025-08-24T00:00:00+00:00"
        }
        - {
            - rid = 18
            - description = "Take care of the cat"
            - comment = "toilet, food, water"
            - points = 3
            - base_priority = 6
            - mode = "interval"
            - interval = "1d"
            - pause_when_active = true
            - active = false
            - last_triggered_at = "2025-08-20T00:00:00+00:00"
        }
        - {
            - rid = 19
            - description = "Leetcode practice"
            - comment = "problem of the day"
            - points = 1
            - base_priority = 6
            - mode = "interval"
            - interval = "1d"
            - pause_when_active = true
            - active = false
            - last_triggered_at = "2025-08-24T00:00:00+00:00"
        }
    }
}
