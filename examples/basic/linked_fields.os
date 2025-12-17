tab main {

    // Minimal focus: a selected date (day precision) and Prev/Next navigation
    div (hidden=true) {

        div T
        {
            int id = 0
            int value = 10
        }
    }
    
    div Linked (link="/main/L[key=5]") {
    }

    list L (entry=<T>, key="id") {
        - {
            - id = 5
            - value = 15
        }
    }
}
