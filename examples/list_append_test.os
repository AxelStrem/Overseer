div T {
    int i = 10
    int ii = $(2*i)
}

button B (label="Button 1") {
    on click {
        append (template="<T>", list="/L") {
            - i = 20
        }
    }
}

button B2 (label="Button 2") {
    on click {
        append (template="<T>", list="/L") {
            - i = 25
        }
    }
}


list L (layout="horizontal", entry=<T>) {
}
