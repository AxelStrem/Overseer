tab test_template_dependency {
    // First template (no dependencies)
    div BaseTemplate (background-color=blue, font-size=16px) {
        string title = "Base Title"
        int priority = 1
    }
    
    // Second template that extends the first (depends on BaseTemplate)
    list ExtendedTemplate (entry=<BaseTemplate>) {
        - {
            - title = "Extended Title" 
            - priority = 2
        }
    }
    
    // Third template that uses the extended template
    list FinalTemplate (entry=<ExtendedTemplate>) {
        - {
            // Should inherit BaseTemplate parameters through ExtendedTemplate
        }
    }
    
    text header = "Multi-level template test"
    list items (entry=<FinalTemplate>) {
        - {
            // This should work after multi-pass resolution
        }
    }
}
