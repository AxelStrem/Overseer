// Written by the server from its accounts. A display name is changed in the profile.

div (hidden=true) {
    div Person (layout="horizontal") {
        string tag = ""
        string name = ""
        string colour = "#6b7280"
    }
}

list People (entry=<Person>, key="tag") {
    - {
        - tag = "ap31"
        - name = "Dmitrii"
    }
    - {
        - tag = "bob"
        - name = "Bob"
    }
}
