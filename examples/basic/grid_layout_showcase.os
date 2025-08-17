tab grid_examples {
    text title = "Advanced Grid Layout Examples"
    
    text basic_grid_title = "1. Basic Grid with Fixed Sizes and Overflow Clamping:"
    div basic_grid (layout=horizontal, spacing=0, margin=10) {
        div cell1 (width=100px, height=60px, background-color=#f0f0f0, border-style=solid(1px, black)) {
            text content = "This is a lot of content that should be clamped when it exceeds the fixed 100px width"
        }
        div cell2 (width=100px, height=60px, background-color=#e0e0e0, border-style=solid(1px, black)) {
            text content = "Cell 2"
        }
        div cell3 (width=100px, height=60px, background-color=#d0d0d0, border-style=solid(1px, black)) {
            text content = "Cell 3"
        }
    }
    
    text selective_borders_title = "2. Table-like Layout with Selective Borders:"
    div table_header (layout=horizontal, spacing=0, margin=10) {
        div header1 (width=120px, height=40px, background-color=#333, font-color=white, border-top=solid(2px, black), border-bottom=solid(2px, black), border-left=solid(2px, black)) {
            text content = "Name"
        }
        div header2 (width=80px, height=40px, background-color=#333, font-color=white, border-top=solid(2px, black), border-bottom=solid(2px, black)) {
            text content = "Age"
        }
        div header3 (width=100px, height=40px, background-color=#333, font-color=white, border-top=solid(2px, black), border-bottom=solid(2px, black), border-right=solid(2px, black)) {
            text content = "Status"
        }
    }
    
    div table_row1 (layout=horizontal, spacing=0, margin=0) {
        div cell1 (width=120px, height=35px, background-color=#f9f9f9, border-bottom=solid(1px, #ccc), border-left=solid(2px, black)) {
            text content = "John Doe"
        }
        div cell2 (width=80px, height=35px, background-color=#f9f9f9, border-bottom=solid(1px, #ccc)) {
            text content = "25"
        }
        div cell3 (width=100px, height=35px, background-color=#f9f9f9, border-bottom=solid(1px, #ccc), border-right=solid(2px, black)) {
            text content = "Active"
        }
    }
    
    div table_row2 (layout=horizontal, spacing=0, margin=0) {
        div cell1 (width=120px, height=35px, background-color=#ffffff, border-bottom=solid(1px, #ccc), border-left=solid(2px, black)) {
            text content = "Jane Smith"
        }
        div cell2 (width=80px, height=35px, background-color=#ffffff, border-bottom=solid(1px, #ccc)) {
            text content = "30"
        }
        div cell3 (width=100px, height=35px, background-color=#ffffff, border-bottom=solid(1px, #ccc), border-right=solid(2px, black)) {
            text content = "Inactive"
        }
    }
    
    div table_row3 (layout=horizontal, spacing=0, margin=0) {
        div cell1 (width=120px, height=35px, background-color=#f9f9f9, border-bottom=solid(2px, black), border-left=solid(2px, black)) {
            text content = "Bob Johnson"
        }
        div cell2 (width=80px, height=35px, background-color=#f9f9f9, border-bottom=solid(2px, black)) {
            text content = "35"
        }
        div cell3 (width=100px, height=35px, background-color=#f9f9f9, border-bottom=solid(2px, black), border-right=solid(2px, black)) {
            text content = "Active"
        }
    }
    
    text dashboard_title = "3. Dashboard Grid with Mixed Content:"
    div dashboard (layout=vertical, spacing=10, margin=20) {
        div dashboard_row1 (layout=horizontal, spacing=10) {
            div metric_card1 (width=200px, height=120px, background-color=#e3f2fd, border-style=default, border-left=solid(4px, #2196f3)) {
                text metric_title = "Total Users"
                text metric_value = "1,247"
                text metric_change = "+12% this month"
            }
            div metric_card2 (width=200px, height=120px, background-color=#f3e5f5, border-style=default, border-left=solid(4px, #9c27b0)) {
                text metric_title = "Revenue"
                text metric_value = "$45,230"
                text metric_change = "+8% this month"
            }
            div metric_card3 (width=200px, height=120px, background-color=#e8f5e8, border-style=default, border-left=solid(4px, #4caf50)) {
                text metric_title = "Conversion"
                text metric_value = "3.24%"
                text metric_change = "+0.3% this month"
            }
        }
        
        div dashboard_row2 (layout=horizontal, spacing=10) {
            div chart_area (width=420px, height=200px, background-color=#fafafa, border-style=solid(1px, #ddd)) {
                text chart_title = "Performance Chart"
                text chart_placeholder = "[Chart visualization would go here]"
            }
            div activity_feed (width=200px, height=200px, background-color=#fff, border-style=solid(1px, #ddd)) {
                text feed_title = "Recent Activity"
                text activity1 = "User signed up"
                text activity2 = "Order completed"
                text activity3 = "Payment received"
            }
        }
    }
    
    text mixed_units_title = "4. Mixed Unit Types and Responsive Design:"
    div responsive_grid (layout=horizontal, spacing=5, margin=10) {
        div fixed_sidebar (width=200px, height=300px, background-color=#263238, border-right=solid(2px, #37474f)) {
            text sidebar_title = "Fixed Sidebar"
            text sidebar_content = "200px wide, always consistent"
        }
        div flexible_content (width=60%, height=300px, background-color=#f5f5f5, border-style=solid(1px, #ddd)) {
            text content_title = "Flexible Content"
            text content_description = "60% width, adapts to container size"
        }
        div action_panel (width=150px, height=300px, background-color=#fff3e0, border-left=solid(2px, #ff9800)) {
            text panel_title = "Action Panel"
            text panel_content = "150px fixed width for controls"
        }
    }
}
