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

    // What the tags mean, for drawing them. A separate mount because FOODS brings in the
    // catalogue and this is a different list in the same file; a mount names one thing.
    //
    // Only the renderer reads it, to turn a handle like "vegan" into a coloured chip saying so.
    // A food carrying a tag this does not list still shows it, marked as unlisted - which is the
    // honest thing, since the food really does carry it.
    mount VOCAB (hidden=true, lazy=false, mutable=false, source="foods.os/food_catalog/Labels") { }
    div (hidden=true) {


        // Nutri-Score: the letter on the front of a European packet, worked out from the
        // figures per 100 g. One block, fed by whoever instantiates it - a meal takes its
        // food's figures, a day takes its own totals spread over everything eaten - so the
        // rules live in exactly one place and the two can never drift apart.
        //
        // Points run the wrong way round on purpose: the nutrients to limit earn points, the
        // ones worth having take them away, and a low total is a good food. Thresholds are
        // the published ones for general foods.
        //
        // Fruit, vegetables and nuts earn points in the real scheme and nothing here knows
        // what fraction of a food they are, so that term is left out. It costs whole produce
        // roughly one grade - an apple lands on B where the packet would say A - and affects
        // nothing else.
        div NutriScore (layout="horizontal", margin=0, border-style=none) {
            // Per 100 g of whatever is being graded. Every one of these is overridden.
            float kcal (hidden=true) = 0
            float sugar (hidden=true) = 0
            float sat_fat (hidden=true) = 0
            float salt (hidden=true) = 0
            float fibre (hidden=true) = 0
            float protein (hidden=true) = 0

            // The scheme works in kilojoules and in milligrams of sodium; the catalogue keeps
            // calories and grams of salt. Salt is sodium x 2.5, so sodium mg is salt g x 400.
            float kj (hidden=true) = $(kcal * 4.184)
            float sodium_mg (hidden=true) = $(salt * 400)

            int p_energy (hidden=true) = $(kj > 3350 ? 10 : (kj > 3015 ? 9 : (kj > 2680 ? 8 : (kj > 2345 ? 7 : (kj > 2010 ? 6 : (kj > 1675 ? 5 : (kj > 1340 ? 4 : (kj > 1005 ? 3 : (kj > 670 ? 2 : (kj > 335 ? 1 : (0)))))))))))
            int p_sugar (hidden=true) = $(sugar > 45 ? 10 : (sugar > 40 ? 9 : (sugar > 36 ? 8 : (sugar > 31 ? 7 : (sugar > 27 ? 6 : (sugar > 22.5 ? 5 : (sugar > 18 ? 4 : (sugar > 13.5 ? 3 : (sugar > 9 ? 2 : (sugar > 4.5 ? 1 : (0)))))))))))
            int p_sat_fat (hidden=true) = $(sat_fat > 10 ? 10 : (sat_fat > 9 ? 9 : (sat_fat > 8 ? 8 : (sat_fat > 7 ? 7 : (sat_fat > 6 ? 6 : (sat_fat > 5 ? 5 : (sat_fat > 4 ? 4 : (sat_fat > 3 ? 3 : (sat_fat > 2 ? 2 : (sat_fat > 1 ? 1 : (0)))))))))))
            int p_sodium (hidden=true) = $(sodium_mg > 900 ? 10 : (sodium_mg > 810 ? 9 : (sodium_mg > 720 ? 8 : (sodium_mg > 630 ? 7 : (sodium_mg > 540 ? 6 : (sodium_mg > 450 ? 5 : (sodium_mg > 360 ? 4 : (sodium_mg > 270 ? 3 : (sodium_mg > 180 ? 2 : (sodium_mg > 90 ? 1 : (0)))))))))))
            int p_fibre (hidden=true) = $(fibre > 4.7 ? 5 : (fibre > 3.7 ? 4 : (fibre > 2.8 ? 3 : (fibre > 1.9 ? 2 : (fibre > 0.9 ? 1 : (0))))))
            int p_protein (hidden=true) = $(protein > 8.0 ? 5 : (protein > 6.4 ? 4 : (protein > 4.8 ? 3 : (protein > 3.2 ? 2 : (protein > 1.6 ? 1 : (0))))))

            int bad (hidden=true) = $(p_energy + p_sugar + p_sat_fat + p_sodium)

            // Protein stops counting once a food is already heavily penalised, so that cured
            // meat and hard cheese cannot climb back up on protein alone.
            //
            // The sum is written out again rather than read from `bad`, which is the same
            // expression one field away. Reading it gave a day the wrong grade: `bad` settled
            // on the right number while this kept the half-finished one it saw first, and the
            // two disagreed in the finished document. Meals never showed it - their figures
            // come straight from the catalogue and settle in one go, while a day's come
            // through its totals and take longer to arrive.
            int score (hidden=true) = $((p_energy + p_sugar + p_sat_fat + p_sodium) >= 11 ? (p_energy + p_sugar + p_sat_fat + p_sodium) - p_fibre : (p_energy + p_sugar + p_sat_fat + p_sodium) - p_fibre - p_protein)

            // Written out rather than reading `score`, for the same reason `score`
            // does not read `bad`: a field one step further along the chain can be
            // left holding the value its input had before it settled, and the
            // document then shows a letter that its own numbers contradict. Only the
            // point fields are read here, and those settle with the figures they
            // come from.
            string grade (font-size=44px, width=100%, font-color=$(((p_energy + p_sugar + p_sat_fat + p_sodium) >= 11 ? (p_energy + p_sugar + p_sat_fat + p_sodium) - p_fibre : (p_energy + p_sugar + p_sat_fat + p_sodium) - p_fibre - p_protein) <= -1 ? "#038141" : (((p_energy + p_sugar + p_sat_fat + p_sodium) >= 11 ? (p_energy + p_sugar + p_sat_fat + p_sodium) - p_fibre : (p_energy + p_sugar + p_sat_fat + p_sodium) - p_fibre - p_protein) <= 2 ? "#85bb2f" : (((p_energy + p_sugar + p_sat_fat + p_sodium) >= 11 ? (p_energy + p_sugar + p_sat_fat + p_sodium) - p_fibre : (p_energy + p_sugar + p_sat_fat + p_sodium) - p_fibre - p_protein) <= 10 ? "#fecb02" : (((p_energy + p_sugar + p_sat_fat + p_sodium) >= 11 ? (p_energy + p_sugar + p_sat_fat + p_sodium) - p_fibre : (p_energy + p_sugar + p_sat_fat + p_sodium) - p_fibre - p_protein) <= 18 ? "#ee8100" : ("#e63e11"))))), width=10%) = $(((p_energy + p_sugar + p_sat_fat + p_sodium) >= 11 ? (p_energy + p_sugar + p_sat_fat + p_sodium) - p_fibre : (p_energy + p_sugar + p_sat_fat + p_sodium) - p_fibre - p_protein) <= -1 ? "A" : (((p_energy + p_sugar + p_sat_fat + p_sodium) >= 11 ? (p_energy + p_sugar + p_sat_fat + p_sodium) - p_fibre : (p_energy + p_sugar + p_sat_fat + p_sodium) - p_fibre - p_protein) <= 2 ? "B" : (((p_energy + p_sugar + p_sat_fat + p_sodium) >= 11 ? (p_energy + p_sugar + p_sat_fat + p_sodium) - p_fibre : (p_energy + p_sugar + p_sat_fat + p_sodium) - p_fibre - p_protein) <= 10 ? "C" : (((p_energy + p_sugar + p_sat_fat + p_sodium) >= 11 ? (p_energy + p_sugar + p_sat_fat + p_sodium) - p_fibre : (p_energy + p_sugar + p_sat_fat + p_sodium) - p_fibre - p_protein) <= 18 ? "D" : ("E")))))
        }
        div MealRecord (layout="vertical", margin=0, background-color="#16203a", border-radius=0, shadow="lifted") {
            // What the record actually stores, and the reason a correction to a food reaches
            // every meal that ever used it. Hidden because it is machinery: a diary is read in
            // names, and the name below is looked up through this.
            // No default worth eating. A record that lost its handle used to fall back
            // to a real food and resolve into a plausible meal nobody had - three of them
            // are in the history. Empty, the lookup finds nothing and the record says so.
            string food (label="Food", hidden=true, width=20%) = ""

            // The catalogued weight of one portion of this food, used to convert between
            // the two amount styles. Not shown - it belongs to the food, not to the meal.
            float portion_weight (hidden=true) = $(FOODS/Catalog.filter(|x| x/handle == ../food)/portion_weight)

            // What the record is, on one line: which food, how much of it, and how good it is.
            // The macros go underneath, so a meal reads as a heading with its detail below.
            // The group has no name, so it changes the layout and nothing else.
            // When it was eaten, what it was, and what it cost. One row, because a
            // name that has the width to itself does not wrap.
            div (layout="horizontal", margin=0, alignment="center") {
                // When it was eaten. The clock only: the day is the entry this record sits in,
                // and repeating it on every mouthful would say nothing thirteen times a day.
                //
                // Stored as a full instant all the same - a time without a date is not a moment,
                // and `format` decides how much of one is shown, not how much is kept.
                timestamp at (label="", format="time", precision="minutes", font-size=13px,
                              width=8%, hidden=$(at == "")) = ""
                // The name, and under it the rest of the name.
                //
                // A column, so the qualifier sits directly beneath what it qualifies and reads as
                // part of the same thing rather than as another field. `spacing=0` because the
                // two are one label in two sizes - any gap at all and they read as two.
                div (layout="vertical", margin=0, spacing=0, width=62%) {
                // `margin=0, padding=0` on both, and it is the whole reason the two sit together.
                // A field carries eight pixels of padding and eight of margin above and below by
                // default, so two stacked ones put thirty-two pixels of nothing between two lines
                // that want two. `spacing` on the column does not help: it never becomes a gap.
                    string name (font-size=20px, margin=0, padding=0, value-padding="0 8px") = $(FOODS/Catalog.filter(|x| x/handle == ../food)/name)
                    // The part of the name that is not the name - a flavour, a crust, a cut.
                    // Smaller and under it, and drawn at all only when the food has one.
                    string sub_name (label="", font-size=12px, margin=0, padding=0, value-padding="0 8px", hidden=$(sub_name == "")) = $(FOODS/Catalog.filter(|x| x/handle == ../food)/sub_name)
                }
                // The one figure most meals are read for, at a size to match. It sits beside
                // the grade rather than in the table below, where it was one number among
                // thirteen.
                float calories (suffix=" kcal", precision=0, font-size=32px, width=30%) = $(grams * FOODS/Catalog.filter(|x| x/handle == ../food)/per_100g/calories * 0.01)
            }
            // What qualifies the meal: which kind of the food it was, what the food is, and how
            // much of it. Kept off the first row so a long name has the width to itself.
            //
            // Centred because the chips have no label above them and the amounts do, and without
            // it the chips ride up against the top of the row while the numbers sit lower.
            div (layout="horizontal", margin=0, alignment="center") {
                // What the food is, looked up the same way its name and its macros are.
                //
                // Not editable here, and that is the point rather than a restriction: these
                // belong to the food, not to this meal. An editable field would write the tags
                // onto the record, where the next resolve would overwrite them from the
                // catalogue - a change that appears to work and then quietly does not. Change
                // them in foods.os and every meal that ever ate it says the same new thing.
                tags labels (label="", vocabulary="tracker_v2/VOCAB/Labels", mutable=false, width=36%) = $(FOODS/Catalog.filter(|x| x/handle == ../food)/labels)
                // Whichever of these a record states, the other is derived. Both are null here
                // so neither shadows the other; a record must state one of them.
                float portions (label="portions", precision=2, format="trim", width=18%, fallback=$(grams / portion_weight), default=1) = null
                float grams (label="weight", suffix=" g", precision=0, width=20%, fallback=$(portions * portion_weight)) = null
            }
            // The detail, and the grade beside it rather than above it. The grade is worked out
            // from these very figures, so reading them together is reading one thing; and the
            // macro rows stop short of the right edge, which is exactly the space a single large
            // letter wants.
            div (layout="horizontal", margin=0, alignment="center") {
            // Two even rows rather than one long one, so a meal is a filled block
            // instead of a line of figures trailing off the side of the card.
                // `padding=0` here and on the two rows inside, so the figures start where the
                // tag chips above them do. Every level of nesting adds eight pixels: the chips are
                // a field inside a row and sit at sixteen, while these were a field inside a row
                // inside this wrapper inside a row, at thirty-two - which read as the chips being
                // badly inset when it was the macros being doubly indented.
                div macros (layout="vertical", margin=0, padding=0, border-style=none, shadow="none", width=88%) {
                    div (layout="horizontal", margin=0, padding=0) {
                        float protein (label="Protein", suffix=" g", precision=1) = $(grams * FOODS/Catalog.filter(|x| x/handle == ../food)/per_100g/protein * 0.01)
                        float fat (label="Fat", suffix=" g", precision=1) = $(grams * FOODS/Catalog.filter(|x| x/handle == ../food)/per_100g/fat * 0.01)
                        float saturated_fat (label="Sat. Fat", suffix=" g", precision=1) = $(grams * FOODS/Catalog.filter(|x| x/handle == ../food)/per_100g/saturated_fat * 0.01)
                        float trans_fat (label="Trans Fat", suffix=" g", precision=1) = $(grams * FOODS/Catalog.filter(|x| x/handle == ../food)/per_100g/trans_fat * 0.01)
                        float carbs (label="Carbs", suffix=" g", precision=1) = $(grams * FOODS/Catalog.filter(|x| x/handle == ../food)/per_100g/carbs * 0.01)
                        float sugar (label="Sugar", suffix=" g", precision=1) = $(grams * FOODS/Catalog.filter(|x| x/handle == ../food)/per_100g/sugar * 0.01)
                        float sugar_added (label="Added", suffix=" g", precision=1) = $(grams * FOODS/Catalog.filter(|x| x/handle == ../food)/per_100g/sugar_added * 0.01)

                        // Beside the energy macros because that is what it is: alcohol carries
                        // 7 kcal a gram, and a day with any in it is not read the same way.
                        float alcohol (label="Alcohol", suffix=" g", precision=1) = $(grams * FOODS/Catalog.filter(|x| x/handle == ../food)/per_100g/alcohol * 0.01)
                    }
                    div (layout="horizontal", margin=0, padding=0) {
                        float fibre (label="Fibre", suffix=" g", precision=1) = $(grams * FOODS/Catalog.filter(|x| x/handle == ../food)/per_100g/fibre * 0.01)
                        float salt (label="Salt", suffix=" g", precision=2) = $(grams * FOODS/Catalog.filter(|x| x/handle == ../food)/per_100g/salt * 0.01)
                        float vitamin_d (label="Vit. D", suffix=" µg", precision=1) = $(grams * FOODS/Catalog.filter(|x| x/handle == ../food)/per_100g/vitamin_d * 0.01)
                        float calcium (label="Calcium", suffix=" mg", precision=0) = $(grams * FOODS/Catalog.filter(|x| x/handle == ../food)/per_100g/calcium * 0.01)
                        float iron (label="Iron", suffix=" mg", precision=1) = $(grams * FOODS/Catalog.filter(|x| x/handle == ../food)/per_100g/iron * 0.01)
                        float potassium (label="Potassium", suffix=" mg", precision=0) = $(grams * FOODS/Catalog.filter(|x| x/handle == ../food)/per_100g/potassium * 0.01)

                        // The other row, because caffeine is not an energy macro - it is the one
                        // figure here that says something about the shape of a day rather than
                        // about what was in it.
                        float caffeine (label="Caffeine", suffix=" mg", precision=0) = $(grams * FOODS/Catalog.filter(|x| x/handle == ../food)/per_100g/caffeine * 0.01)
                    }
                }

                // The food's own grade, in the space the macro rows leave. Its own figures are
                // per 100 g already, which is the side the scheme is defined on, so how much was
                // eaten does not come into it.
                <NutriScore> quality {
                    - kcal = $(FOODS/Catalog.filter(|x| x/handle == ../../food)/per_100g/calories)
                    - sugar = $(FOODS/Catalog.filter(|x| x/handle == ../../food)/per_100g/sugar)
                    - sat_fat = $(FOODS/Catalog.filter(|x| x/handle == ../../food)/per_100g/saturated_fat)
                    - salt = $(FOODS/Catalog.filter(|x| x/handle == ../../food)/per_100g/salt)
                    - fibre = $(FOODS/Catalog.filter(|x| x/handle == ../../food)/per_100g/fibre)
                    - protein = $(FOODS/Catalog.filter(|x| x/handle == ../../food)/per_100g/protein)
                }
            }
        }

        div DayRecord (layout="vertical") {
            timestamp date (precision="day") = $(today())

            // The whole day on one row: what day it is, what it came to, what it was
            // aiming at, how good it was, and the two buttons that add to it. The list of
            // what was actually eaten goes underneath, so a day reads as a summary with its
            // detail below instead of a column of boxes.
            //
            // The wrapper has no name of its own, which keeps it out of the addresses -
            // `History/[2026-08-09]/intake` still means what it did before.
            div (layout="horizontal", margin=0, width=100%, alignment="center") {

                // Two rows of six, for the same reason the meals have them: thirteen
            // figures in a line leaves a strip of empty space under every one of them.
            div totals (layout="vertical", margin=0, border-radius=0, shadow="soft") {
                div (layout="horizontal", margin=0) {
                    // Not hidden, though the headline shows it again in large type. A node
                    // called `totals` that omits the total is a trap: anything reading the
                    // day finds every macro except the one figure it came for, and fills
                    // the gap from somewhere else. That is exactly how a morning's first
                    // meal got reported against yesterday's running total.
                    float calories (label="Total kcal", precision=0) = $(intake.map(|x| x/calories).sum())
                    float protein (label="Protein", suffix=" g", precision=1) = $(intake.map(|x| x/macros/protein).sum())
                    float fat (label="Fat", suffix=" g", precision=1) = $(intake.map(|x| x/macros/fat).sum())
                    float saturated_fat (label="Sat. Fat", suffix=" g", precision=1) = $(intake.map(|x| x/macros/saturated_fat).sum())
                    float carbs (label="Carbs", suffix=" g", precision=1) = $(intake.map(|x| x/macros/carbs).sum())
                    float sugar (label="Sugar", suffix=" g", precision=1) = $(intake.map(|x| x/macros/sugar).sum())
                    float sugar_added (label="Added sugar", suffix=" g", precision=1) = $(intake.map(|x| x/macros/sugar_added).sum())
                    float alcohol (label="Alcohol", suffix=" g", precision=1) = $(intake.map(|x| x/macros/alcohol).sum())
                }
                div (layout="horizontal", margin=0) {
                    float fibre (label="Fibre", suffix=" g", precision=1) = $(intake.map(|x| x/macros/fibre).sum())
                    float salt (label="Salt", suffix=" g", precision=2) = $(intake.map(|x| x/macros/salt).sum())
                    float vitamin_d (label="Vit. D", suffix=" µg", precision=1) = $(intake.map(|x| x/macros/vitamin_d).sum())
                    float calcium (label="Calcium", suffix=" mg", precision=0) = $(intake.map(|x| x/macros/calcium).sum())
                    float iron (label="Iron", suffix=" mg", precision=1) = $(intake.map(|x| x/macros/iron).sum())
                    float potassium (label="Potassium", suffix=" mg", precision=0) = $(intake.map(|x| x/macros/potassium).sum())
                    float caffeine (label="Caffeine", suffix=" mg", precision=0) = $(intake.map(|x| x/macros/caffeine).sum())
                }

                // Where the day's calories came from, by what the food was.
                //
                // Hidden because the pie beside the macros is how these are read; they are here
                // rather than in the chart so that anything else - a month's average, the bot
                // answering "how much meat this week" - can have them without repeating the
                // arithmetic.
                //
                // The tag test is nested inside the predicate rather than worked out on each
                // meal. A `tags` field offers its tags to a method chain as a list, so
                // `x/labels.filter(|tag| tag == "meat").count() > 0` asks whether a meal's food
                // carries one. Doing it here keeps a meal row as it was - three more hidden
                // fields on every meal would cost more than these four formulas do.
                //
                // `vegetarian` is strict: vegetarian and not vegan, so the two slices do not
                // count the same food twice. Every vegan food carries `vegetarian` as well, and
                // without the second half of that test every vegan meal would appear in both.
                //
                // Nothing here pretends a portion of beef is entirely beef. A tagged food puts
                // all of its calories in one slice, which is wrong in detail and right enough
                // for the only question being asked: whether this is going up or down.
                float vegan_calories (hidden=true) = $(intake.filter(|x| x/labels.filter(|tag| tag == "vegan").count() > 0).map(|x| x/calories).sum())
                float vegetarian_calories (hidden=true) = $(intake.filter(|x| x/labels.filter(|tag| tag == "vegetarian").count() > 0 && x/labels.filter(|tag| tag == "vegan").count() == 0).map(|x| x/calories).sum())
                float meat_calories (hidden=true) = $(intake.filter(|x| x/labels.filter(|tag| tag == "meat").count() > 0).map(|x| x/calories).sum())

                // Whatever the three above did not claim: fish, a food carrying only `dairy`,
                // and everything nobody has classified yet. Taken as the remainder rather than
                // worked out from its own predicate, so the four always add up to the day - a
                // pie whose slices do not is worse than no pie.
                float other_calories (hidden=true) = $(calories - vegan_calories - vegetarian_calories - meat_calories)
            }

                // The most recent day recorded before this one. Worked out by date rather than by
                // What the day is aiming at: the standing target at the top of the document.
                //
                // This used to be carried from the day before it, each day reading the one before
                // that in turn - a chain as long as the history, walked again every time any
                // figure on the page was wanted. It answered the same number every time, because
                // the walk always reached the seed: no day had ever stated a target of its own,
                // and none could. A list entry has no way to write into a nested div, so these
                // two fields were never reachable from the history at all.
                //
                // Moving them up to the day itself is what it would take to let one day differ
                // from the rest. Until something needs that, one standing figure is the honest
                // shape - and it is what the chain was computing anyway.
                div targets (label="Targets", layout="horizontal", margin=0, border-radius=0, shadow="soft") {
                    float target_calories (label="Target kcal", precision=0,
                                           fallback=$(/tracker_v2/standing_calories)) = null
                    float target_protein (label="Target protein", suffix=" g", precision=0,
                                          fallback=$(/tracker_v2/standing_protein)) = null
                }

                // Everything eaten today, graded as though it were one food: the day's totals
                // spread back over the weight it took to get them. A day with nothing in it has no
                // grade to give, so the figures fall to zero rather than dividing by it.
                //
                // Written out here rather than through the <NutriScore> block the meals use. A
                // template instance anywhere in this record makes the comment further down lose
                // its indentation every time the file is written - see the ignored test in
                // tests/comment_after_childless_list.rs. The rules are the same rules; when that
                // is fixed this becomes an instance like the meals'.
                float eaten_grams (hidden=true) = $(intake.map(|x| x/grams).sum())
                float day_kj (hidden=true) = $(eaten_grams > 0 ? totals/calories / eaten_grams * 100 * 4.184 : 0)
                float day_sugar (hidden=true) = $(eaten_grams > 0 ? totals/sugar / eaten_grams * 100 : 0)
                float day_sat_fat (hidden=true) = $(eaten_grams > 0 ? totals/saturated_fat / eaten_grams * 100 : 0)
                float day_sodium_mg (hidden=true) = $(eaten_grams > 0 ? totals/salt / eaten_grams * 100 * 400 : 0)
                float day_fibre (hidden=true) = $(eaten_grams > 0 ? totals/fibre / eaten_grams * 100 : 0)
                float day_protein (hidden=true) = $(eaten_grams > 0 ? totals/protein / eaten_grams * 100 : 0)
                int day_p_energy (hidden=true) = $(day_kj > 3350 ? 10 : (day_kj > 3015 ? 9 : (day_kj > 2680 ? 8 : (day_kj > 2345 ? 7 : (day_kj > 2010 ? 6 : (day_kj > 1675 ? 5 : (day_kj > 1340 ? 4 : (day_kj > 1005 ? 3 : (day_kj > 670 ? 2 : (day_kj > 335 ? 1 : (0)))))))))))
                int day_p_sugar (hidden=true) = $(day_sugar > 45 ? 10 : (day_sugar > 40 ? 9 : (day_sugar > 36 ? 8 : (day_sugar > 31 ? 7 : (day_sugar > 27 ? 6 : (day_sugar > 22.5 ? 5 : (day_sugar > 18 ? 4 : (day_sugar > 13.5 ? 3 : (day_sugar > 9 ? 2 : (day_sugar > 4.5 ? 1 : (0)))))))))))
                int day_p_sat_fat (hidden=true) = $(day_sat_fat > 10 ? 10 : (day_sat_fat > 9 ? 9 : (day_sat_fat > 8 ? 8 : (day_sat_fat > 7 ? 7 : (day_sat_fat > 6 ? 6 : (day_sat_fat > 5 ? 5 : (day_sat_fat > 4 ? 4 : (day_sat_fat > 3 ? 3 : (day_sat_fat > 2 ? 2 : (day_sat_fat > 1 ? 1 : (0)))))))))))
                int day_p_sodium (hidden=true) = $(day_sodium_mg > 900 ? 10 : (day_sodium_mg > 810 ? 9 : (day_sodium_mg > 720 ? 8 : (day_sodium_mg > 630 ? 7 : (day_sodium_mg > 540 ? 6 : (day_sodium_mg > 450 ? 5 : (day_sodium_mg > 360 ? 4 : (day_sodium_mg > 270 ? 3 : (day_sodium_mg > 180 ? 2 : (day_sodium_mg > 90 ? 1 : (0)))))))))))
                int day_p_fibre (hidden=true) = $(day_fibre > 4.7 ? 5 : (day_fibre > 3.7 ? 4 : (day_fibre > 2.8 ? 3 : (day_fibre > 1.9 ? 2 : (day_fibre > 0.9 ? 1 : (0))))))
                int day_p_protein (hidden=true) = $(day_protein > 8.0 ? 5 : (day_protein > 6.4 ? 4 : (day_protein > 4.8 ? 3 : (day_protein > 3.2 ? 2 : (day_protein > 1.6 ? 1 : (0))))))
                // Kept together, and out of the flow of the fields: loose in the row
                // they wrapped separately, one ending up on a line of its own.
                div (layout="horizontal", margin=0) {
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
                int day_score (hidden=true) = $((day_p_energy + day_p_sugar + day_p_sat_fat + day_p_sodium) >= 11 ? (day_p_energy + day_p_sugar + day_p_sat_fat + day_p_sodium) - day_p_fibre : (day_p_energy + day_p_sugar + day_p_sat_fat + day_p_sodium) - day_p_fibre - day_p_protein)
                // What the day came to and how good it was, side by side.
                div day_headline (layout="horizontal", margin=0, background-color="#16203a", border-radius=0, shadow="lifted") {
                    float day_calories (label="Total kcal", precision=0, font-size=34px, width=25%, font-color=$(totals/calories > targets/target_calories ? "#e63e11" : (totals/calories > targets/target_calories * 0.9 ? "#ee8100" : "#7ed321"))) = $(totals/calories)
                    string day_grade (label="Day quality", font-size=64px, font-color=$(((day_p_energy + day_p_sugar + day_p_sat_fat + day_p_sodium) >= 11 ? (day_p_energy + day_p_sugar + day_p_sat_fat + day_p_sodium) - day_p_fibre : (day_p_energy + day_p_sugar + day_p_sat_fat + day_p_sodium) - day_p_fibre - day_p_protein) <= -1 ? "#038141" : (((day_p_energy + day_p_sugar + day_p_sat_fat + day_p_sodium) >= 11 ? (day_p_energy + day_p_sugar + day_p_sat_fat + day_p_sodium) - day_p_fibre : (day_p_energy + day_p_sugar + day_p_sat_fat + day_p_sodium) - day_p_fibre - day_p_protein) <= 2 ? "#85bb2f" : (((day_p_energy + day_p_sugar + day_p_sat_fat + day_p_sodium) >= 11 ? (day_p_energy + day_p_sugar + day_p_sat_fat + day_p_sodium) - day_p_fibre : (day_p_energy + day_p_sugar + day_p_sat_fat + day_p_sodium) - day_p_fibre - day_p_protein) <= 10 ? "#fecb02" : (((day_p_energy + day_p_sugar + day_p_sat_fat + day_p_sodium) >= 11 ? (day_p_energy + day_p_sugar + day_p_sat_fat + day_p_sodium) - day_p_fibre : (day_p_energy + day_p_sugar + day_p_sat_fat + day_p_sodium) - day_p_fibre - day_p_protein) <= 18 ? "#ee8100" : ("#e63e11"))))), width=20%) = $(((day_p_energy + day_p_sugar + day_p_sat_fat + day_p_sodium) >= 11 ? (day_p_energy + day_p_sugar + day_p_sat_fat + day_p_sodium) - day_p_fibre : (day_p_energy + day_p_sugar + day_p_sat_fat + day_p_sodium) - day_p_fibre - day_p_protein) <= -1 ? "A" : (((day_p_energy + day_p_sugar + day_p_sat_fat + day_p_sodium) >= 11 ? (day_p_energy + day_p_sugar + day_p_sat_fat + day_p_sodium) - day_p_fibre : (day_p_energy + day_p_sugar + day_p_sat_fat + day_p_sodium) - day_p_fibre - day_p_protein) <= 2 ? "B" : (((day_p_energy + day_p_sugar + day_p_sat_fat + day_p_sodium) >= 11 ? (day_p_energy + day_p_sugar + day_p_sat_fat + day_p_sodium) - day_p_fibre : (day_p_energy + day_p_sugar + day_p_sat_fat + day_p_sodium) - day_p_fibre - day_p_protein) <= 10 ? "C" : (((day_p_energy + day_p_sugar + day_p_sat_fat + day_p_sodium) >= 11 ? (day_p_energy + day_p_sugar + day_p_sat_fat + day_p_sodium) - day_p_fibre : (day_p_energy + day_p_sugar + day_p_sat_fat + day_p_sodium) - day_p_fibre - day_p_protein) <= 18 ? "D" : ("E")))))

                    // Energy, not weight: a gram of fat carries nine calories and a gram of
                    // protein four, so shares by weight would say something else entirely.
                    chart day_macros (kind="pie", width=220px, height=150px) {
                        plot protein (label="Protein", color="#4A90E2", amount=$(totals/protein * 4))
                        plot fat (label="Fat", color="#e2a24a", amount=$(totals/fat * 9))
                        plot carbs (label="Carbs", color="#7ed321", amount=$(totals/carbs * 4))
                    }

                    // And where they came from. Same day, same calories, cut a different way:
                    // the macros pie asks what the energy was made of, this one asks what it was
                    // made from. Reduction is the point of it - a wedge that shrinks week on
                    // week is the whole reading.
                    chart day_sources (kind="pie", width=220px, height=150px) {
                        plot vegan (label="Vegan", color="#0e8a16", amount=$(totals/vegan_calories))
                        plot vegetarian (label="Vegetarian", color="#4caf50", amount=$(totals/vegetarian_calories))
                        plot meat (label="Meat", color="#b71c1c", amount=$(totals/meat_calories))
                        plot other (label="Other", color="#6b7280", amount=$(totals/other_calories))
                    }

                    // And how much of the day's allowances went. A third question about the same
                    // day: the pies ask what the energy was and where it came from, this asks
                    // what is nearly used up.
                    //
                    // Each bar is a share of its own limit, which is the only way these are
                    // comparable - salt is in grams and caffeine in milligrams, and on one axis
                    // the salt bar would be a thousandth the height and say nothing. The dashed
                    // line is every limit at once, and a bar past it turns red.
                    //
                    // Alcohol is deliberately absent. A day is the wrong window for it: nothing
                    // useful is said by "you were under the limit today" when the figure worth
                    // watching is the week. It wants a rolling total, which is a different chart.
                    chart day_allowances (kind="bar", width=240px, height=150px) {
                        plot calories (label="kcal", color="#4A90E2", amount=$(totals/calories), limit=$(targets/target_calories), suffix=" kcal")
                        plot salt (label="Salt", color="#e2a24a", amount=$(totals/salt), limit=$(/tracker_v2/limits/limit_salt), suffix=" g")
                        plot sat_fat (label="Sat. fat", color="#d0a34a", amount=$(totals/saturated_fat), limit=$(/tracker_v2/limits/limit_saturated_fat), suffix=" g")
                        plot sugar (label="Added sugar", color="#c77dd6", amount=$(totals/sugar_added), limit=$(/tracker_v2/limits/limit_sugar), suffix=" g")
                        plot caffeine (label="Caffeine", color="#5d4037", amount=$(totals/caffeine), limit=$(/tracker_v2/limits/limit_caffeine), suffix=" mg")
                    }
                }


                // These buttons live inside the day, so the intake path above targets whichever
                // day they are rendered for, including the one shown through the selected-day
                // link. An absolute or keyed path cannot express the day currently on screen:
                // action targets do not understand key selectors, and a link is resolved by the
                // renderer rather than by the backend that runs the append.
                //
                // There are two of them because each writes only its own amount, leaving the
                // other unset so it derives. A single button would have to decide which of the
                // two draft values was meant, and there is no way to ask whether a field is set.
            }



            // Side by side, wrapping onto the next row: a meal card is nowhere near a
            // screen wide, and stacking them one per row was most of the empty space.
            list intake (entry=<MealRecord>, layout="flow", min-width=520px)
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
        timestamp selected_date (precision="day", mutable="guarded") = $(today())
        button Next (label="> Next Day") {
            on click {
                set (path="/tracker_v2/Selected/selected_date") = $(date_add_days(../selected_date, 1))
            }
        }

        div SelectedDay (mutable=true, layout="vertical", link="/tracker_v2/History[key=$(../selected_date)]", phantom-materialize="append-on-edit") {
            // The same layout the day itself uses. Declaring the list here without
            // one made the selected day the only one shaped differently.
            list intake (hidden=false, layout="flow", min-width=520px)
        }
    }

    // What every day aims at.
    //
    // One figure rather than one per day. Change it and every day follows, including those
    // already recorded - which is the honest reading of "this is what I am aiming at", and is
    // what the document did before this anyway.
    div standing (label="Standing targets", layout="horizontal", margin=0) {
        float standing_calories (label="kcal a day", precision=0) = 1800
        float standing_protein (label="protein a day", suffix=" g", precision=0) = 100
    }

    // What a day is meant to stay under.
    //
    // Editable like everything else, and worth editing: these are general figures, not advice
    // for one person. Where each comes from, so a figure can be argued with rather than merely
    // changed - the WHO and EFSA ones are population guidance for adults:
    //
    //   - salt            5 g          WHO, less than five grams a day
    //   - saturated fat   20 g         under a tenth of the energy in a 1,800 kcal day
    //   - sugar           50 g         WHO, under a tenth of the energy - see the caveat below
    //   - caffeine        400 mg       EFSA, without safety concern for a healthy adult
    //
    // Calories have no figure here on purpose: `standing_calories` above is already the number
    // being aimed at, and two places to state the same thing is one place to get it wrong.
    //
    // The sugar figure is the loose one, and knowingly so. WHO's ten per cent is about *free*
    // sugars - what is added, plus juice - and the catalogue records *total* sugar, which counts
    // the sugar in fruit and milk as well. So the sugar bar reads higher than the guidance it is
    // named after, and a day of fruit can fill it without anything being wrong. Kept because the
    // trend still means something; drop it if it reads as an accusation.
    div limits (label="Daily limits", layout="horizontal", margin=0) {
        float limit_salt (label="salt", suffix=" g", precision=1) = 5
        float limit_saturated_fat (label="sat. fat", suffix=" g", precision=0) = 20
        float limit_sugar (label="sugar", suffix=" g", precision=0) = 50
        float limit_caffeine (label="caffeine", suffix=" mg", precision=0) = 400
    }

    // Newest first, so today is at the top and the scroll goes backwards in time. `sort_by`
    // sorts ascending only, so the instant is negated - the same idiom the diary, the shopping
    // list and the blood pressure log all use. `millis_since_epoch` rather than "how long ago",
    // because a key that follows the clock changes every minute and would then put every entry
    // into the delta of every interaction.
    //
    // Presentation only: the file keeps the days in whatever order they were written in, which
    // matters here because days added through the UI are prepended and days added by the bot are
    // appended, so the stored order has never been the order to read them in.
    // On one line because the serialiser writes parameters on one line, and this document is
    // checked byte for byte across a save - see tests/app_load_save_cycle.rs.
    // Three days in view, and the rest costing nothing.
    //
    // `window` is applied before anything is instantiated, so a day out of view has no template
    // copied onto it, no formula worked out, and no entry in the dependency graph - and neither
    // does the list of meals inside it. The history is forty-three days and grows; three is what
    // anyone reads.
    //
    // Which three depends on `sort_by` above, because that is the order they are shown in. Nothing
    // here aggregates across the history - the charts are a pie per day - so a day out of view is
    // genuinely unread rather than quietly wrong.
    list History (entry=<DayRecord>, key="date", keyPrecision="day", window=3, sort_by=$(|x| 0 - millis_since_epoch(x/date))) {
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
            - date = "2026-08-05"
            list intake {
                - {
                    - food = "apple"
                    - portions = 1
                }
            }
        }
        - {
            - date = "2026-08-06"
            list intake {
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
            - date = "2026-08-07"

            list intake {
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
            - date = "2026-08-08"

            list intake {
                - {
                    - food = "apple"
                    - portions = 1
                }
                - {
                    - food = "apple"
                    - portions = 1
                }
                - {
                    - food = "banana"
                    - portions = 1
                }
            }
        }
    }
}
