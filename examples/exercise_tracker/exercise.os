div exercise_tracker (border-style=none, layout="vertical", mutable=true) {
    text header = "## Exercise Tracker"
    div (hidden=true) {
        div ExerciseRecord (margin=0, padding=0) {
            string eid (hidden=true, margin=0) = ""
            string description (margin=0, width=20%) = $(/exercise_tracker/Exercises.find(eid)/description)
            int sets (margin=0, width=5%) = 3
            int reps (margin=0, width=5%) = 20
            float weight (margin=0, precision=1, suffix=" kg", width=5%) = 9.6
            float volume (margin=0, precision=1, suffix=" kg", width=5%) = $(reps*weight)
            timestamp time (margin=0, width=10%) = "2025-08-10T12:48:13.509010+00:00"
            button rem (icon="cross", margin=0, width=2%) {
                on click {
                    remove (from="/exercise_tracker/History", keyField="time", keyValue=$(../time))
                }
            }
        }
        <ExerciseRecord> ExerciseRecordSample {
            - eid = "bench_press_db"
            - sets = 1
            - reps = 18
            - weight = 16.1
            - time = "2026-08-09T06:34:33.436291200+00:00"
        }
    }
    div (hidden=true) {
        div Exercise (alignment="center", background-color=$(
                days_since(last_done) >= 5 ? "#4b0a0aff" : 
                (days_since(last_done) >= 3 ? "#654d04ff" : 
                (days_since(last_done) >= 1 ? "inherit" : "#1c4005ff"))), layout="horizontal", margin=0) {
            string id (hidden=true) = ""
            string description (margin=0, width=30%) = ""
            div plates (layout="horizontal", margin=0) {
                int p1 = 0
                int p2 = 1
                int p3 = 0
                int p4 = 0
            }
            float weight (fallback=$(1.1+plates/p1*1.0 + plates/p2*2.5 + plates/p3*5.0 + plates/p4*15.0), label="Weight", margin=0) = null
            int sets (label="Sets", margin=0) = 3
            int reps (label="Reps", margin=0) = 10
            timestamp last_done (label="Last Done", margin=0, mode="elapsed", width=200px) = $(/exercise_tracker/History.filter(|x| x/eid == ../id).map(|x| x/time).max())
            button done (label="Done", margin=0) {
                on click {
                    append (list="/exercise_tracker/History") {
                        - eid = $(../id)
                        - sets = $(../sets)
                        - reps = $(../reps)
                        - weight = $(../weight)
                        - time = $(now())
                    }
                }
            }
        }
    }
    div (border-style=none) {
        list Exercises (border-style=none, entry=<Exercise>, key="id", layout="vertical", sort_by=$(|x| x/last_done), width=50%) {
            - {
                - id = "bicep_curls"
                - description = "Bicep curls"
                div plates {
                    int p1 = 0
                    int p2 = 0
                    int p3 = 0
                    int p4 = 1
                }
                - sets = 3
                - reps = 19
            }
            - {
                - id = "deltoids_lateral_raise"
                - description = "Deltoids: Lateral raise"
                div plates {
                    int p1 = 1
                    int p2 = 1
                    int p3 = 1
                    int p4 = 0
                }
                - sets = 3
                - reps = 26
            }
            - {
                - id = "deltoids_shoulder_press"
                - description = "Deltoids: Shoulder Press"
                div plates {
                    int p1 = 0
                    int p2 = 0
                    int p3 = 0
                    int p4 = 1
                }
                - sets = 3
                - reps = 16
            }
            - {
                - id = "deltoids_reverse_fly"
                - description = "Deltoids: Reverse Fly"
                div plates {
                    int p1 = 1
                    int p2 = 1
                    int p3 = 1
                    int p4 = 0
                }
                - sets = 2
                - reps = 21
            }
            - {
                - id = "triceps_overhead_extension"
                - description = "Triceps: Overhead Extension"
                div plates {
                    int p1 = 1
                    int p2 = 1
                    int p3 = 1
                    int p4 = 0
                }
                - sets = 3
                - reps = 20
            }
            - {
                - id = "bench_press_db"
                - description = "Bench Press DB"
                div plates {
                    int p1 = 0
                    int p2 = 0
                    int p3 = 0
                    int p4 = 1
                }
                - weight = null
                - sets = 1
                - reps = 18
            }
            - {
                - id = "rhomboid_2_hand_rows"
                - description = "Rhomboid: 2-Hand Rows"
                div plates {
                    int p1 = 0
                    int p2 = 0
                    int p3 = 0
                    int p4 = 1
                }
                - sets = 3
                - reps = 25
            }
            - {
                - id = "rhomboid_1_hand_rows"
                - description = "Rhomboid: 1-Hand Rows"
                div plates {
                    int p1 = 0
                    int p2 = 0
                    int p3 = 1
                    int p4 = 1
                }
                - sets = 3
                - reps = 21
            }
            - {
                - id = "push_ups"
                - description = "Push Ups"
                div plates {
                    int p1 = 0
                    int p2 = 0
                    int p3 = 0
                    int p4 = 0
                }
                - weight = 35
                - sets = 1
                - reps = 17
            }
            - {
                - id = "pelvic_stretch"
                - description = "Pelvic Stretch"
                div plates {
                    int p1 = 0
                    int p2 = 0
                    int p3 = 1
                    int p4 = 1
                }
                - sets = 1
                - reps = 33
            }
            - {
                - id = "sit_ups"
                - description = "Sit Ups"
                div plates {
                    int p1 = 0
                    int p2 = 0
                    int p3 = 0
                    int p4 = 0
                }
                - weight = 15
                - sets = 1
                - reps = 35
            }
            - {
                - id = "traps_shrugs"
                - description = "Traps: Shrugs"
                div plates {
                    int p1 = 0
                    int p2 = 0
                    int p3 = 1
                    int p4 = 1
                }
                - sets = 1
                - reps = 22
            }
            - {
                - id = "hammer_curls"
                - description = "Hammer Curls"
                div plates {
                    int p1 = 0
                    int p2 = 0
                    int p3 = 0
                    int p4 = 1
                }
                - sets = 1
                - reps = 16
            }
        }
        div plots (height=800px, width=40%) {
            chart C (height=100%, width=100%) {
                plot P1 (color="#b42c22ff", label="Bicep Curls", source=$( /exercise_tracker/History.filter(|x| x/eid == "bicep_curls") ), x=$(|x| x/time), y=$(|x| x/volume))
                plot P9 (color="#cc4f6aff", label="Hammer Curls", source=$( /exercise_tracker/History.filter(|x| x/eid == "hammer_curls") ), x=$(|x| x/time), y=$(|x| x/volume))
                plot P2 (color=#4A90E2, label="Triceps", source=$( /exercise_tracker/History.filter(|x| x/eid == "triceps_overhead_extension") ), x=$(|x| x/time), y=$(|x| x/volume))
                plot P3 (color="#599b1fff", label="Deltoids Lateral", source=$( /exercise_tracker/History.filter(|x| x/eid == "deltoids_lateral_raise") ), x=$(|x| x/time), y=$(|x| x/volume))
                plot P4 (color="#649317ff", label="Deltoids Shoulder Press", source=$( /exercise_tracker/History.filter(|x| x/eid == "deltoids_shoulder_press") ), x=$(|x| x/time), y=$(|x| x/volume))
                plot P5 (color="#569b3fff", label="Deltoids Reverse Fly", source=$( /exercise_tracker/History.filter(|x| x/eid == "deltoids_reverse_fly") ), x=$(|x| x/time), y=$(|x| x/volume))
                plot P6 (color="#9c33caff", label="Rhomboid 1-Hand", source=$( /exercise_tracker/History.filter(|x| x/eid == "rhomboid_1_hand_rows") ), x=$(|x| x/time), y=$(|x| x/volume))
                plot P7 (color="#ab59ceff", label="Rhomboid 2-Hand", source=$( /exercise_tracker/History.filter(|x| x/eid == "rhomboid_2_hand_rows") ), x=$(|x| x/time), y=$(|x| x/volume))
                plot P8 (color="#c9b941ff", label="Bench Press", source=$( /exercise_tracker/History.filter(|x| x/eid == "bench_press_db") ), x=$(|x| x/time), y=$(|x| x/volume))
            }
        }
    }
    list History (entry=<ExerciseRecord>, layout="vertical", margin=0, spacing=0) {
        - {
            - eid = "bicep_curls"
            - sets = 3
            - reps = 12
            - weight = 16.1
            - time = "2026-08-01T07:00:00+00:00"
        }
        - {
            - eid = "deltoids_lateral_raise"
            - sets = 3
            - reps = 20
            - weight = 9.6
            - time = "2026-08-01T16:12:00+00:00"
        }
        - {
            - eid = "rhomboid_1_hand_rows"
            - sets = 3
            - reps = 12
            - weight = 16.1
            - time = "2026-08-01T13:24:00+00:00"
        }
        - {
            - eid = "push_ups"
            - sets = 3
            - reps = 20
            - weight = 0
            - time = "2026-08-01T10:36:00+00:00"
        }
        - {
            - eid = "pelvic_stretch"
            - sets = 1
            - reps = 30
            - weight = 0
            - time = "2026-08-01T07:48:00+00:00"
        }
        - {
            - eid = "triceps_overhead_extension"
            - sets = 3
            - reps = 16
            - weight = 9.6
            - time = "2026-08-02T13:08:00+00:00"
        }
        - {
            - eid = "deltoids_reverse_fly"
            - sets = 3
            - reps = 15
            - weight = 6.1
            - time = "2026-08-02T10:20:00+00:00"
        }
        - {
            - eid = "bench_press_db"
            - sets = 3
            - reps = 10
            - weight = 16.1
            - time = "2026-08-02T07:32:00+00:00"
        }
        - {
            - eid = "traps_shrugs"
            - sets = 3
            - reps = 15
            - weight = 16.1
            - time = "2026-08-02T16:44:00+00:00"
        }
        - {
            - eid = "hammer_curls"
            - sets = 3
            - reps = 12
            - weight = 16.1
            - time = "2026-08-03T10:04:00+00:00"
        }
        - {
            - eid = "deltoids_shoulder_press"
            - sets = 3
            - reps = 10
            - weight = 16.1
            - time = "2026-08-03T07:16:00+00:00"
        }
        - {
            - eid = "rhomboid_2_hand_rows"
            - sets = 3
            - reps = 12
            - weight = 11.1
            - time = "2026-08-03T16:28:00+00:00"
        }
        - {
            - eid = "sit_ups"
            - sets = 3
            - reps = 25
            - weight = 0
            - time = "2026-08-03T13:40:00+00:00"
        }
        - {
            - eid = "bicep_curls"
            - sets = 3
            - reps = 13
            - weight = 16.1
            - time = "2026-08-04T07:00:00+00:00"
        }
        - {
            - eid = "deltoids_lateral_raise"
            - sets = 3
            - reps = 21
            - weight = 9.6
            - time = "2026-08-04T16:12:00+00:00"
        }
        - {
            - eid = "rhomboid_1_hand_rows"
            - sets = 3
            - reps = 13
            - weight = 16.1
            - time = "2026-08-04T13:24:00+00:00"
        }
        - {
            - eid = "push_ups"
            - sets = 3
            - reps = 21
            - weight = 0
            - time = "2026-08-04T10:36:00+00:00"
        }
        - {
            - eid = "pelvic_stretch"
            - sets = 1
            - reps = 31
            - weight = 0
            - time = "2026-08-04T07:48:00+00:00"
        }
        - {
            - eid = "triceps_overhead_extension"
            - sets = 3
            - reps = 17
            - weight = 9.6
            - time = "2026-08-05T13:08:00+00:00"
        }
        - {
            - eid = "deltoids_reverse_fly"
            - sets = 3
            - reps = 16
            - weight = 6.1
            - time = "2026-08-05T10:20:00+00:00"
        }
        - {
            - eid = "bench_press_db"
            - sets = 3
            - reps = 11
            - weight = 16.1
            - time = "2026-08-05T07:32:00+00:00"
        }
        - {
            - eid = "traps_shrugs"
            - sets = 3
            - reps = 16
            - weight = 16.1
            - time = "2026-08-05T16:44:00+00:00"
        }
        - {
            - eid = "hammer_curls"
            - sets = 3
            - reps = 13
            - weight = 16.1
            - time = "2026-08-06T10:04:00+00:00"
        }
        - {
            - eid = "deltoids_shoulder_press"
            - sets = 3
            - reps = 11
            - weight = 16.1
            - time = "2026-08-06T07:16:00+00:00"
        }
        - {
            - eid = "rhomboid_2_hand_rows"
            - sets = 3
            - reps = 13
            - weight = 11.1
            - time = "2026-08-06T16:28:00+00:00"
        }
        - {
            - eid = "sit_ups"
            - sets = 3
            - reps = 26
            - weight = 0
            - time = "2026-08-06T13:40:00+00:00"
        }
        - {
            - eid = "bicep_curls"
            - sets = 3
            - reps = 14
            - weight = 16.1
            - time = "2026-08-07T07:00:00+00:00"
        }
        - {
            - eid = "deltoids_lateral_raise"
            - sets = 3
            - reps = 22
            - weight = 9.6
            - time = "2026-08-07T16:12:00+00:00"
        }
        - {
            - eid = "rhomboid_1_hand_rows"
            - sets = 3
            - reps = 14
            - weight = 16.1
            - time = "2026-08-07T13:24:00+00:00"
        }
        - {
            - eid = "push_ups"
            - sets = 3
            - reps = 22
            - weight = 0
            - time = "2026-08-07T10:36:00+00:00"
        }
        - {
            - eid = "pelvic_stretch"
            - sets = 1
            - reps = 32
            - weight = 0
            - time = "2026-08-07T07:48:00+00:00"
        }
        - {
            - eid = "triceps_overhead_extension"
            - sets = 3
            - reps = 18
            - weight = 9.6
            - time = "2026-08-08T13:08:00+00:00"
        }
        - {
            - eid = "deltoids_reverse_fly"
            - sets = 3
            - reps = 17
            - weight = 6.1
            - time = "2026-08-08T10:20:00+00:00"
        }
        - {
            - eid = "bench_press_db"
            - sets = 3
            - reps = 12
            - weight = 16.1
            - time = "2026-08-08T07:32:00+00:00"
        }
        - {
            - eid = "traps_shrugs"
            - sets = 3
            - reps = 17
            - weight = 16.1
            - time = "2026-08-08T16:44:00+00:00"
        }
        - {
            - eid = "hammer_curls"
            - sets = 3
            - reps = 14
            - weight = 16.1
            - time = "2026-08-09T10:04:00+00:00"
        }
        - {
            - eid = "deltoids_shoulder_press"
            - sets = 3
            - reps = 12
            - weight = 16.1
            - time = "2026-08-09T07:16:00+00:00"
        }
        - {
            - eid = "rhomboid_2_hand_rows"
            - sets = 3
            - reps = 14
            - weight = 11.1
            - time = "2026-08-09T16:28:00+00:00"
        }
        - {
            - eid = "sit_ups"
            - sets = 3
            - reps = 27
            - weight = 0
            - time = "2026-08-09T13:40:00+00:00"
        }
        - {
            - eid = "bicep_curls"
            - sets = 3
            - reps = 15
            - weight = 16.1
            - time = "2026-08-10T07:00:00+00:00"
        }
        - {
            - eid = "deltoids_lateral_raise"
            - sets = 3
            - reps = 23
            - weight = 9.6
            - time = "2026-08-10T16:12:00+00:00"
        }
        - {
            - eid = "rhomboid_1_hand_rows"
            - sets = 3
            - reps = 15
            - weight = 16.1
            - time = "2026-08-10T13:24:00+00:00"
        }
        - {
            - eid = "push_ups"
            - sets = 3
            - reps = 23
            - weight = 0
            - time = "2026-08-10T10:36:00+00:00"
        }
        - {
            - eid = "pelvic_stretch"
            - sets = 1
            - reps = 33
            - weight = 0
            - time = "2026-08-10T07:48:00+00:00"
        }
        - {
            - eid = "triceps_overhead_extension"
            - sets = 3
            - reps = 19
            - weight = 9.6
            - time = "2026-08-11T13:08:00+00:00"
        }
        - {
            - eid = "deltoids_reverse_fly"
            - sets = 3
            - reps = 18
            - weight = 6.1
            - time = "2026-08-11T10:20:00+00:00"
        }
        - {
            - eid = "bench_press_db"
            - sets = 3
            - reps = 13
            - weight = 16.1
            - time = "2026-08-11T07:32:00+00:00"
        }
        - {
            - eid = "traps_shrugs"
            - sets = 3
            - reps = 18
            - weight = 16.1
            - time = "2026-08-11T16:44:00+00:00"
        }
        - {
            - eid = "hammer_curls"
            - sets = 3
            - reps = 15
            - weight = 16.1
            - time = "2026-08-12T10:04:00+00:00"
        }
        - {
            - eid = "deltoids_shoulder_press"
            - sets = 3
            - reps = 13
            - weight = 16.1
            - time = "2026-08-12T07:16:00+00:00"
        }
        - {
            - eid = "rhomboid_2_hand_rows"
            - sets = 3
            - reps = 15
            - weight = 11.1
            - time = "2026-08-12T16:28:00+00:00"
        }
        - {
            - eid = "sit_ups"
            - sets = 3
            - reps = 28
            - weight = 0
            - time = "2026-08-12T13:40:00+00:00"
        }
        - {
            - eid = "bicep_curls"
            - sets = 3
            - reps = 16
            - weight = 16.1
            - time = "2026-08-13T07:00:00+00:00"
        }
        - {
            - eid = "deltoids_lateral_raise"
            - sets = 3
            - reps = 24
            - weight = 9.6
            - time = "2026-08-13T16:12:00+00:00"
        }
        - {
            - eid = "rhomboid_1_hand_rows"
            - sets = 3
            - reps = 16
            - weight = 16.1
            - time = "2026-08-13T13:24:00+00:00"
        }
        - {
            - eid = "push_ups"
            - sets = 3
            - reps = 24
            - weight = 0
            - time = "2026-08-13T10:36:00+00:00"
        }
        - {
            - eid = "pelvic_stretch"
            - sets = 1
            - reps = 34
            - weight = 0
            - time = "2026-08-13T07:48:00+00:00"
        }
        - {
            - eid = "triceps_overhead_extension"
            - sets = 3
            - reps = 20
            - weight = 9.6
            - time = "2026-08-14T13:08:00+00:00"
        }
        - {
            - eid = "deltoids_reverse_fly"
            - sets = 3
            - reps = 19
            - weight = 6.1
            - time = "2026-08-14T10:20:00+00:00"
        }
        - {
            - eid = "bench_press_db"
            - sets = 3
            - reps = 14
            - weight = 16.1
            - time = "2026-08-14T07:32:00+00:00"
        }
        - {
            - eid = "traps_shrugs"
            - sets = 3
            - reps = 19
            - weight = 16.1
            - time = "2026-08-14T16:44:00+00:00"
        }
    }
}
