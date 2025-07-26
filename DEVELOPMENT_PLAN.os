// Overseer Development Plan - Incremental Progress Tracking
// Created: 2025-07-25
// Purpose: Track development progress with small, testable steps

tab (title="Overseer DSL Development") {

    text PhasesHeader = "Overseer DSL Development phases:"
    list Phases (entry=string) {
        - "Phase 1: Core File Operations"
        - "Phase 2: Basic Content Display"
        - "Phase 3: Interactive Editing"
        - "Phase 4: UI Polish"
    }
    
    text StepsHeader = "Step list:"

    div Task (hidden=true){
        string task_description = ""
        checkbox complete = false
        checkbox tested = false
        string notes = ""
    }

    div Step (hidden=true) {
        string name = ""
        string description = ""
        int priority = 0        
        list StepTasks (entry=../Task) {
        }

    }

    list Steps (entry=../Step) {

        - {
            - name = "1.1: Test Current File Operations"
            - description = "Verify basic file operations work without JavaScript errors"
            - priority = 1
            - StepTasks {
                - {
                    - task_description = "Verify New File works without getElementById error"
                    - complete = false
                    - tested = false
                    - notes = "Test the JavaScript fix for document variable naming conflict"
                }
                - {
                    - task_description = "Verify Open File works without error"
                    - complete = false
                    - tested = false
                    - notes = "Ensure file dialog and loading process works correctly"
                }
                - {
                    - task_description = "Test basic file saving"
                    - complete = false
                    - tested = false
                    - notes = "Verify save functionality preserves file content"
                }
            }
        }

        - {
            - name = "1.2: Fix File Status Indicators"
            - description = "Add proper UI feedback for file operations"
            - priority = 2
            - StepTasks {
                - {
                    - task_description = "Add status bar at bottom of app"
                    - complete = false
                    - tested = false
                    - notes = "Show current operation status and file state"
                }
                - {
                    - task_description = "Show file path in title/header"
                    - complete = false
                    - tested = false
                    - notes = "Display currently open file path for user reference"
                }
                - {
                    - task_description = "Add unsaved changes indicator (*)"
                    - complete = false
                    - tested = false
                    - notes = "Visual indicator when file has unsaved modifications"
                }
            }
        }

        - {
            - name = "1.3: Integrate Improved File Operations"
            - description = "Replace current file ops with enhanced file_ops_new.rs"
            - priority = 3
            - StepTasks {
                - {
                    - task_description = "Replace current file operations with file_ops_new.rs functions"
                    - complete = false
                    - tested = false
                    - notes = "Use the enhanced async file operations we created"
                }
                - {
                    - task_description = "Add proper error handling for file operations"
                    - complete = false
                    - tested = false
                    - notes = "Comprehensive error messages and recovery options"
                }
                - {
                    - task_description = "Test backup creation functionality"
                    - complete = false
                    - tested = false
                    - notes = "Verify automatic backup creation before saves"
                }
            }
        }
    }
    
    div metadata {
        string created = "2025-07-25"
        string lastUpdated = "2025-07-25"
        int totalSteps = 9
        int totalTasks = 27
        int completedSteps = 0
        int completedTasks = 0
        string currentPhase = "Phase 1: Core File Operations"
        string nextStep = "1.1: Test Current File Operations"
        string estimatedCompletion = "TBD"
        string notes = "This document tracks our incremental development approach with small, testable steps. Each task should take 15-30 minutes and be immediately verifiable."
    }
}
