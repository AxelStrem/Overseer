div FormulaFeatureTest (layout = vertical) {
    // Constants
    int a = 10
    int b = 5
    float c = 2.5

    // Arithmetic
    int sum = $(a + b)                // 15
    int diff = $(a - b)               // 5
    int prod = $(a * b)               // 50
    int quotient = $(a / b)           // 2 (integer normalization)
    int precedence = $(a + b * 2)     // 20
    int grouping = $((a + b) * 2)     // 30

    // Mixed with float, normalized when whole
    float mix1 = $(a + c)             // 12.5
    int mix2 = $((a - c) * 2)         // 15

    // Comparisons (should be booleans)
    bool eq = $(a == 11)              // false
    bool neq = $(a != b)              // true
    bool lt = $(b < a)                // true
    bool lte = $(b <= 5)              // true
    bool gt = $(a > b)                // true
    bool gte = $(a >= 10)             // true

    // Boolean logic
    bool and1 = $((a > b) && (b == 5))    // true
    bool and2 = $((a < b) && (b == 5))    // false
    bool or1 = $((a < b) || (b == 5))     // true
    bool not1 = $(!(a < b))               // true

    // Ternary operator
    int tern1 = $((a > b) ? 1 : 0)        // 1
    int tern2 = $(and2 ? 100 : 200)      // 200

    // Function
    date todayValue = $(today())      // current date (YYYY-MM-DD)

    // Path references
    div ParentScope {
    int income = 5000
        div ChildScope {
            int bonus = 1000
            div GrandChild {
        div Inner { int x = 3 }
            }
            int total1 = $(../income + bonus)    // 6000 via parent + sibling
            int total2 = $((../income + ../bonus) * 1) // 6000 via explicit parent on both
            int total3 = $(GrandChild/Inner/x * bonus) // 3000 via path
        // Parameter extraction from a sibling node parameter
        div label (background-color = #FF2233) {
                string "label text"
            }
        string colorGrab = $(label.background-color)
        }
        
    }
    // Cross to grandparent
    div Outer {
        int base = 4
        div Mid {
            div Inner {
                int calc = $((../../base + 1) * 2) // 10
            }
        }
    }

    // Parameter formulas: color depends on boolean
   
    div Colorful (background-color=$(/isOk ? #AABBFF : #AA5555)) {
        string Note = "Color based on isOk"
        bool isOk = true
    }
}
