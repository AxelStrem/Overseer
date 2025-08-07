tab test_radius {
    text title = "Border Radius Test"
    
    div rounded_default {
        text content = "Default rounded corners"
    }
    
    div straight_corners (border-radius=0px, border-style=solid 2px red) {
        text content = "Should have straight corners"
    }
    
    div minimal_radius (border-radius=1px, border-style=solid 2px green) {
        text content = "Should have barely visible corners"
    }
    
    div extra_rounded (border-radius=20px, border-style=solid 2px blue) {
        text content = "Should have very round corners"
    }
}
