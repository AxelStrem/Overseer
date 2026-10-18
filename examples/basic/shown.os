div (hidden=true) {
    div Module (layout="horizontal") {
        string name (label="Module") = ""
        int hp (label="HP") = 0
        int main (label="Main", shown=$(main == 0 ? "" : main)) = 0
        int perf (label="Perf", shown=$(perf == 0 ? "" : perf)) = 0
        int swap (label="Swap", shown=$(swap == 0 ? "" : swap)) = 0
        string note (label="Note", shown=$(note == "" ? "?" : note)) = ""
    }
}

tab modules (label="Modules", mutable=true) {
    int hp_in_main (label="HP in main", mutable=false) = $(Owned.map(|x| x/hp * x/main).sum())

    list Owned (entry=<Module>, key="name", view="table", header=true, sortable="main, perf, swap") {
        - {
            - name = "Oscillator"
            - hp = 4
            - main = 1
        }
        - {
            - name = "Filter"
            - hp = 6
            - perf = 2
            - note = "spare"
        }
        - {
            - name = "Envelope"
            - hp = 2
            - swap = 1
        }
    }
}
