tab test {
    div bug (margin=0) {
        string description (width=40%, margin=0, spacing=0) = "bug description"
        checkbox in_demo (width=5%, margin=0, spacing=0) = true
        string commentary (width=20%, margin=0, spacing=0) = ""
        int story_points (width=10%, margin=0, spacing=0) = 1
        int complete (width=10%, margin=0, spacing=0) = 0
    }
    text header = "Bug list:"
    list bugs (entry=<../bug>, layout=vertical, spacing=0, margin=0) {
        - {
            - description = "Enemy Sub-Units loot keeps the parent"
            - in_demo = true
            - commentary = ""
            - story_points = 3
            - complete = 3
        }
        - {
            - description = "Another test bug"
            - in_demo = false
            - commentary = "test comment"
            - story_points = 2
            - complete = 1
        }
    }
}
