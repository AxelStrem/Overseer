tab test {
    div item (margin=0) {
        string description (width=40%, margin=0, spacing=0) = "template description"
    }
    
    list items (entry=<item>, spacing=0) {
        - {
            - description = "Test item 1"
        }
        - {
            - description = "Test item 2"
        }
    }
}
