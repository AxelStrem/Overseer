// Calorie tracker, handle-based records. Prototype alongside weight_tracker_new.os.
// -
// A consumed-food record stores only what is specific to eating it: which food, and how
// much. Everything else is looked up in foods.os through the FOODS mount, so a record is
// two lines on disk and correcting a food's macros fixes every record that ever used it.
// -
// Amount can be entered either way round. State `portions` and grams are derived; state
// `grams` and portions are derived. Both are null in the template so whichever one a record
// states wins - which also means a record must state one of them. Each Add button writes
// only its own amount, so an entry made through the UI always has exactly one.
// -
// Editing an existing record sets whichever field you type into, and does not clear the
// other. Nothing stops a record ending up with both set; the assumption is that you enter
// one. Clearing the counterpart automatically would need an `on change` handler on the
// field, which the parser does not support - a body after a field value is read as a
// sibling node and ends the enclosing block early.
// -
// Formatting note: this leading block is deliberately contiguous, because a blank line
// between leading comment blocks is still hoisted to the top of the file on save. Bare "//"
// separator lines are fine now and used below. See src-tauri/tests/comment_trivia.rs.

tab tracker_v2 (label="Calories", mutable=true) {

    // The food catalog, mounted read-only and never shown. Preloaded so handle lookups
    // resolve as soon as the document opens rather than after a manual Load.
    mount FOODS (hidden=true, lazy=false, mutable=false, source="foods.os/food_catalog/Catalog") { }
div (hidden=true) {

        div MealRecord (layout="vertical", margin=0) {
            string food (label="Food", width=20%) = "apple"

            // The catalogued weight of one portion of this food, used to convert between
            // the two amount styles. Not shown - it belongs to the food, not to the meal.
            float portion_weight (hidden=true) = $(FOODS/Catalog.filter(|x| x/handle == ../food)/portion_weight)

            // Whichever of these a record states, the other is derived. Both are null here
            // so neither shadows the other; a record must state one of them.
            float portions (label="Portions", precision=2, fallback=$(grams / portion_weight)) = null
            float grams (label="Grams", suffix=" g", precision=0, fallback=$(portions * portion_weight)) = null

            string name (label="Name", width=25%) = $(FOODS/Catalog.filter(|x| x/handle == ../food)/name)

            div macros (layout="horizontal", margin=0) {
                float calories (label="kcal", precision=0) = $(grams * FOODS/Catalog.filter(|x| x/handle == ../food)/per_100g/calories * 0.01)
                float protein (label="Protein", suffix=" g", precision=1) = $(grams * FOODS/Catalog.filter(|x| x/handle == ../food)/per_100g/protein * 0.01)
                float fat (label="Fat", suffix=" g", precision=1) = $(grams * FOODS/Catalog.filter(|x| x/handle == ../food)/per_100g/fat * 0.01)
                float saturated_fat (label="Sat. Fat", suffix=" g", precision=1) = $(grams * FOODS/Catalog.filter(|x| x/handle == ../food)/per_100g/saturated_fat * 0.01)
                float trans_fat (label="Trans Fat", suffix=" g", precision=1) = $(grams * FOODS/Catalog.filter(|x| x/handle == ../food)/per_100g/trans_fat * 0.01)
                float carbs (label="Carbs", suffix=" g", precision=1) = $(grams * FOODS/Catalog.filter(|x| x/handle == ../food)/per_100g/carbs * 0.01)
                float sugar (label="Sugar", suffix=" g", precision=1) = $(grams * FOODS/Catalog.filter(|x| x/handle == ../food)/per_100g/sugar * 0.01)
                float fibre (label="Fibre", suffix=" g", precision=1) = $(grams * FOODS/Catalog.filter(|x| x/handle == ../food)/per_100g/fibre * 0.01)
                float salt (label="Salt", suffix=" g", precision=2) = $(grams * FOODS/Catalog.filter(|x| x/handle == ../food)/per_100g/salt * 0.01)
            }
        }

        div DayRecord (layout="vertical") {
            timestamp date (precision="day") = $(today())

            div totals (layout="horizontal", margin=0) {
                float calories (label="Total kcal", precision=0) = $(intake.map(|x| x/macros/calories).sum())
                float protein (label="Protein", suffix=" g", precision=1) = $(intake.map(|x| x/macros/protein).sum())
                float fat (label="Fat", suffix=" g", precision=1) = $(intake.map(|x| x/macros/fat).sum())
                float saturated_fat (label="Sat. Fat", suffix=" g", precision=1) = $(intake.map(|x| x/macros/saturated_fat).sum())
                float carbs (label="Carbs", suffix=" g", precision=1) = $(intake.map(|x| x/macros/carbs).sum())
                float sugar (label="Sugar", suffix=" g", precision=1) = $(intake.map(|x| x/macros/sugar).sum())
                float fibre (label="Fibre", suffix=" g", precision=1) = $(intake.map(|x| x/macros/fibre).sum())
                float salt (label="Salt", suffix=" g", precision=2) = $(intake.map(|x| x/macros/salt).sum())
            }

            list intake (entry=<MealRecord>, layout="vertical")

            // These buttons live inside the day, so the intake path above targets whichever
            // day they are rendered for, including the one shown through the selected-day
            // link. An absolute or keyed path cannot express the day currently on screen:
            // action targets do not understand key selectors, and a link is resolved by the
            // renderer rather than by the backend that runs the append.
            //
            // There are two of them because each writes only its own amount, leaving the
            // other unset so it derives. A single button would have to decide which of the
            // two draft values was meant, and there is no way to ask whether a field is set.
            button add_by_portions (label="+ Add by portions", margin=0) {
                on click {
                    append (list="../intake") {
                        - food = $(/tracker_v2/NewEntry/draft_food)
                        - portions = $(/tracker_v2/NewEntry/draft_portions)
                    }
                }
            }
            button add_by_grams (label="+ Add by weight", margin=0) {
                on click {
                    append (list="../intake") {
                        - food = $(/tracker_v2/NewEntry/draft_food)
                        - grams = $(/tracker_v2/NewEntry/draft_grams)
                    }
                }
            }
        }
    }

    // Draft for a new entry. Fill in the food and whichever amount you know, then press one
    // of the Add buttons on the day below. Only the amount belonging to the button you press
    // is stored; the other is derived from the food's portion weight.
    div NewEntry (border-style=solid 1px gray, layout="vertical", margin=8) {
        text new_entry_header (markdown=true) = "### New entry"
        string draft_food (label="Food handle") = "apple"
        float draft_portions (label="Portions", precision=2) = 1
        float draft_grams (label="Weight", suffix=" g", precision=0) = 100
    }

    div Selected (layout="vertical") {
        button Prev (label="< Prev Day") {
            on click {
                set (path="/tracker_v2/Selected/selected_date") = $(date_add_days(../selected_date, -1))
            }
        }
        timestamp selected_date (precision="day", mutable="guarded") = "2026-08-05"
        button Next (label="> Next Day") {
            on click {
                set (path="/tracker_v2/Selected/selected_date") = $(date_add_days(../selected_date, 1))
            }
        }

        div SelectedDay (mutable=true, link="/tracker_v2/History[key=$(../selected_date)]", phantom-materialize="prepend-on-edit") {
            list intake (hidden=false)
        }
    }

    list History (entry=<DayRecord>, key="date", keyPrecision="day") {
        - {
            - date = "2026-08-07"

            list intake {
                - {
                    - portions = 1
                }
            }
        }
        - {
            - date = "2026-08-06"
            list intake {
                - {
                    - portions = 1
                }
                - {
                    - food = "apple"
                    - portions = 1
                }
            }
        }
        - {
            - date = "2026-08-05"
            list intake {
                - {
                    - portions = 1
                }
            }
        }
        - {
            - date = "2026-08-04"
            list intake {
                - {
                    - food = "chicken_wrap"
                    - portions = 1
                }
                - {
                    - food = "coffee_latte"
                    - grams = 250
                }
                - {
                    - food = "apple"
                    - portions = 1
                }
                - {
                    - food = "apple"
                    - portions = 1
                }
                - {
                    - food = "apple"
                    - portions = 1
                }
                - {
                    - food = "apple"
                    - portions = 1
                }
                - {
                    - food = "apple"
                    - portions = 1
                }
            }
        }
        - {
            - date = "2026-08-03"
            list intake {
                - {
                    - food = "pizza_slice"
                    - portions = 3
                }
                - {
                    - food = "potato_salad"
                    - grams = 150
                }
            }
        }
    }
}
