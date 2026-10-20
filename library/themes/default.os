context theme {
    - light {
        - background = "#f8fafc"
        - surface = "#ffffff"
        - text = "#1e293b"
        - muted = "#64748b"
        - line = "#e2e8f0"
        - accent = "#2563eb"
        - success = "#10b981"
        - warning = "#f59e0b"
        - danger = "#ef4444"
    }
    - dark {
        - background = "#0f172a"
        - surface = "#1e293b"
        - text = "#f1f5f9"
        - muted = "#94a3b8"
        - line = "#334155"
        - accent = "#3b82f6"
        - success = "#22c55e"
        - warning = "#eab308"
        - danger = "#f87171"
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
