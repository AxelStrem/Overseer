// Test file for checkbox labels
tab test_tab (title="Checkbox Label Test") {
    text header = "Testing checkbox labels"
    
    div TestTemplate (hidden=true) {
        string description = ""
        checkbox is_complete(label="Completed") = false
        checkbox is_tested(label="Tested") = false
    }
    
    list TestItems (entry=<TestTemplate>) {
        - {
            - description = "First test item"
            - is_complete = true
            - is_tested = false
        }
        - {
            - description = "Second test item"  
            - is_complete = false
            - is_tested = true
        }
    }
}
