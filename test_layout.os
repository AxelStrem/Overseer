// Test file for layout system functionality
tab main (title="Layout System Test") {
    text header = "Testing Adaptive Layout System"
    
    div TaskBoard (layout=vertical, spacing=16, margin=8) {
        text boardTitle = "Task Board (Vertical with 16px spacing)"
        
        div TaskRow (layout=horizontal, spacing=12, margin-top=16) {
            text rowTitle = "Task Row (Horizontal with 12px spacing)"
            
            div TaskCard (spacing=4, margin=6) {
                text cardTitle = "Card 1 (Default opposite=vertical)"
                string title (margin-bottom=8) = "Important Task"
                int priority (margin-top=4) = 9
                string status = "In Progress"
            }
            
            div TaskCard (layout=inherit, spacing=6, margin=6) {
                text cardTitle = "Card 2 (Explicit inherit=horizontal)"  
                string title (margin-bottom=8) = "Another Task"
                int priority (margin-top=4) = 5
                string status = "Todo"
            }
            
            div TaskCard (layout=opposite, spacing=8, margin=6) {
                text cardTitle = "Card 3 (Explicit opposite=vertical)"
                string title (margin-bottom=8) = "Final Task"
                int priority (margin-top=4) = 2
                string status = "Done"
            }
        }
        
        div DataRow (layout=horizontal, spacing=20, margin-top=24) {
            div Stats (margin-right=16) {
                text statsTitle = "Statistics (Default opposite=vertical)"
                int totalTasks = 15
                int completedTasks = 8
                int inProgress = 4
            }
            
            div Controls (margin-left=16) {
                text controlsTitle = "Controls (Default opposite=vertical)"
                string filterStatus = "All"
                string sortBy = "Priority"
                button refreshBtn = "Refresh Data"
            }
        }
    }
    
    div TestMargins (layout=horizontal, spacing=10, margin=20) {
        text marginTest = "Testing individual margins:"
        
        string field1 (margin-top=5, margin-right=10) = "Top 5px, Right 10px"
        string field2 (margin-bottom=15, margin-left=8) = "Bottom 15px, Left 8px"  
        string field3 (margin=12) = "All margins 12px"
    }
}
