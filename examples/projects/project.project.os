<projects/project_definition.os/definition> definition {
    - description = "A document editor and the parser under it, released as an alpha and then a beta.\n\n**Checking work.** `cargo test` in `{editor}` for the parser, and the page opened against `{samples}` for anything a person sees.\n\n**What has bitten before.** A block after a value closes it early, so every parser change is run over the samples too."
    - Repos {
        - {
            - name = "editor"
            - about = "The editor and its parser"
            - url = "https://example.com/editor.git"
        }
        - {
            - name = "website"
            - about = "The site the releases are announced on"
        }
    }
    - Places {
        - {
            - name = "samples"
            - about = "Documents the parser is checked against"
        }
    }
    - Tools {
        - {
            - file = "tools/setup.ps1"
            - systems = "windows"
            - setup = true
            - sha256 = "9f2b6c0d4e1a8f73b5c2d6e9a0f1b4c7d8e3a2f5b6c9d0e1f4a7b8c3d2e5f6a9"
        }
        - {
            - file = "tools/setup.sh"
            - systems = "linux"
            - setup = true
            - sha256 = "3c8e1f4a7b2d5e9c0f6a3b8d1e4f7a2c5b9e0d3f6a1c4b7e2d5f8a9c0b3e6d1f"
        }
    }
    - systems = "windows, linux"
    - settings = "{\"effort\": \"medium\"}"
}
