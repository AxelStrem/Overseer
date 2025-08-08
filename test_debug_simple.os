tab test_debug {
    div item (margin=5) {
        string name (width=50%) = "test"
    }
    list items (entry=<item>) {
        - {
            - name = "Item 1"
        }
    }
}
