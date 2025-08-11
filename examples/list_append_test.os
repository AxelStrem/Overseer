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
<T> T2 {
    - i = 30
}
button C (label="Button 2") {
    on click {
        append (list="/L", template="<T2>")
    }
}
int x = 50
button D (label="button 3") {
    on click {
        append (template="<T>", list="/L") {
            - i = $(x)
        }
    }
}
list L (layout="horizontal", entry=<T>) {
}
