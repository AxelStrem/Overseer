tab clock (label="Clock", mutable=true, tick=$(running ? "500ms" : 0)) {
    bool running (label="Running", mutable="guarded") = false
    int beats (label="Beats", mutable="guarded") = 0

    div (layout="horizontal") {
        button go (label="Start / stop") {
            on click {
                toggle (path="/clock/running")
            }
        }
        button reset (label="Reset") {
            on click {
                set (path="/clock/beats") = 0
            }
        }
    }

    on tick {
        inc (path="/clock/beats")
    }
}
