tab week (label="Meal plan", mutable=true) {
    div (hidden=true) {
        div Meal (layout="horizontal", spacing=6) {
            string emoji = ""
            string name = ""
        }
    }

    list Dishes (entry=<Meal>, key="name", layout="horizontal", copyable=true) {
        - {
            - emoji = "🍝"
            - name = "Pasta"
        }
        - {
            - emoji = "🥗"
            - name = "Salad"
        }
        - {
            - emoji = "🍲"
            - name = "Soup"
        }
    }

    div (layout="horizontal") {
        list Monday (entry=<Meal>, width=240px, movable=true, copyable=true) {
            - {
                - emoji = "🥗"
                - name = "Salad"
            }
        }

        list Tuesday (entry=<Meal>, width=240px, movable=true, copyable=true) { }
    }
}
