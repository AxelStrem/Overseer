// Food catalog: the single source of nutritional data for the calorie tracker.
// -
// Every food is identified by a readable string `handle`, which is what a consumed-food
// record in the tracker stores alongside an amount. Records keep nothing else, so a meal
// entry stays two lines and correcting a food's macros here fixes every record at once.
// -
// Values are stored per 100 g - that is what nutrition labels print, and it is the side
// that carries the defaults. The per-portion figures are derived from `portion_weight`,
// so a food only ever states the numbers it actually knows.
// -
// Anything omitted falls back to the template defaults below. They are deliberately
// plausible rather than accurate: a food with no salt figure reads as lightly salted
// instead of as salt-free, which keeps totals honest-ish until the real number is filled in.
// -
// Formatting note: this leading block is deliberately contiguous. A bare "//" line is
// dropped on save, and a blank line between leading comment blocks is hoisted to the top
// of the file. See the ignored tests in src-tauri/tests/comment_trivia.rs.

tab food_catalog (label="Food Catalog", mutable=true) {

    text header (markdown=true) = "# Food Catalog"

    div (hidden=true) {

        div Food (layout="vertical", margin=0) {
            string handle (label="Handle", width=15%) = "unnamed"
            string name (label="Name", width=25%) = ""
            float portion_weight (label="Portion", suffix=" g", precision=0) = 100

            // Per 100 g - the canonical side. Defaults live here.
            div per_100g (label="Per 100 g", layout="horizontal", margin=0) {
                float calories (label="kcal", precision=0) = 100
                float protein (label="Protein", suffix=" g", precision=1) = 5
                float fat (label="Fat", suffix=" g", precision=1) = 5
                float saturated_fat (label="Sat. Fat", suffix=" g", precision=1) = 1
                float trans_fat (label="Trans Fat", suffix=" g", precision=1) = 0
                float carbs (label="Carbs", suffix=" g", precision=1) = 10
                float sugar (label="Sugar", suffix=" g", precision=1) = 2
                float fibre (label="Fibre", suffix=" g", precision=1) = 1
                float salt (label="Salt", suffix=" g", precision=2) = 0.2
            }

            // Per portion - always derived, never stored.
            div per_portion (label="Per portion", layout="horizontal", margin=0) {
                float calories (label="kcal", precision=0) = $(per_100g/calories * portion_weight * 0.01)
                float protein (label="Protein", suffix=" g", precision=1) = $(per_100g/protein * portion_weight * 0.01)
                float fat (label="Fat", suffix=" g", precision=1) = $(per_100g/fat * portion_weight * 0.01)
                float saturated_fat (label="Sat. Fat", suffix=" g", precision=1) = $(per_100g/saturated_fat * portion_weight * 0.01)
                float trans_fat (label="Trans Fat", suffix=" g", precision=1) = $(per_100g/trans_fat * portion_weight * 0.01)
                float carbs (label="Carbs", suffix=" g", precision=1) = $(per_100g/carbs * portion_weight * 0.01)
                float sugar (label="Sugar", suffix=" g", precision=1) = $(per_100g/sugar * portion_weight * 0.01)
                float fibre (label="Fibre", suffix=" g", precision=1) = $(per_100g/fibre * portion_weight * 0.01)
                float salt (label="Salt", suffix=" g", precision=2) = $(per_100g/salt * portion_weight * 0.01)
            }
        }
    }

    // Editable draft for a new food. Fill it in, press Add, and it is appended to the
    // catalog below; the draft keeps its values so a near-identical food is quick to add.
    div NewFood (border-style=solid 1px gray, layout="vertical", margin=8) {
        text new_header (markdown=true) = "### Add a food"
        <Food> draft {
            - handle = "new_food"
            - name = "New Food"
            - portion_weight = 100
        }
        button add (label="Add to catalog", margin=0) {
            on click {
                append (list="/food_catalog/Catalog", template="<draft>")
            }
        }
    }

    list Catalog (entry=<Food>, key="handle", layout="vertical", width=100%) {
        - {
            - handle = "apple"
            - name = "Apple"
            - portion_weight = 180
            div per_100g {
                - calories = 52
                - protein = 0.3
                - fat = 0.2
                - saturated_fat = 0.1
                - carbs = 14
                - sugar = 10.4
                - fibre = 2.4
                - salt = 0
            }
        }
        - {
            - handle = "chicken_wrap"
            - name = "Chicken Wrap"
            - portion_weight = 300
            div per_100g {
                - calories = 137
                - protein = 9
                - fat = 7.7
                - saturated_fat = 1
                - carbs = 6.3
                - sugar = 1.3
                - fibre = 1.7
                - salt = 0.11
            }
        }
        - {
            - handle = "coffee_latte"
            - name = "Coffee Latte"
            - portion_weight = 250
            div per_100g {
                - calories = 80
                - protein = 0
                - fat = 10
                - saturated_fat = 2
                - carbs = 20
                - sugar = 5
                - fibre = 0
                - salt = 0
            }
        }
        - {
            - handle = "pizza_slice"
            - name = "Pizza Slice"
            - portion_weight = 125
            div per_100g {
                - calories = 340
                - protein = 3
                - fat = 12
                - saturated_fat = 4
                - carbs = 62
                - sugar = 3
                - fibre = 3
                - salt = 1
            }
        }
        - {
            - handle = "potato_salad"
            - name = "Potato Salad"
            - portion_weight = 200
            div per_100g {
                - calories = 150
                - protein = 5
                - fat = 5
                - saturated_fat = 1
                - carbs = 5
                - sugar = 1
                - fibre = 10
                - salt = 1
            }
        }
        - {
            - handle = "banana"
            - name = "Banana"
            - portion_weight = 118

            // Per 100 g - the canonical side. Defaults live here.
            div per_100g {
                - calories = 89
                - protein = 1.1
                - fat = 0
                - carbs = 22.9
            }
    }
    }
}
