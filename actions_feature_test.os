div ActionFeatureTest (layout = vertical, spacing=md) {
    // Counter demo
    int counter = 0
    button increment (label="Increment") { on click { inc(path="../counter", by=1) } }

    // Template for tasks (hidden so it doesn't render)
    div Task (hidden=true) {
        string id = ""
        string title = ""
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

    // Simple value list
    list Names (entry=string, spacing=sm) {
        - "Eve"
    }
    button append_name (label="Append 'Alice'") {
        on click { append(list="/ActionFeatureTest/Names", value="Alice") }
    }
}
