int a = 10
int b = $(2*a)

chart C {
    plot P (x=$(|x| x), source="/data", y=$(|x| x*x))
}

list data(entry=float) {
    - 0.0
    - 1.0
    - 2.0
    - 3.0
    - 4.0
    - 5.0
    - 6.0
}