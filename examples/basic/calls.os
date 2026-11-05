tab calls (label="Calls", mutable="guarded") {
    string line (label="Line") = "Overseer"
    string said (label="Said") = ""

    div (layout="horizontal") {
        button copy (label="Copy the line") {
            on click {
                call (name="copy", into="../said") {
                    - text = $(../line)
                }
            }
        }

        button dark (label="Dark") {
            on click {
                call (name="look") {
                    - mode = "dark"
                }
            }
        }

        button light (label="Light") {
            on click {
                call (name="look") {
                    - mode = "light"
                }
            }
        }

        button file (label="Open a file") {
            on click {
                call (name="open_file", into="../said")
            }
        }

        button enums (label="Go to enums") {
            on click {
                call (name="open") {
                    - path = "basic/enum.os"
                }
            }
        }
    }

    div (layout="horizontal") {
        textbox current (placeholder="current password", kind="secret") = ""
        textbox password (placeholder="new password", kind="secret") = ""

        button change (label="Change the password") {
            on click {
                call (name="change_password", into="../said") {
                    - current = <../current>
                    - password = <../password>
                }
            }
        }
    }
}
