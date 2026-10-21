context sunset {
    - dusk {
        - kind = "gradient"
        - from = "#f9731688"
        - to = "#7c3aed88"
        - angle = 160
    }
}

context theme {
    - backdrop {
        - glow {
            - kind = "gradient"
            - from = $(accent + "55")
            - to = "transparent"
            - angle = 135
            - center = "20%"
            - sharpness = "30%"
        }
        - base {
            - kind = "gradient"
            - from = $(background)
            - to = $(surface)
        }
    }
    - button {
        - backdrop {
            - shine {
                - kind = "gradient"
                - from = "rgba(255, 255, 255, 0.35)"
                - to = "transparent"
                - sharpness = "100%"
            }
        }
    }
}

tab sky (label="Backdrops") {
    string note (label="Note") = "The page is painted with two gradients, one over the other."
    button press (label="A button with a shine") { }

    div warm (layout="vertical") {
        context theme {
            - backdrop {
                - glow {
                    - from = $(warning + "88")
                }
            }
        }
        string what (label="Warm") = "This part changes one layer and keeps the other."
    }

    div bare (layout="vertical") {
        context theme {
            - backdrop (base=none)
        }
        string what (label="Bare") = "This part drops the backdrop and shows the background colour alone."
    }

    div evening (layout="vertical") {
        context theme {
            - backdrop (base=<sunset>)
        }
        string what (label="Evening") = "This part takes its layers from the sunset written at the top."
    }

    div striped (layout="vertical") {
        context theme {
            - backdrop (base=none) {
                - stripe {
                    - kind = "gradient"
                    - from = $(accent + "44")
                    - to = $(success + "44")
                    - angle = 90
                    - sharpness = "100%"
                }
            }
        }
        string what (label="Striped") = "This part starts over with a hard edge of its own."
    }
}
