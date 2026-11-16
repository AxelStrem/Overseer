div definition (layout="vertical", mutable=true) {
    div (hidden=true) {
        div Place (layout="horizontal", margin=0, spacing=8, alignment="center") {
            string name (label="", font-weight="bold", width=180px) = ""
            string about (label="") = ""
        }

        div System {
            string tag = ""
            string name = ""
        }

        list Systems (entry=<System>, key="tag") {
            - {
                - tag = "windows"
                - name = "Windows"
            }
            - {
                - tag = "macos"
                - name = "macOS"
            }
            - {
                - tag = "linux"
                - name = "Linux"
            }
        }
    }

    text description (markdown=true, font-size=14px) = "Say what the project is and what it holds to, then where its parts are, how work on it is checked, and what has bitten before."

    text repos_header (markdown=true) = "## Repositories"
    list Repos (entry=<Place>, key="name", layout="vertical", spacing=2) { }

    text places_header (markdown=true) = "## Places"
    list Places (entry=<Place>, key="name", layout="vertical", spacing=2) { }

    tags systems (label="Runs on", vocabulary="/definition/Systems", hover-text="Every system, when none is picked") = ""
    text settings (label="Settings") = ""
}
