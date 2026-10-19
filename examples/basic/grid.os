tab week (label="Week", mutable=true) {
    div (hidden=true) {
        div Shift (layout="vertical", spacing=2) {
            string who = ""
            int hours = 0
        }
    }

    grid Rota (entry=<Shift>, columns=5, rows=3) {
        - A1 {
            - who = "Ann"
            - hours = 8
        }
        - C2 {
            - who = "Bob"
            - hours = 6
        }
    }

    grid Steps (entry=int, columns=7, rows=2, header=false) {
        - A1 = 8200
        - B1 = 10400
        - D2 = 6100
    }

    div (layout="horizontal") {
        int hours (label="Hours") = $(Rota.cells().sum(|s| s/hours))
        int wednesday (label="Wednesday") = $(Rota.column(2).sum(|s| s/hours))
        int first_shift (label="First shift") = $(Rota.row(0).sum(|s| s/hours))
        int steps (label="Steps") = $(Steps.cells().sum())
        int monday (label="Monday") = $(Steps.at(0, 0))
    }

    button cover (label="Cover Friday morning") {
        on click {
            set (path="../Rota[4,0]/who") = "Cleo"
            set (path="../Rota/E1/hours") = 4
        }
    }
}
