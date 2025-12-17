div Template {
    int field = 1
    div wrapper {
        int nested = 2
    }
}

<Template> Instance {
    - field = 42
    - wrapper {
        - nested = 5
    }
}

list Records (entry=<Template>) {
    // comment about first entry
    - {
        - field = 7
    }

    // second entry comment
    - {
        - field = 8
    }
}
