<projects/project_definition.os/definition> definition {
    - description = "A document editor and the parser under it, released as an alpha and then a beta.\n\n**Checking work.** `cargo test` in `{editor}` for the parser, and the page opened against `{samples}` for anything a person sees.\n\n**What has bitten before.** A block after a value closes it early, so every parser change is run over the samples too."
    - Repos {
        - {
            - name = "editor"
            - about = "The editor and its parser"
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
    - systems = "windows, linux"
    - settings = "{\"effort\": \"medium\"}"
}
