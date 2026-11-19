div definition (layout="vertical", mutable=true) {
    div (hidden=true) {
        div Place (layout="horizontal", margin=0, spacing=8, alignment="center") {
            string name (label="", font-weight="bold", width=180px) = ""
            string about (label="") = ""
        }

        <Place> Repo {
            string url (label="", font-size=11px, hover-text="The address git clone takes, with no credentials in it") = ""
            string branch (label="", font-size=11px, width=100px, hover-text="The branch to clone; empty for the default") = ""
        }

        div System {
            string tag = ""
            string name = ""
        }

        div Tool (layout="horizontal", margin=0, spacing=8, alignment="center") {
            string file (label="", font-weight="bold", width=180px) = ""
            tags systems (label="", vocabulary="/definition/Systems", hover-text="Every system, when none is picked") = ""
            string sha256 (label="", font-size=11px) = ""
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
    list Repos (entry=<Repo>, key="name", layout="vertical", spacing=2) { }

    text places_header (markdown=true) = "## Places"
    list Places (entry=<Place>, key="name", layout="vertical", spacing=2) { }

    text tools_header (markdown=true) = "## Tools"
    list Tools (entry=<Tool>, key="file", layout="vertical", spacing=2) { }

    tags systems (label="Runs on", vocabulary="/definition/Systems", hover-text="Every system, when none is picked") = ""
    text settings (label="Settings") = ""
}
