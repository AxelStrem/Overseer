// Fixture for exercise_inline_edit_preserves_local_diff.
// Trimmed from examples/exercise_tracker/exercise.os down to the shape the test
// needs: a template carrying an inline `plates` block, and a list whose entries
// override it. Frozen on purpose — the live example is user data that changes
// whenever the app is used, so it cannot anchor an assertion about a specific node.
div exercise_tracker (border-style=none, layout="vertical", mutable=true) {
    text header = "## Exercise Tracker"
    div (hidden=true) {
        div Exercise (alignment="center", layout="horizontal", margin=0) {
            int id (hidden=true) = 0
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
        }
    }
    div (border-style=none) {
        list Exercises (border-style=none, entry=<Exercise>, key="id", layout="vertical", width=50%) {
            - {
                - id = 1
                - description = "Bicep curls"
                div plates {
                    int p1 = 1
                    int p2 = 0
                    int p3 = 0
                    int p4 = 1
                }
                - sets = 3
                - reps = 19
            }
            - {
                - id = 2
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
                - id = 3
                - description = "Deltoids: Shoulder Press"
                div plates {
                    int p1 = 0
                    int p2 = 0
                    int p3 = 0
                    int p4 = 1
                }
                - sets = 3
                - reps = 17
            }
        }
    }
}
