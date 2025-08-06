text Title = "Grid Layout Test - Step 2.4 Implementation"

text Section1 = "Width and Height Parameters"

div FixedSizeContainer (width=300px, height=200px, background-color=lightblue) {
    string content = "Fixed size container (300px x 200px)"
}

div PercentageContainer (width=50%, height=150px, background-color=lightgreen) {
    string content = "Percentage width container (50% x 150px)"
}

div FitContentContainer (width=fit-content, height=auto, background-color=lightyellow, border-style=solid 2px black) {
    string content = "Fit-content width with auto height"
}

text Section2 = "Border Style Parameters"

div NoBorderContainer (border-style=none, background-color=lightcoral, width=200px, height=100px) {
    string content = "No border"
}

div DefaultBorderContainer (border-style=default, background-color=lightpink, width=200px, height=100px) {
    string content = "Default border"
}

div SolidBorderContainer (border-style=solid 1px red, background-color=white, width=200px, height=100px) {
    string content = "Solid 1px red border"
}

div DashedBorderContainer (border-style=dashed 3px blue, background-color=white, width=200px, height=100px) {
    string content = "Dashed 3px blue border"
}

div DottedBorderContainer (border-style=dotted 2px green, background-color=white, width=200px, height=100px) {
    string content = "Dotted 2px green border"
}

text Section3 = "Combined Properties Test"

div CombinedContainer (width=400px, height=150px, border-style=solid 3px purple, background-color=lavender, font-color=darkpurple, font-size=18px) {
    string title = "Combined test: 400px x 150px with solid purple border"
    string description = "Multiple text elements in one container"
}

text Section4 = "CSS Units Test"

div EmUnitsContainer (width=20em, height=10em, border-style=dashed 0.2em orange, background-color=wheat) {
    string content = "Em units: 20em x 10em with 0.2em border"
}

div ViewportContainer (width=30vw, height=20vh, border-style=dotted 5px teal, background-color=lightcyan) {
    string content = "Viewport units: 30vw x 20vh"
}
