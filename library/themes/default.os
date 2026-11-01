context theme {
    - light {
        - background = "#f6f7f8"
        - surface = "#ffffff"
        - text = "#1f2329"
        - muted = "#686e76"
        - line = "#e2e4e7"
        - accent = "#3d5f8a"
        - success = "#2f9e6e"
        - warning = "#d4932a"
        - danger = "#d9534f"
    }
    - dark {
        - background = "#131416"
        - surface = "#1b1d20"
        - text = "#e9eaec"
        - muted = "#9a9fa6"
        - line = "#2e3135"
        - accent = "#6b8cb5"
        - success = "#3fae7a"
        - warning = "#d6a63a"
        - danger = "#e0716c"
    }
    - background = $(mode == "dark" ? dark/background : light/background)
    - surface = $(mode == "dark" ? dark/surface : light/surface)
    - text = $(mode == "dark" ? dark/text : light/text)
    - muted = $(mode == "dark" ? dark/muted : light/muted)
    - line = $(mode == "dark" ? dark/line : light/line)
    - accent = $(mode == "dark" ? dark/accent : light/accent)
    - success = $(mode == "dark" ? dark/success : light/success)
    - warning = $(mode == "dark" ? dark/warning : light/warning)
    - danger = $(mode == "dark" ? dark/danger : light/danger)
    - font = "-apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif"
    - mono = "ui-monospace, SFMono-Regular, Consolas, 'Liberation Mono', Menlo, monospace"
    - radius = "8px"
    - checkbox {
        - style = "box"
    }
    - button {
        - hover = "background"
    }
    - separator {
        - style = "solid"
        - thickness = "1px"
        - length = "full"
    }
}
