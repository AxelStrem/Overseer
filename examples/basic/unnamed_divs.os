
div (hidden=true) {
    div T {
        div {
            int A (label="X") = 1
        }

        int B = 2
        int C = $(A*B)
    }
}
list L (entry=<T>) {
    - {
    }
}
