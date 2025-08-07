tab test_save_filtering {
    div bug (margin=0) {
        string description (width=40%, margin=0, spacing=0) = "bug description"
        checkbox in_demo (width=5%, margin=0, spacing=0) = true
    }
    text header = "Bug list:"
    list bugs (entry=<../bug>, layout=vertical, spacing=0, margin=0) {
        - {
            - description = "Test bug 1"
            - in_demo = true
        }
        - {
            - description = "Test bug 2"
            - in_demo = false
        }
    }
}
