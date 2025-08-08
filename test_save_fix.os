tab test_save {
    div item (margin=5px) {
        string name (width=60%, color=blue) = "default"
        int value (width=40%) = 0
    }
    
    list items (entry=<item>, spacing=0) {
        - {
            - name = "Test Item"
        }
        - {
            - name = "Another Item"
            - value = 42
        }
    }
}
