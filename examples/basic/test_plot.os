chart C {
    plot P (label="Actual", y=$(|x| x/value), color=#4A90E2, source="/L", x=$(|x| x/id))
    plot P2 (label="Scaled", y=$(|x| x/value * 1.2), color=#E24A4A, source="/L", x=$(|x| x/id))
}
div V {
    int id = 0
    float value = 1
}
list L (entry=<V>) {
    - {
        - id = 1
        - value = 2
    }
    - {
        - id = 2
        - value = 3
    }
    - {
        - id = 3
        - value = 2.5
    }
    - {
        - id = 4
        - value = 2.7
    }
    - {
        - id = 5
        - value = 2.6
    }
    - {
        - id = 6
        - value = 2.4
    }
    - {
        - id = 7
        - value = 2.2
    }
    - {
        - id = 8
        - value = 1.5
    }
    - {
        - id = 9
        - value = 2
    }
}
