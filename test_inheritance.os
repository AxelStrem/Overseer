tab test_inheritance {
    div item {
        string name (width=60%, color=blue) = "default"
        int value (width=40%, color=red) = 0
    }
    
    list items (entry=<../item>) {
        - {
            - name = "Test Item"
        }
    }
}
