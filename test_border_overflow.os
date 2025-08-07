tab border_test {
    text title = "Selective Border Controls Test"
    
    text section1 = "Individual Border Sides:"
    div border_examples (layout=vertical, spacing=20, margin=20) {
        div top_only (width=150px, height=50px, background-color=#f0f0f0, border-top=solid 3px red) {
            text content = "Top Border Only"
        }
        
        div bottom_only (width=150px, height=50px, background-color=#f0f0f0, border-bottom=solid 3px blue) {
            text content = "Bottom Border Only"
        }
        
        div left_only (width=150px, height=50px, background-color=#f0f0f0, border-left=solid 3px green) {
            text content = "Left Border Only"
        }
        
        div right_only (width=150px, height=50px, background-color=#f0f0f0, border-right=solid 3px orange) {
            text content = "Right Border Only"
        }
        
        div multiple_sides (width=150px, height=50px, background-color=#f0f0f0, border-top=solid 2px black, border-bottom=solid 2px black) {
            text content = "Top + Bottom"
        }
        
        div all_different (width=150px, height=50px, background-color=#f0f0f0, border-top=solid 1px red, border-bottom=solid 2px blue, border-left=solid 3px green, border-right=dashed 2px orange) {
            text content = "All Different"
        }
    }
    
    text section2 = "Content Overflow Test:"
    div overflow_examples (layout=vertical, spacing=20, margin=20) {
        div overflow_width (width=100px, height=60px, background-color=#ffe0e0, border-style=solid 1px red, margin = 0) {
            text content = "This text is much longer than the 100px width container and should be clipped"
        }
        
        div overflow_height (width=200px, height=40px, background-color=#e0f0ff, border-style=solid 1px blue, margin = 0) {
            text line1 = "Line 1 of content"
            text line2 = "Line 2 of content" 
            text line3 = "Line 3 of content"
            text line4 = "Line 4 should be hidden"
        }
        
        div overflow_both (width=80px, height=30px, background-color=#e0ffe0, border-style=solid 1px gree, margin = 0) {
            text content = "Very long text in a very small container that should clip both horizontally and vertically"
        }
    }
}
