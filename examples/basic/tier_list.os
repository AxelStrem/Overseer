tab tiers (label="Best Food, Ranked", mutable=true) {
    div (hidden=true) {
        div Tier {
            string tag = ""
            string colour = "#cbd5e1"
        }

        div Food (layout="vertical", spacing=2, width=110px, background-color=#ffffff, border-style=solid 1px #e2e8f0, border-radius=8px, font-color=#1e293b) {
            context hide-labels = true
            string emoji (font-size=32px) = ""
            string name (font-weight=bold) = ""
            enum tier (vocabulary="/tiers/Tiers", hidden=true) = ""
        }
    }

    list Tiers (entry=<Tier>, key="tag", hidden=true) {
        - {
            - tag = "S"
            - colour = "#ff7f7f"
        }
        - {
            - tag = "A"
            - colour = "#ffbf7f"
        }
        - {
            - tag = "B"
            - colour = "#ffdf7f"
        }
        - {
            - tag = "C"
            - colour = "#bfff7f"
        }
        - {
            - tag = "D"
            - colour = "#7fdfff"
        }
        - {
            - tag = "F"
            - colour = "#cbd5e1"
        }
    }

    list Ranked (entry=<Food>, key="name", view="board", by="tier", layout="vertical", movable=true) {
        - {
            - emoji = "🍕"
            - name = "Pizza"
            - tier = "S"
        }
        - {
            - emoji = "🍜"
            - name = "Ramen"
            - tier = "S"
        }
        - {
            - emoji = "🌮"
            - name = "Tacos"
            - tier = "S"
        }
        - {
            - emoji = "🍣"
            - name = "Sushi"
            - tier = "A"
        }
        - {
            - emoji = "🥟"
            - name = "Dumplings"
            - tier = "A"
        }
        - {
            - emoji = "🍔"
            - name = "Burger"
            - tier = "A"
        }
        - {
            - emoji = "🍛"
            - name = "Curry"
            - tier = "A"
        }
        - {
            - emoji = "🌯"
            - name = "Burrito"
            - tier = "A"
        }
        - {
            - emoji = "🍲"
            - name = "Pho"
            - tier = "B"
        }
        - {
            - emoji = "🍟"
            - name = "Fries"
            - tier = "B"
        }
        - {
            - emoji = "🧆"
            - name = "Falafel"
            - tier = "B"
        }
        - {
            - emoji = "🥪"
            - name = "Toastie"
            - tier = "B"
        }
        - {
            - emoji = "🥯"
            - name = "Bagel"
            - tier = "B"
        }
        - {
            - emoji = "🥞"
            - name = "Pancakes"
            - tier = "C"
        }
        - {
            - emoji = "🥣"
            - name = "Porridge"
            - tier = "C"
        }
        - {
            - emoji = "🥗"
            - name = "Salad"
            - tier = "C"
        }
        - {
            - emoji = "🫒"
            - name = "Olives"
            - tier = "D"
        }
        - {
            - emoji = "🍥"
            - name = "Marzipan"
            - tier = "D"
        }
        - {
            - emoji = "🍬"
            - name = "Liquorice"
            - tier = "F"
        }
    }

    text unranked_header (markdown=true) = "### Unranked"

    list Unranked (entry=<Food>, key="name", layout="horizontal", spacing=6, movable=true) {
        - {
            - emoji = "🧇"
            - name = "Waffles"
        }
        - {
            - emoji = "🍩"
            - name = "Donut"
        }
        - {
            - emoji = "🍦"
            - name = "Ice Cream"
        }
    }
}
