// Tabs anywhere, not only at the top.
//
// The tabs one parent holds are one tab system: a bar of their labels and a page each, one showing
// at a time, standing where the first of them stands. The parent's layout places the bar - above
// the pages, or beside them when the parent lays its children out in a row. Which tab is showing
// is the viewer's, remembered for the session and never written here.
tab tabs (label="Tabs", mutable=true) {
    text intro (markdown=true) = "## A project, in tabs"

    // The bar above the pages. The text beside the tabs is drawn as usual, before them.
    div plan (layout="vertical") {
        text note (markdown=true) = "Two tabs in a vertical div: the bar sits above the pages."

        tab open (label="Open", hover-text="What is still to do") {
            string first = "Write the parser"
            string second = "Draw the tabs"
        }

        tab done (label="Finished", hover-text="What has been done") {
            string third = "Read the file"

            // A tab inside a tab: a system of its own, on this page.
            div detail (layout="vertical") {
                tab week (label="This week") {
                    int count = 3
                }
                tab month (label="This month") {
                    int count = 12
                }
            }
        }
    }

    // The bar beside the pages, because this div lays its children out in a row.
    div side (layout="horizontal") {
        tab one (label="One") {
            string what = "the first page"
        }
        tab two (label="Two") {
            string what = "the second page"
        }
    }

    // Two systems at one level: each in a div of its own.
    div both (layout="vertical") {
        div left (layout="vertical") {
            tab a (label="A") {
                string what = "system one, page A"
            }
            tab b (label="B") {
                string what = "system one, page B"
            }
        }
        div right (layout="vertical") {
            tab c (label="C") {
                string what = "system two, page C"
            }
            tab d (label="D") {
                string what = "system two, page D"
            }
        }
    }

    // A list whose entries are tabs: one page an entry.
    div (hidden=true) {
        // Each entry's header is its date.
        tab Day (label=$(date)) {
            string date = ""
            string mood = ""
        }
    }

    list Days (entry=<Day>, key="date", layout="vertical") {
        - {
            - date = "2026-10-01"
            - mood = "busy"
        }
        - {
            - date = "2026-10-02"
            - mood = "calm"
        }
    }
}
