mount people (source="people.os/People", lazy=false, hidden=true, mutable=false) { }

div (hidden=true) {
    div Note (layout="horizontal") {
        enum author (label="By", vocabulary="/people/People", width=20%) = ""
        string body (label="Note") = ""
    }
}

tab notes (label="Notes", mutable=true) {
    context vocabulary = "/people/People"

    string viewer (label="Signed in as", mutable=false) = $(me/name)

    list Notes (entry=<Note>, layout="vertical") {
        - {
            - author = "ap31"
            - body = "Ask <#bob> about the shelves"
        }
        - {
            - author = "bob"
            - body = "Measured them, see <#ap31>'s plan"
        }
    }

    button add (label="Add a note") {
        on click {
            append (list="../Notes") {
                - author = $(me/handle)
            }
        }
    }
}
