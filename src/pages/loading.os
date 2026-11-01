div page (layout="vertical", alignment="center", height=1fr, border-style=none) {
    context theme {
        - backdrop (base=none) {
            - watchers {
                - kind = "icon"
                - source = "eye-*"
                - colour = $(muted)
                - opacity = 0.35
                - size = "3%"
                - count = 24
                - motion = "sparkle"
                - period = "7s"
            }
            - eye {
                - kind = "icon"
                - source = "eye"
                - colour = $(muted)
                - opacity = 0.12
                - size = "70%"
                - motion = "appear"
                - period = "6s"
            }
        }
    }

    div (layout="horizontal", alignment="center", height=1fr, width=fit-content) {
        text title (markdown=true, font-size=44px, font-weight=300) = "<motion=pulse | OVERSEER>"
    }
}
