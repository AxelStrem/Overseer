tab main {
    div (hidden=true) {
        div T {
            div {
                int A (label="A") = 1
            }

            int B = 2
            int C = $(A*B)
        }
    }


    int total = $(L.map(|x| x/C).sum())

    list L (entry=<T>) {
        - {
        }
    }
}
