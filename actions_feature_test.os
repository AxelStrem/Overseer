div ActionFeatureTest (layout = horizontal) {
    
    int counter = 0
    button increment (label="Increment") {
        on click { inc(path="../counter", by=1) }
    }
}
