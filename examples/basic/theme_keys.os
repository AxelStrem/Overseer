div (hidden=true) {
    div Kind (layout="horizontal") {
        string tag = ""
        string colour = "#6b7280"
    }
    div Module (layout="horizontal") {
        string name (label="Module", width=30%) = ""
        tags kinds (label="Kinds", vocabulary="/catalogue/Kinds", width=30%) = ""
        int inputs (label="In", width=10%) = 0
        int outputs (label="Out", width=10%) = 0
        button add (label="Add") {
            on click {
                inc (path="../inputs") = 1
            }
        }
        string note (label="Note", span="row") = ""
    }
}

context theme {
    - size = "14px"
    - leading = 1.5
    - numerals = "tabular"
    - radius = "4px"
    - mono = $(font)
    - field {
        - value-frame = "none"
        - number-align = "start"
    }
    - table {
        - frame = "none"
        - heading {
            - size = "13px"
            - weight = 600
            - case = "upper"
            - tracking = 0.05
            - rule = "1px"
        }
        - row {
            - padding = "7px"
            - rule = "1px"
            - pointed = "tint"
        }
        - cell-padding = "8px"
        - detail = "bar"
        - detail-padding = "10px 16px"
        - field {
            - spacing = "0px"
        }
        - button {
            - height = "22px"
            - spacing = "0px"
            - padding = "0 8px"
            - size = "11.5px"
            - weight = 600
            - style = "outline"
        }
    }
    - chip {
        - size = "12px"
        - padding = "1px 8px"
        - remove = "pointed"
        - add = "pointed"
    }
    - tab {
        - size = "15px"
        - weight = 500
        - padding = "10px 18px"
    }
    - filter {
        - width = "320px"
        - size = "14px"
        - padding = "7px 12px"
    }
}

tab catalogue (label="Catalogue", mutable=true) {
    list Kinds (entry=<Kind>, key="tag", hidden=true) {
        - {
            - tag = "audio"
            - colour = "#b0603f"
        }
        - {
            - tag = "control"
            - colour = "#3f7fb0"
        }
    }

    filter (target="/catalogue/Modules", text="name, note", tags="kinds", vocabulary="/catalogue/Kinds", label="find") { }

    list Modules (entry=<Module>, key="name", view="table", header=true) {
        - {
            - name = "Oscillator"
            - kinds = "audio"
            - inputs = 2
            - outputs = 1
            - note = "Four waveforms, hard sync."
        }
        - {
            - name = "Sequencer"
            - kinds = "control"
            - outputs = 2
            - note = "Sixteen steps, two lanes."
        }
        - {
            - name = "Filter"
            - kinds = "audio, control"
            - inputs = 3
            - outputs = 1
            - note = "Low-pass and band-pass."
        }
    }

    button reset (label="Reset counts") {
        on click {
            set (path="../Modules/[Oscillator]/inputs") = 0
        }
    }
}

tab about (label="About") {
    text info = "A catalogue drawn in a theme of its own."
}
