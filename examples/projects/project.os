<projects/project_template.os/project> project {
    - definition (source="project.project.os/definition", lazy=false)
    - Items {
        - {
            - added = "2026-09-01T09:00:00+04:00"
            - handle = "editor"
            - title = "Rewrite the document editor"
            - labels = "feature"
            - milestones = "beta"
            - points = 3
        }
        - {
            - added = "2026-09-01T09:05:00+04:00"
            - handle = "parser"
            - title = "Parser mishandles nested blocks"
            - parent = "editor"
            - labels = "bug"
            - milestones = "alpha, beta"
            - points = 2
        }
        - {
            - added = "2026-09-01T09:10:00+04:00"
            - handle = "brace"
            - title = "A body after a value closes the block early"
            - parent = "parser"
            - labels = "bug"
            - points = 2
            div Talk {
                list Comments {
                    - {
                        - at = "2026-09-01T09:10:00+04:00"
                        - author = "owner"
                        - body = "only when the value comes first; a body on its own is fine"
                    }
                }
            }
        }
        - {
            - added = "2026-09-02T10:00:00+04:00"
            - handle = "rows"
            - title = "Tighten the row layout"
            - parent = "editor"
            - after = "escape"
            - labels = "ui"
            - points = 4
        }
        - {
            - added = "2026-09-02T10:30:00+04:00"
            - handle = "undo"
            - title = "Undo survives a repaint"
            - stage = "later"
            - parent = "editor"
            - labels = "bug"
            - points = 5
        }
        - {
            - added = "2026-09-03T11:00:00+04:00"
            - handle = "docs"
            - title = "Write the guide"
            - labels = "docs"
            - points = 2
        }
        - {
            - added = "2026-09-04T16:00:00+04:00"
            - handle = "favicon"
            - title = "Pick a favicon"
            - stage = "later"
            - after = "rows"
            - labels = "ui"
            - points = 1
            div Talk {
                list Comments {
                    - {
                        - at = "2026-09-04T16:00:00+04:00"
                        - author = "owner"
                        - body = "stands on its own; nothing is part of it and it is part of nothing"
                    }
                }
            }
        }
    }
    - Labels {
        - {
            - tag = "bug"
            - name = "bug"
            - colour = "#d73a4a"
        }
        - {
            - tag = "feature"
            - name = "feature"
            - colour = "#0e8a16"
        }
        - {
            - tag = "ui"
            - name = "interface"
            - colour = "#1d76db"
        }
        - {
            - tag = "docs"
            - name = "docs"
            - colour = "#7057ff"
        }
    }
    - Milestones {
        - {
            - tag = "alpha"
            - name = "alpha"
            - colour = "#d97706"
        }
        - {
            - tag = "beta"
            - name = "beta"
            - colour = "#0891b2"
        }
    }
    - Features {
        - {
            - added = "2026-09-02T10:00:00+04:00"
            - handle = "UNDO-TREE"
            - standing = "accepted"
            - title = "Undo keeps every branch, not only the last"
            - synergies = "HISTORY-VIEW"
            - x = 24
            - y = 24
            div Detail {
                - description = "An undo after a new edit keeps the edits it replaced, as a branch to go back to."
                - pros = "Nothing typed is ever lost."
                - cons = "Needs a way to show the branches."
            }
        }
        - {
            - added = "2026-09-02T10:05:00+04:00"
            - handle = "HISTORY-VIEW"
            - standing = "wanted"
            - title = "A page that draws the edit history"
            - requires = "UNDO-TREE"
            - synergies = "UNDO-TREE"
            - x = 320
            - y = 40
            div Talk {
                list Comments {
                    - {
                        - at = "2026-09-03T09:00:00+04:00"
                        - author = "owner"
                        - body = "Only worth it once <#UNDO-TREE> is in."
                    }
                }
            }
        }
        - {
            - added = "2026-09-02T10:10:00+04:00"
            - handle = "PLUGINS"
            - title = "Third-party plugins for the editor"
            - incompatible = "SANDBOX"
            - x = 40
            - y = 380
        }
        - {
            - added = "2026-09-02T10:15:00+04:00"
            - handle = "SANDBOX"
            - standing = "discarded"
            - title = "Run every document in a sandbox"
            - incompatible = "PLUGINS"
            - x = 336
            - y = 380
        }
        - {
            - added = "2026-09-02T10:20:00+04:00"
            - handle = "SPELLCHECK"
            - title = "Underline misspelled words as they are typed"
        }
    }
    - History {
        - {
            - finished_at = "2026-09-05T17:20:00+04:00"
            - added = "2026-09-01T09:20:00+04:00"
            - title = "Escape quotes and newlines when saving"
            - handle = "escape"
            - parent = "editor"
            - labels = "bug"
            - points = 5
        }
        - {
            - finished_at = "2026-09-09T09:30:00+04:00"
            - added = "2026-09-03T11:05:00+04:00"
            - title = "Document the filter"
            - handle = "gfilter"
            - parent = "docs"
            - labels = "docs"
            - points = 2
        }
        - {
            - finished_at = "2026-09-09T11:00:00+04:00"
            - added = "2026-09-01T09:15:00+04:00"
            - title = "Round-trip every real document in a test"
            - handle = "roundtrip"
            - parent = "parser"
            - labels = "bug, docs"
            - milestones = "alpha"
            - points = 6
            div Talk {
                list Comments {
                    - {
                        - at = "2026-09-01T09:15:00+04:00"
                        - author = "owner"
                        - body = "every document under examples/, read and written back byte for byte"
                    }
                    - {
                        - at = "2026-09-09T11:00:00+04:00"
                        - author = "claude-opus (medium)"
                        - body = "Built: a test per document. Two failed at first - **trailing blank lines** were dropped - and pass now."
                    }
                }
            }
            div Agents {
                list Work {
                    - {
                        - at = "2026-09-08T18:00:00+04:00"
                        - kind = "triage"
                        - model = "claude-opus (medium)"
                        - moved = "filed > ready"
                        - points = 5
                        - cost = 0.42
                        - read = 310
                        - written = 48
                        - output = 9
                        - turns = 14
                        - minutes = 3.5
                    }
                    - {
                        - at = "2026-09-09T11:00:00+04:00"
                        - kind = "build"
                        - model = "claude-opus (medium)"
                        - moved = "ready > testing"
                        - points = 6
                        - cost = 1.87
                        - read = 1420
                        - written = 96
                        - output = 31
                        - turns = 52
                        - minutes = 14.2
                        - lines = 164
                    }
                }
            }
        }
        - {
            - finished_at = "2026-09-10T15:10:00+04:00"
            - added = "2026-09-03T11:10:00+04:00"
            - title = "Document how a task becomes a larger one"
            - handle = "gparent"
            - parent = "docs"
            - labels = "docs"
            - points = 1
        }
    }
}
