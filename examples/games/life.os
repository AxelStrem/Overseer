context theme {
    - backdrop {
        - glow {
            - kind = "gradient"
            - from = "#22c55e2e"
            - to = "transparent"
            - angle = $(160 + /life/generation * 4 % 360)
            - center = $(20 + /life/population / 8 + "%")
        }
        - base {
            - kind = "gradient"
            - from = "#03120b"
            - to = "#0b2117"
        }
    }
    - text = "#d1fae5"
    - accent = "#22c55e"
    - mono = "system-ui, sans-serif"
    - field {
        - value-frame = "none"
        - spacing = "0px"
        - number-align = "start"
    }
}

div (hidden=true) {
    div Cell (width=16, height=16, padding=0, margin=0, border-radius=5,
              background-color=$(./alive ? "#4ade80" : "#0f2a1d"),
              shadow=$(./alive ? "0 0 8px #4ade80, inset 0 0 0 1px #bbf7d0" : "inset 0 0 0 2px #06170f"),
              motion=$(./alive ? "fade" : "")) {
        div (hidden=true) {
            bool alive = false
            int x = $(column())
            int y = $(row())
            int left = $((x + /life/W - 1) % /life/W)
            int right = $((x + 1) % /life/W)
            int up = $((y + /life/H - 1) % /life/H)
            int down = $((y + 1) % /life/H)
            int n = $((Board.at(left, up)/alive ? 1 : 0) + (Board.at(x, up)/alive ? 1 : 0) + (Board.at(right, up)/alive ? 1 : 0)
                    + (Board.at(left, y)/alive ? 1 : 0) + (Board.at(right, y)/alive ? 1 : 0)
                    + (Board.at(left, down)/alive ? 1 : 0) + (Board.at(x, down)/alive ? 1 : 0) + (Board.at(right, down)/alive ? 1 : 0))
            bool next = $(alive ? n == 2 || n == 3 : n == 3)
        }

        on click {
            toggle (path="./alive")
        }
    }
}

div life (mutable="guarded", layout="vertical", alignment="center", tick=$(running ? speed + "ms" : 0)) {
    int W (hidden=true) = 20
    int H (hidden=true) = 20
    timestamp seed (hidden=true) = ""
    bool running (hidden=true) = false
    int speed (hidden=true) = 200
    int generation (hidden=true) = 0
    int population (hidden=true) = $(Board.cells().filter(|c| c/alive).count())

    on load {
        press (path="/life/shuffle")
    }

    on tick {
        press (path="/life/step")
    }

    text title (hide-labels=true, markdown=true, font-size=22px, font-weight="bold", font-color="#86efac") = "<motion=wave | Game of Life>"

    div (layout="horizontal", alignment="center", spacing=20) {
        string gen_shown (hide-labels=true, font-size=15px) = $("<icon=history> Generation " + generation)
        string pop_shown (hide-labels=true, font-size=15px) = $("<icon=sprout> " + population + " alive")
        button pace (icon="gauge", label=$(speed >= 400 ? "Slow" : speed >= 200 ? "Normal" : "Fast")) {
            on click {
                set (path="/life/speed") = $(/life/speed >= 400 ? 200 : /life/speed >= 200 ? 100 : 400)
            }
        }
    }

    div (width=fit-content, alignment="center", padding=10, border-radius=14, shadow="floating", margin-top=8, margin-bottom=8,
         background-color="#06170f") {
        grid Board (entry=<Cell>, columns=20, rows=20, header=false, width=fit-content, border-style=none) { }
    }

    div (layout="horizontal", alignment="center") {
        button go (icon=$(running ? "pause" : "play"), label=$(running ? "Stop" : "Start"), motion=$(running ? "pulse" : "")) {
            on click {
                toggle (path="/life/running")
            }
        }
        button step (icon="step-forward", label="Step") {
            on click {
                each (in=$(/life/Board.cells().filter(|c| c/alive != c/next)), as="c") {
                    toggle (path="c/alive")
                }
                inc (path="/life/generation")
            }
        }
        button clear (icon="eraser", label="Clear") {
            on click {
                set (path="/life/running") = false
                each (in=$(/life/Board.cells().filter(|c| c/alive)), as="c") {
                    set (path="c/alive") = false
                }
                set (path="/life/generation") = 0
            }
        }
    }

    div (layout="horizontal", alignment="center") {
        button shuffle (icon="dices", label="Random") {
            on click {
                set_now_ts (path="/life/seed")
                each (in=$(/life/Board.cells()), as="c") {
                    set (path="c/alive") = $(random(/life/seed, c/x, c/y, 100) < 30)
                }
                set (path="/life/generation") = 0
            }
        }
        button glider (icon="send", label="Glider") {
            on click {
                press (path="/life/clear")
                set (path="/life/Board[1,0]/alive") = true
                set (path="/life/Board[2,1]/alive") = true
                set (path="/life/Board[0,2]/alive") = true
                set (path="/life/Board[1,2]/alive") = true
                set (path="/life/Board[2,2]/alive") = true
            }
        }
        button pulsar (icon="sparkles", label="Pulsar") {
            on click {
                each (in=$(/life/Board.cells()), as="c") {
                    set (path="c/alive") = $((c/y == 4 || c/y == 9 || c/y == 11 || c/y == 16) && (c/x >= 6 && c/x <= 8 || c/x >= 12 && c/x <= 14)
                                          || (c/x == 4 || c/x == 9 || c/x == 11 || c/x == 16) && (c/y >= 6 && c/y <= 8 || c/y >= 12 && c/y <= 14))
                }
                set (path="/life/generation") = 0
            }
        }
    }

    string hint (hide-labels=true, font-size=13px, font-color="#6ee7b7aa") = "Tap a cell to bring it to life or clear it"
}
