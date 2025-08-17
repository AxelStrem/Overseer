tab test_template_inheritance {
    // Test 1: Remove hidden=true requirement - string template should work
    string StringTemplate = "This is a string template"
    
    // Test 2: Any node type should work as template - int template
    int IntTemplate (font-size=20px) = 42
    
    // Test 3: Verify parent parameter inheritance - div template with styling
    div DivTemplate (background-color=red, font-size=22px, layout=horizontal) {
        string field1 = "default field1" 
        int field2 = 10
        string field3 = "default field3"
    }
    
    text header = "Template Inheritance Tests:"
    
    list string_items (entry=<StringTemplate>) {
        - { }
        - { }
    }
    
    list int_items (entry=<IntTemplate>) {  
        - { }
        - { }
    }
    
    list div_items (entry=<DivTemplate>) {
        - {
            - field1 = "overridden field1"
            - field2 = 99
            // field3 should use template default
        }
        - {
            - field1 = "second item"
            - field2 = 77
            - field3 = "overridden field3"
        }
    }
}
