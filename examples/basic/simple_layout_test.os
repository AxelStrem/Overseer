// Simple layout test
tab main (title="Simple Layout Test") {
    text title = "Layout Test"
    
    div Container (layout=vertical, spacing=20) {
        text header = "Vertical Container"
        
        div Row (layout=horizontal, spacing=15) {
            div Card1 {
                string name = "Card One"
                int value = 10
            }
            div Card2 {
                string name = "Card Two" 
                int value = 20
            }
            div Card3 {
                string name = "Card Three"
                int value = 30
            }
        }
        
        div Row2 (layout=horizontal, spacing=10) {
            string field1 = "Field A"
            string field2 = "Field B"
            string field3 = "Field C"
        }
    }
}
