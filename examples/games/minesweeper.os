context theme {
    - backdrop {
        - glow {
            - kind = "gradient"
            - from = "#38bdf833"
            - to = "transparent"
            - angle = 150
            - center = "30%"
        }
        - base {
            - kind = "gradient"
            - from = "#0f172a"
            - to = "#1e293b"
        }
    }
    - text = "#e2e8f0"
    - mono = "system-ui, sans-serif"
    - field {
        - value-frame = "none"
        - spacing = "0px"
        - number-align = "start"
    }
}

div (hidden=true) {
    div Cell (hold="2s", width=34, height=34, padding=0, margin=0, alignment="center", border-radius=4,
              background-color=$(./opened ? (./mine ? "#ef4444" : "#cbd5e1") : (/game/lost && ./mine ? "#fca5a5" : "#64748b")),
              shadow=$(./opened ? "inset 0 0 0 1px #94a3b8" : "inset 3px 3px 0 #94a3b8, inset -3px -3px 0 #334155")) {
        div (hidden=true) {
            bool opened = false
            bool flagged = false
            int c = $(column())
            int r = $(row())
            int roll = $(random(/game/seed, column(), row(), 1000000))
            bool mine = $(Field.cells().filter(|x| x/roll < roll).count() < 10)
            int around = $(Field.cells().filter(|x| x/mine && x/c >= c - 1 && x/c <= c + 1 && x/r >= r - 1 && x/r <= r + 1).count())

            button dig {
                on click {
                    if (cond=$(!../opened && !../flagged)) {
                        set (path="../opened") = true
                        if (cond=$(!../mine && ../around == 0)) {
                            each (in=$(/game/Field.cells().filter(|x| !x/opened && !x/flagged && x/c >= ../c - 1 && x/c <= ../c + 1 && x/r >= ../r - 1 && x/r <= ../r + 1)), as="n") {
                                press (path="n/dig")
                            }
                        }
                    }
                }
            }
        }

        string face (hide-labels=true, padding=0, margin=0, font-weight="bold", font-size=18px,
                     font-color=$(!../opened ? (../flagged || /game/won ? "#fbbf24" : "#0f172a") : ../mine ? "#0f172a" : ../around == 1 ? "#1d4ed8" : ../around == 2 ? "#15803d" : ../around == 3 ? "#dc2626" : ../around == 4 ? "#1e3a8a" : ../around == 5 ? "#7f1d1d" : ../around == 6 ? "#0f766e" : ../around == 7 ? "#111827" : "#6b7280"))
            = $(../opened ? (../mine ? "<icon=bomb>" : ../around == 0 ? "" : "" + ../around) : ../flagged || /game/won && ../mine ? "<icon=flag>" : /game/lost && ../mine ? "<icon=bomb>" : "")

        on click {
            if (cond=$(!/game/over && !./opened)) {
                toggle (path="./flagged")
            }
        }

        on hold {
            if (cond=$(!/game/over)) {
                press (path="./dig")
            }
        }
    }
}

div game (mutable="guarded", layout="vertical", alignment="center") {
    timestamp seed (hidden=true) = ""
    int opened (hidden=true) = $(Field.cells().filter(|x| x/opened && !x/mine).count())
    bool lost (hidden=true) = $(Field.cells().filter(|x| x/opened && x/mine).count() > 0)
    bool won (hidden=true) = $(!lost && opened == 71)
    bool over (hidden=true) = $(lost || won)

    on load {
        press (path="/game/new_game")
    }

    div (layout="horizontal", alignment="center", spacing=16) {
        string flags_left (hide-labels=true, font-size=20px, font-weight="bold", font-color="#fbbf24") = $("<icon=flag> " + (won ? 0 : 10 - Field.cells().filter(|x| x/flagged).count()))
        div new_game (width=44, height=44, padding=0, padding-left=10, layout="horizontal", alignment="center", border-radius=22, shadow="lifted",
                      background-color=$(lost ? "#ef4444" : won ? "#22c55e" : "#fbbf24")) {
            string face (hide-labels=true, padding=0, margin=0, font-size=24px, font-color="#0f172a")
                = $(lost ? "<icon=frown>" : won ? "<icon=party-popper>" : "<icon=smile>")

            on click {
                set_now_ts (path="/game/seed")
                each (in=$(/game/Field.cells().filter(|x| x/opened || x/flagged)), as="c") {
                    set (path="c/opened") = false
                    set (path="c/flagged") = false
                }
            }
        }
        string status (hide-labels=true, font-size=16px) = $(lost ? "Boom. Tap the face to try again." : won ? "Cleared!" : "Tap to flag, hold to dig")
    }

    div (width=fit-content, border-radius=12, shadow="floating", margin-top=12) {
        context theme {
            - backdrop (base=none) {
                - board {
                    - kind = "gradient"
                    - from = "#334155"
                    - to = "#0f172a"
                    - angle = 160
                }
            }
            - field {
                - leading = 1.9
            }
        }
        grid Field (entry=<Cell>, columns=9, rows=9, header=false, width=fit-content, border-style=none) { }
    }
}
