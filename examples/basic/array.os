tab week (label="Week", mutable=true) {
    div (hidden=true) {
        div Day (layout="vertical", spacing=2) {
            int steps = 0
            int gain = $(index() > 0 ? steps - Steps.at(index() - 1)/steps : 0)
        }
    }

    array Steps (entry=<Day>, length=7) {
        - A {
            - steps = 8200
        }
        - B {
            - steps = 10400
        }
        - D {
            - steps = 6100
        }
    }

    array Goals (entry=int, length=3, layout="vertical") {
        - A = 8000
    }

    div (layout="horizontal") {
        int total (label="Steps") = $(Steps.cells().sum(|d| d/steps))
        int wednesday (label="Wednesday") = $(Steps.at(2)/steps)
        int goal (label="Goal") = $(Goals.at(0))
    }

    button rest (label="Log Sunday") {
        on click {
            set (path="../Steps[6]/steps") = 3000
            set (path="../Goals/B") = 9000
        }
    }
}
