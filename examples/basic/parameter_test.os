// Test parameters on field declarations
tab main (title="Parameter Test") {
    text header = "Testing String Parameters"
    
    div Container (layout=horizontal, spacing=15) {
        // Test 1: Simple string without parameters
        string simple = "This should work"
        
        // Test 2: String with parameters  
        string withParams (margin-top=10) = "This might not work"
        
        // Test 3: Try alternative syntax
        div Field {
            string title = "Alternative syntax"
        }
        
        div FieldWithMargin (margin-left=5) {
            string title = "Container has margin"
        }
    }
}
