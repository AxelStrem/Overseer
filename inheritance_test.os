tab inheritance_test {
    div item (margin=5px) {
        string name (width=50%, color=blue) = "default name"
        int priority (width=30%, color=red) = 1
        checkbox done (width=20%) = false
    }
    
    list items (entry=<item>, spacing=0) {
        - {
            - name = "Override name only"
        }
        - {
            - name = "Override multiple"
            - priority = 5
        }
        - {
            - name = "Complete item"
            - priority = 3
            - done = true
        }
    }
}
