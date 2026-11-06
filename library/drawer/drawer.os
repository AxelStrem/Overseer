div (hidden=true) {
    div Place (layout="horizontal") {
        string name = ""
        string path (font-size=12px) = ""
        list Places (entry=<Place>, key="name") { }
    }
}

tab drawer (label="Documents", mutable=true) {
    list Places (entry=<Place>, key="name", tree="Places") { }
}
