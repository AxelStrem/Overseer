tab days (label="Days", mutable=true) {
    div (hidden=true) {
        div Day (background-color=$(commits > 6 ? "#216e39" : commits > 3 ? "#30a14e" : commits > 0 ? "#9be9a8" : "")) {
            int commits (label="Commits") = 0
        }
    }

    calendar Contributions (entry=<Day>, from=$(date_add_days(today(), -364)), to=$(today()), layout="year") {
        - 2026-09-14 {
            - commits = 2
        }
        - 2026-10-01 {
            - commits = 5
        }
        - 2026-10-02 {
            - commits = 9
        }
        - 2026-11-03 {
            - commits = 4
        }
    }

    date showing (label="Month of", mutable="guarded") = $(today())

    div (layout="horizontal") {
        button previous (label="Previous month") {
            on click {
                set (path="../showing") = $(date_add_days(../showing, -day_of_month(../showing)))
            }
        }
        button next (label="Next month") {
            on click {
                set (path="../showing") = $(date_add_days(../showing, 32 - day_of_month(../showing)))
            }
        }
    }

    calendar Steps (entry=int, span="month", at=$(../showing), layout="month") {
        - 2026-11-02 = 8200
        - 2026-11-03 = 10400
    }

    int steps (label="Steps this month", mutable=false) = $(Steps.cells().sum())
}
