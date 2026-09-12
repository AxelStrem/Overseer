// Shopping: what we need, where it can be got, and what was bought.
// -
// The list is standing rather than per-trip. Things are added as they run out, and stay until
// someone buys them - so the interesting question is never "what is on the list" but "what can
// I get here", which is what the shop selector answers.
// -
// Where a thing can be bought belongs to the *type*, not to the entry: milk is sold at the
// same shops whether or not it is on the list today. An entry says which type, how much, and
// anything worth remembering about this particular one.
// -
// Buying is a button rather than a message to the bot, because it happens in a shop with one
// hand full. It moves the entry to History - the same two actions the bot would do, written
// once in the document where the finger is.
// -
// Duplicates are allowed: two entries of milk with different amounts are two things to buy.
// Entries are keyed by when they were added, which is unique enough to be an address and does
// not change when the one before it is bought.

tab shopping (label="Shopping", mutable=true) {

    text header (markdown=true) = "# Shopping"

    div (hidden=true) {

        // A shop, and the tag items use to name it.
        div Shop (layout="horizontal", margin=0, alignment="center") {
            string tag (hidden=true) = ""
            string name (font-size=16px, width=40%) = ""

            // What colour this shop's chip takes wherever it is shown. Read from here by any
            // `tags` field pointed at this list, so a shop is recoloured in one place and the
            // change reaches every item that is sold there.
            string colour (label="", width=20%) = "#6b7280"

            // Selecting a shop is what filters the list below.
            button show (label="show", margin=0, width=20%) {
                on click {
                    set (path="/shopping/Selected/selected_shop") = $(../tag)
                }
            }
        }

        // A kind of thing that can be bought, and where.
        div ItemType (layout="horizontal", margin=0, alignment="center") {
            string handle (hidden=true) = ""
            string name (width=35%) = ""
            tags shops (label="sold at", vocabulary="/shopping/Shops", width=55%) = ""
        }

        // Something to buy.
        //
        // Hidden unless the selected shop sells it: the list is long and standing, and what is
        // wanted in a shop is the part of it that shop can answer.
        div Item (layout="horizontal", margin=0, alignment="center", hidden=$(available_here == 0), border-radius=0, shadow="soft") {

            // What the entry stores. `added` is its key, so it is written once and left alone.
            timestamp added (hidden=true) = "2026-01-01T00:00:00Z"
            string handle (hidden=true) = ""

            // Where this type is sold, and whether that includes the shop being looked at.
            //
            // In two steps because a method cannot be chained onto a path that ends at a tag
            // set - `Types.filter(...)/shops.filter(...)` is not a formula the evaluator will
            // take. Held in a `tags` field first, it is one.
            tags shops_here (hidden=true, vocabulary="/shopping/Shops") = $(/shopping/Types.filter(|x| x/handle == ../handle)/shops)
            int available_here (hidden=true) = $(shops_here.filter(|s| s == /shopping/Selected/selected_shop).count())

            string name (font-size=18px, width=30%) = $(/shopping/Types.filter(|x| x/handle == ../handle)/name)
            float amount (precision=2, format="trim", width=10%) = 1
            string commentary (width=38%) = ""

            // Both halves of buying something, from one press.
            button bought (label="bought", margin=0, width=14%) {
                on click {
                    append (list="/shopping/History") {
                        - bought_at = $(now())
                        - handle = $(../handle)
                        - amount = $(../amount)
                        - commentary = $(../commentary)
                        // Which shop was being looked at when the button was pressed. Not
                        // where it was necessarily bought - but the two are the same thing
                        // in the only situation this button gets pressed in.
                        - shop = $(/shopping/Selected/selected_shop)
                    }
                    remove (from="/shopping/List", keyField="added", keyValue=$(../added))
                }
            }
        }

        // Something bought. The same entry with the time it went.
        div Bought (layout="horizontal", margin=0, alignment="center") {
            timestamp bought_at (format="datetime", width=20%) = "2026-01-01T00:00:00Z"
            string handle (hidden=true) = ""
            string name (width=26%) = $(/shopping/Types.filter(|x| x/handle == ../handle)/name)
            float amount (precision=2, format="trim", width=10%) = 1

            // Where it came from, as the shop's own name rather than its tag.
            string shop (hidden=true) = ""
            string shop_name (width=14%) = $(/shopping/Shops.filter(|x| x/tag == ../shop)/name)

            string commentary (width=28%) = ""
        }
    }

    // Which shop the list is being read for. Guarded: looking at a different shop is not a
    // change to the document, and should not be written back to it.
    div Selected (layout="horizontal", margin=0, alignment="center") {
        string selected_shop (label="Showing", mutable="guarded", font-size=18px, width=30%) = "lidl"
    }

    div (layout="horizontal", margin=0) {
        list Shops (entry=<Shop>, key="tag", layout="flow", min-width=260px) {
            - {
                - tag = "lidl"
                - name = "Lidl"
                - colour = "#0e8a16"
            }
            - {
                - tag = "edeka"
                - name = "Edeka"
                - colour = "#1d76db"
            }
            - {
                - tag = "dm"
                - name = "dm"
                - colour = "#8250df"
            }
            - {
                - tag = "ikea"
                - name = "IKEA"
                - colour = "#bf6b00"
            }
        }
    }

    // What to buy, filtered to the shop above.
    list List (entry=<Item>, key="added", layout="vertical") {
        - {
            - added = "2026-08-11T09:00:00Z"
            - handle = "milk"
            - amount = 2
        }
        - {
            - added = "2026-08-11T09:01:00Z"
            - handle = "bread"
            - amount = 1
            - commentary = "the seeded one"
        }
        - {
            - added = "2026-08-11T09:02:00Z"
            - handle = "screws"
            - amount = 20
        }
        - {
            - added = "2026-08-11T09:03:00Z"
            - handle = "toothpaste"
            - amount = 1
        }
    }

    text types_header (markdown=true) = "## Types"

    // What can be bought, and where. A handle is what an entry stores; everything else about
    // the thing is here, so renaming it or adding a shop reaches every entry at once.
    list Types (entry=<ItemType>, key="handle", layout="vertical") {
        - {
            - handle = "milk"
            - name = "Milk"
            - shops = "lidl,edeka"
        }
        - {
            - handle = "bread"
            - name = "Bread"
            - shops = "lidl,edeka"
        }
        - {
            - handle = "screws"
            - name = "Screws, 4x40"
            - shops = "ikea"
        }
        - {
            - handle = "toothpaste"
            - name = "Toothpaste"
            - shops = "dm,edeka"
        }
    }

    text history_header (markdown=true) = "## Bought"

    // Newest last, like every other history here.
    // Newest first. `sort_by` sorts ascending only, so the instant is negated - the same way
    // `tasks/Open` negates its priority. `millis_since_epoch` rather than "how long ago",
    // because a key that follows the clock changes every minute and every entry of it then
    // lands in the delta of every interaction.
    //
    // Presentation only: the file keeps them in the order they happened, so appending stays an
    // append and the backup's line-wise merge sees what it expects.
    list History (entry=<Bought>, layout="vertical",
               sort_by=$(|x| 0 - millis_since_epoch(x/bought_at))) { }
}
