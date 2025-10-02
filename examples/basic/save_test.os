
int int_field(mutable = true) = 22

div item (margin=0) {
    string description (width=40%, margin=0, spacing=0) = "template description"
}

list items (entry=<item>, spacing=0) {
    - {
        - description = "template description"
    }
    - {
        - description = "Test item 2"
    }
}
