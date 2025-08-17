div T {
    int A = 1
    int B = 2
    int C = 3
}
<T> I {
    - A = 3
    - B = 2
}

// List as an example of correct behavior:
list L(entry=<T>) {
    - {
        - A = 3
        - B = 2
    }
}