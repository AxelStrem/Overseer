div (hidden=true) {
    div Part (layout="horizontal") {
        string name = ""
        int own (label="Own") = 1
        int total (label="Total") = $(own + Parts.sum(|x| x/total))
        list Parts (entry=<Part>, key="name") { }
    }

    div Task (layout="horizontal") {
        string handle = ""
        string title = ""
        string parent = ""
    }
}

tab outline (label="Outline", mutable=true) {
    list Parts (entry=<Part>, key="name", tree="Parts") {
        - {
            - name = "Car"
            list Parts {
                - {
                    - name = "Engine"
                    - own = 4
                    list Parts {
                        - {
                            - name = "Pistons"
                            - own = 6
                        }
                        - {
                            - name = "Crankshaft"
                        }
                    }
                }
                - {
                    - name = "Wheels"
                    - own = 4
                }
            }
        }
        - {
            - name = "Bicycle"
            list Parts {
                - {
                    - name = "Frame"
                }
            }
        }
    }
}

tab tracker (label="By parent", mutable=true) {
    textbox find (placeholder="find", narrows="/tracker/Tasks", by="title") = ""

    list Tasks (entry=<Task>, key="handle", tree="parent") {
        - {
            - handle = "site"
            - title = "Build the site"
        }
        - {
            - handle = "pages"
            - title = "Write the pages"
            - parent = "site"
        }
        - {
            - handle = "about"
            - title = "About page"
            - parent = "pages"
        }
        - {
            - handle = "deploy"
            - title = "Deploy it"
            - parent = "site"
        }
        - {
            - handle = "home"
            - title = "Home page"
            - parent = "pages"
        }
        - {
            - handle = "launch"
            - title = "Launch party"
        }
    }
}
