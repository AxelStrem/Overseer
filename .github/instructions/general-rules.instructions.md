---
applyTo: '**'
---
Use proper PowerShell syntax for terminal commands; Don't use && 

Git commands (especially "git commit") complete immediately and silently in PowerShell. DO NOT wait for output or use get_terminal_output after git commands - proceed immediately to the next action. If you find yourself waiting after a git command, you are likely stuck and should move on.

Always consult product_description.md and technical_architecture.md for project-specific guidelines and architecture details.

