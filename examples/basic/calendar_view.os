tab food (label="Food", mutable=true) {
    int target (label="Target, kcal") = 2000

    div (hidden=true) {
        div Day {
            int kcal (label="Calories") = 0
            string note (label="Note") = ""
        }
        <Day> HeatDay (background-color=$(kcal == 0 ? "" : kcal > ../../target ? "#e5534b" : "#30a14e")) { }
    }

    date selected (label="Day", mutable="guarded") = $(today())

    calendar Days (entry=<Day>, span="day", at=$(../selected), layout="day") {
        - 2026-11-03 {
            - kcal = 1850
        }
        - 2026-11-04 {
            - kcal = 2400
            - note = "birthday"
        }
        - 2026-11-05 {
            - kcal = 1990
        }
    }

    calendar Recent (of="../Days", from=$(date_add_days(../selected, -6)), to=$(../selected), layout="agenda")

    calendar Year (of="../Days", entry=<HeatDay>, from=$(date_add_days(today(), -364)), to=$(today()), layout="year")

    int week (label="Calories this week", mutable=false) = $(Recent.cells().sum(|d| d/kcal))
}
