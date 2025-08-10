div ActionFeatureTest (layout = vertical, spacing=md) {
    // Counter demo
    int counter = 0
    button increment (label="Increment") { on click { inc(path="../counter", by=1) } }

    div (hidden=true) { // Template for tasks (hidden so it doesn't render)
        div Task {
            string id (hidden=true) = ""
            string title = ""
        }
    }

    // Template-based list with key identity
    list Tasks (entry=<Task>, key="id", spacing=sm) {
        - Task { id = "x1" title = "First" }
        - Task { id = "b"  title = "Second" }
        - Task { id = "c"  title = "Third" }
    }

    // Actions to test list mutations
    button ensure_a1 (label="Ensure A1 in Tasks") {
        on click { ensure_in_list(list="/ActionFeatureTest/Tasks", keyField="id", keyValue="a1", template="<Task>") }
    }

    button remove_x1 (label="Remove x1 from Tasks") {
        on click { remove(list="/ActionFeatureTest/Tasks", keyField="id", keyValue="x1") }
    }

    button move_b_front (label="Move 'b' to front") {
        on click { move(from="/ActionFeatureTest/Tasks", keyField="id", keyValue="b", at=0) }
    }

    button append_blank (label="Append blank Task") {
        on click { append(list="/ActionFeatureTest/Tasks", template="<Task>") }
    }

    // Sorting demos
    button sort_title_asc (label="Sort Tasks by Title (A→Z)") {
        on click { sort(list="/ActionFeatureTest/Tasks", by="$(x/title)", order="asc") }
    }

    button sort_title_desc (label="Sort Tasks by Title (Z→A)") {
        on click { sort(list="/ActionFeatureTest/Tasks", by="$(x/title)", order="desc") }
    }

    // Simple value list
    list Names (entry=string, spacing=sm) {
        - "Eve"
    }
    button append_name (label="Append 'Alice'") {
        on click { append(list="/ActionFeatureTest/Names", value="Alice") }
    }

    // Timer demo
    div TimerDemo {
        int A (label="Counter A") = 0
        
        // Initialize T with a static timestamp; button will set it to now()+10s when needed
        timestamp T (label="Trigger Timestamp") = $(now())

        // Timer nodes: fire when now() >= T; one-shot (deactivate on fire)
        timer after_10s (active=false, at=$(../T), label="10s timer") {
            on timeout { inc(path="../A", by=1) }
        }
        
        // A second timer to demonstrate multiple timers in one container
        timer after_20s (active=false, at=$(../T), label="20s timer") {
            on timeout { inc(path="../A", by=1) }
        }
        
        button add10_activate (label="Add 10s to T and activate 10s timer") {
            on click {
                set_now_ts(path="../T", offset=10)
                set(path="../after_10s.active", value=true)
            }
        }
    }
}
