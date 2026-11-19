tab canvas (label="Canvas", mutable=true) {
    textbox find (placeholder="find", narrows="/canvas/Notes", by="title") = ""

    div (hidden=true) {
        div Card (layout="vertical", padding=6, border-radius=8px, spacing=4) {
            string title (label="") = ""
            text body (label="") = ""
            float x (hidden=true) = 0
            float y (hidden=true) = 0
        }
    }

    list Notes (entry=<Card>, view="canvas", x="x", y="y") {
        - {
            - title = "Parser"
            - body = "Reads a document."
            - x = 20
            - y = 20
        }
        - {
            - title = "Resolver"
            - body = "Works out its values."
            - x = 260
            - y = 20
        }
        - {
            - title = "Writer"
            - body = "Gives back the bytes it was given."
            - x = 140
            - y = 210
        }
        - {
            - title = "Not placed yet"
        }
    }

    button add (label="Add a note") {
        on click {
            append (list="../Notes") {
                - title = "Untitled"
            }
        }
    }
}
