div Container (background-color=#e6f3ff, font-color=#2563eb, font-size=18px) {
    string title = "Styling System Test"
    
    div Section (background-color=rgb(0.9, 0.9, 0.9), font-size=1.2em) {
        string header = "Section Header"
        string content = "This text should inherit the blue color but have a larger font size"
    }
    
    div Warning (background-color=orange, font-color=white, font-size=14px) {
        string message = "This is a warning with custom styling"
    }
    
    string footer = "Footer text - inherits container styling"
}
