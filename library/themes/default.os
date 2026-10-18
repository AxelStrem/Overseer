context theme {
    - background = $(mode == "dark" ? "#0f172a" : "#f8fafc")
    - surface = $(mode == "dark" ? "#1e293b" : "#ffffff")
    - text = $(mode == "dark" ? "#f1f5f9" : "#1e293b")
    - muted = $(mode == "dark" ? "#94a3b8" : "#64748b")
    - line = $(mode == "dark" ? "#334155" : "#e2e8f0")
    - accent = $(mode == "dark" ? "#3b82f6" : "#2563eb")
    - success = $(mode == "dark" ? "#22c55e" : "#10b981")
    - warning = $(mode == "dark" ? "#eab308" : "#f59e0b")
    - danger = $(mode == "dark" ? "#f87171" : "#ef4444")
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
