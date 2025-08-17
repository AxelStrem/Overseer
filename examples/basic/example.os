tab personal_dashboard {
    div income_tracking(layout=vertical) {
        string salary = "5000"
        string bonus = "1000"
        int total_income = $(salary + bonus)
        date last_updated = $(today())
    }
    
    list monthly_expenses(layout=grid) {
        div rent {
            string category = "Housing"
            float amount = 1200.50
            date due_date = "2024-01-01"
        }
        
        div groceries {
            string category = "Food"
            float amount = 400.00
            bool paid = true
        }
        
        div utilities {
            string category = "Bills"
            float amount = 150.75
            checkbox autopay = false
        }
    }
    
    div summary(background = #f0f8ff, border=2px solid #0066cc) {
        text description = "This is my **personal finance** dashboard.
It helps me track income and expenses."
        
        float total_expenses = $(sum(monthly_expenses.amount))
        float remaining = $(total_income - total_expenses)
        
        button refresh_data = "Update All"
    }
    
    chart expense_breakdown(type=pie) {
        data = monthly_expenses
        x_field = category
        y_field = amount
    }
}

tab projects {
    list active_projects(layout=horizontal) {
        div overseer_project {
            string name = "Overseer Development"
            string status = "In Progress"
            int progress = 75
            date deadline = "2024-02-15"
            
            list tasks {
                div parser {
                    bool completed = true
                    string description = "Implement DSL parser"
                }
                
                div ui {
                    bool completed = false
                    string description = "Create user interface"
                }
                
                div testing {
                    bool completed = false
                    string description = "Write comprehensive tests"
                }
            }
        }
        
        div website_redesign {
            string name = "Company Website"
            string status = "Planning"
            int progress = 20
            date deadline = "2024-03-01"
        }
    }
    
    div project_stats {
        int total_projects = $(count(active_projects))
        int completed_tasks = $(count(active_projects.tasks[completed=true]))
        float avg_progress = $(avg(active_projects.progress))
    }
}
