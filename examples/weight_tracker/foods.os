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


        // One tag that a food can carry: the handle it is stored as, what to call it, and what
        // colour to draw it.
        div Label (layout="horizontal", margin=0, spacing=6, alignment="center") {
            string tag (label="", width=25%) = ""
            string name (label="", width=45%) = ""
            string colour (label="", width=30%) = "#6b7280"
        }

        div Food (layout="vertical", margin=0) {
            string handle (label="Handle", width=15%) = "unnamed"
            string name (label="Name", width=25%) = ""
            float portion_weight (label="Portion", suffix=" g", precision=0) = 100

            // What this food is, as chips. Empty is a perfectly good answer - it means nobody
            // has decided yet, not that the food is none of these.
            tags labels (label="Tags", vocabulary="food_catalog/Labels", width=40%) = ""

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
                float vitamin_d (label="Vit. D", suffix=" µg", precision=1) = 0.1
                float calcium (label="Calcium", suffix=" mg", precision=0) = 25
                float iron (label="Iron", suffix=" mg", precision=1) = 0.8
                float potassium (label="Potassium", suffix=" mg", precision=0) = 180

                // Neither is nutrition exactly, and both are worth a column anyway: they are
                // the two things in food that change how a day goes rather than what it was
                // worth. Zero by default because zero is the truth for nearly everything -
                // only coffee, tea, a few soft drinks and anything alcoholic say otherwise.
                float caffeine (label="Caffeine", suffix=" mg", precision=0) = 0
                float alcohol (label="Alcohol", suffix=" g", precision=1) = 0
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
                float vitamin_d (label="Vit. D", suffix=" µg", precision=1) = $(per_100g/vitamin_d * portion_weight * 0.01)
                float calcium (label="Calcium", suffix=" mg", precision=0) = $(per_100g/calcium * portion_weight * 0.01)
                float iron (label="Iron", suffix=" mg", precision=1) = $(per_100g/iron * portion_weight * 0.01)
                float potassium (label="Potassium", suffix=" mg", precision=0) = $(per_100g/potassium * portion_weight * 0.01)
                float caffeine (label="Caffeine", suffix=" mg", precision=0) = $(per_100g/caffeine * portion_weight * 0.01)
                float alcohol (label="Alcohol", suffix=" g", precision=1) = $(per_100g/alcohol * portion_weight * 0.01)
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


    text labels_header (markdown=true) = "## Tags"

    // What a food can be marked as.
    //
    // Kept here rather than in the tracker because the tags belong to the food: a record of
    // eating something stores a handle and an amount, and everything else about it is looked up.
    // The tracker mounts this list to draw the chips, the same way it mounts the catalogue for
    // the macros.
    //
    // `vegan` and `vegetarian` are both put on a vegan food rather than one implying the other.
    // Nothing here knows that vegan is narrower, so a search for vegetarian food would otherwise
    // miss every vegan one - and the reader would have to remember the implication.
    //
    // `alcohol` and `caffeine` are the odd pair: `per_100g` already carries both as numbers, so
    // these say nothing new. They are here to be *read* - a chip on a row, a filter by eye - and
    // they were set from those numbers rather than typed, so the two agree. If one is ever
    // changed without the other they will not, and the number is the one to believe.
    list Labels (entry=<Label>, key="tag", layout="vertical", spacing=1) {
        - {
            - tag = "vegetarian"
            - name = "vegetarian"
            - colour = "#4caf50"
        }
        - {
            - tag = "vegan"
            - name = "vegan"
            - colour = "#0e8a16"
        }
        - {
            - tag = "meat"
            - name = "meat"
            - colour = "#b71c1c"
        }
        - {
            - tag = "fish"
            - name = "fish"
            - colour = "#0277bd"
        }
        - {
            - tag = "dairy"
            - name = "dairy"
            - colour = "#f9a825"
        }
        - {
            - tag = "alcohol"
            - name = "alcohol"
            - colour = "#6a1b9a"
        }
        - {
            - tag = "caffeine"
            - name = "caffeine"
            - colour = "#5d4037"
        }
    }

    list Catalog (entry=<Food>, key="handle", layout="vertical", width=100%) {
        - {
            - handle = "apple"
            - name = "Apple"
            - portion_weight = 180
            - labels = "vegetarian, vegan"
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
            - labels = "meat"
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
            - labels = "vegetarian, dairy, caffeine"
            div per_100g {
                - calories = 80
                - protein = 0
                - fat = 10
                - saturated_fat = 2
                - carbs = 20
                - sugar = 5
                - fibre = 0
                - salt = 0
                - caffeine = 33
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
            - labels = "vegetarian, vegan"

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
