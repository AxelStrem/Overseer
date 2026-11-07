div (hidden=true) {
    div Item (layout="horizontal") {
        string title = ""
        button more (label="…") {
            on click {
                toggle_fold (path="../menu")
            }
        }
        div menu (foldable=true, folded=true, float=true, layout="vertical") {
            button rename (label="Rename") {
                on click {
                    start_editing (path="../../title")
                    fold (path="..")
                }
            }
            button remove (label="Remove") {
                on click {
                    remove (from="/items/Items", keyField="title", keyValue=$(../../title))
                }
            }
        }
    }
}

tab items (label="Items", mutable=true) {
    list Items (entry=<Item>, key="title", layout="vertical") {
        - {
            - title = "Water the plants"
        }
        - {
            - title = "Call the bank"
        }
        - {
            - title = "Buy bread"
        }
    }
}
