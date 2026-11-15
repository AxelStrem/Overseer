div (hidden=true) {
    div Card (border-style=solid 1px gray, padding=8, margin=4) {
        string title (label="") = ""
        string note (label="Note") = "plain"
        int size (label="Size") = 1
    }
    div Fancy (border-style=solid 2px #d97706) {
        string note (font-weight="bold") = "fancy"
        string ribbon (label="Ribbon") = "gold"
    }
    div Big {
        int size = 3
    }

    div Food (layout="horizontal") {
        string handle (label="Handle", width=20%) = ""
        string name (label="Name", width=40%) = ""
        float kcal (label="kcal per 100 g", width=40%) = 100
    }
    div Meal (layout="horizontal") {
        string handle (hidden=true) = ""
        string name (label="", width=40%) = ""
        float kcal (hidden=true) = 0
        float grams (label="Grams", width=30%) = 100
        float eaten (label="kcal", width=30%, precision=0, mutable=false) = $(grams * kcal * 0.01)
    }
}

tab cards (label="Cards", mutable=true) {
    <Card> plain {
        - title = "Card"
    }
    <Card, Fancy> fancy {
        - title = "Card, Fancy"
    }
    <Card, Fancy, Big> all {
        - title = "Card, Fancy, Big"
        - note = "and a note of its own"
    }
}

tab meals (label="Meals", mutable=true) {
    float total (label="Eaten", suffix=" kcal", precision=0, mutable=false) = $(Meals.sum(|x| x/eaten))

    list Meals (entry=<Meal>, layout="vertical") {
        - <Catalog/[apple]> {
            - grams = 180
        }
        - <Catalog/[bread]> { }
        - {
            - name = "Soup"
            - kcal = 40
            - grams = 300
        }
    }

    string picked (label="Food to add") = "butter"

    button add (label="Add a meal") {
        on click {
            append (list="../Meals", with=$("Catalog/[" + ../picked + "]"))
        }
    }

    text catalogue (markdown=true) = "### Catalogue"

    list Catalog (entry=<Food>, key="handle", layout="vertical") {
        - {
            - handle = "apple"
            - name = "Apple"
            - kcal = 52
        }
        - {
            - handle = "bread"
            - name = "Bread"
            - kcal = 250
        }
        - {
            - handle = "butter"
            - name = "Butter"
            - kcal = 717
        }
    }
}
