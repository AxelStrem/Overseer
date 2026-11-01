div page (layout="vertical", alignment="center", height=1fr, border-style=none) {
    context theme {
        - backdrop (base=none) {
            - eye {
                - kind = "icon"
                - source = "eye-closed"
                - colour = $(muted)
                - opacity = 0.1
                - size = "70%"
                - motion = "appear"
                - period = "2s"
            }
        }
    }

    div (layout="horizontal", alignment="center", height=1fr, width=fit-content) {
        div (alignment="center", width=fit-content) {
            text title (font-size=28px, font-weight=600) = "Something went wrong"
            text message (font-color=$(theme/muted)) = "The server could not be reached."
        }
    }
}
