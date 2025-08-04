tab main (title="Overseer DSL Development") {
    text PhasesHeader = "Overseer DSL Development phases:"
    list Phases (entry=string) {
        - = "Phase 1: Core File Operations"
        - = "Phase 2: Basic Content Display"
        - = "Phase 3: Interactive Editing"
        - = "Phase 4: UI Polish"
    }
    text StepsHeader = "Step list:"
    div Task (hidden=true) {
        string task_description = ""
        checkbox complete = false
        checkbox tested = false
        string notes = ""
    }
    div Step (hidden=true) {
        string name = ""
        string description = ""
        int priority = 0
        list StepTasks (entry=<../Task>)
    }
    list Steps (entry=<../Step>) {
        - {
            string name = "1.1: Test Current File Operations"
            string description = "Verify basic file operations work without JavaScript errors"
            int priority = 1
            list StepTasks (entry=<../Task>) {
                - {
                    string task_description = "Verify New File works without getElementById error"
                    checkbox complete = true
                    checkbox tested = true
                    string notes = "COMPLETED: JavaScript fix for document variable naming conflict working"
                }
                - {
                    string task_description = "Verify Open File works without error"
                    checkbox complete = true
                    checkbox tested = true
                    string notes = "COMPLETED: File dialog and loading process works correctly"
                }
                - {
                    string task_description = "Test basic file saving"
                    checkbox complete = true
                    checkbox tested = false
                    string notes = ""
                }
            }
        }
        - {
            string name = "1.2: Fix File Status Indicators"
            string description = "Add proper UI feedback for file operations"
            int priority = 2
            list StepTasks (entry=<../Task>) {
                - {
                    string task_description = "Add status bar at bottom of app"
                    checkbox complete = false
                    checkbox tested = false
                    string notes = "Show current operation status and file state"
                }
                - {
                    string task_description = "Show file path in title/header"
                    checkbox complete = false
                    checkbox tested = false
                    string notes = "Display currently open file path for user reference"
                }
                - {
                    string task_description = "Add unsaved changes indicator (*)"
                    checkbox complete = false
                    checkbox tested = false
                    string notes = "Visual indicator when file has unsaved modifications"
                }
            }
        }
        - {
            string name = "1.2: Fix File Status Indicators"
            string description = "Add proper UI feedback for file operations"
            int priority = 2
            list StepTasks (entry=<../Task>) {
                - {
                    string task_description = "Add status bar at bottom of app"
                    checkbox complete = false
                    checkbox tested = false
                    string notes = "Show current operation status and file state"
                }
                - {
                    string task_description = "Show file path in title/header"
                    checkbox complete = false
                    checkbox tested = false
                    string notes = "Display currently open file path for user reference"
                }
                - {
                    string task_description = "Add unsaved changes indicator (*)"
                    checkbox complete = false
                    checkbox tested = false
                    string notes = "Visual indicator when file has unsaved modifications"
                }
            }
        }
    }
    div metadata {
        string created = "2025-07-25"
        string lastUpdated = "2025-07-26"
        int totalSteps = 9
        int totalTasks = 27
        int completedSteps = 1
        int completedTasks = 3
        string currentPhase = "Phase 1: Core File Operations"
        string nextStep = "2.1: Basic Content Parsing and Display"
        string estimatedCompletion = "TBD"
        string notes = "Step 1.1 completed - file operations working. Now debugging content parsing and display. This document tracks our incremental development approach with small, testable steps. Each task should take 15-30 minutes and be immediately verifiable."
    }
}
