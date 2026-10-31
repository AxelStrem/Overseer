context theme {
    - hold {
        - style = "fill"
    }
}

tab motion (label="Motion", mutable="guarded") {
    int presses (label="Presses") = 0
    bool busy (label="Busy") = false
    bool flagged (label="Flagged") = false

    div (layout="horizontal") {
        button pop (label="Pop", motion=$("pop:" + presses)) {
            on click {
                inc (path="/motion/presses")
            }
        }
        button shake (label="Shake", motion=$("shake:" + presses)) {
            on click {
                inc (path="/motion/presses")
            }
        }
        button fade (label="Fade", motion=$("fade:" + presses)) {
            on click {
                inc (path="/motion/presses")
            }
        }
        button burst (label="Burst", motion=$("burst:" + presses)) {
            on click {
                inc (path="/motion/presses")
            }
        }
        button pulse (label="Pulse", motion=$(busy ? "pulse" : "")) {
            on click {
                toggle (path="/motion/busy")
            }
        }
    }

    div flag (hold="1s", border-style="solid", motion=$(flagged ? "pop" : "")) {
        string shown (label="Hold to flag") = $(flagged ? "<icon=flag>" : "")

        on hold {
            toggle (path="/motion/flagged")
        }
    }
}
