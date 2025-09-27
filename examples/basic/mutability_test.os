int A (mutable=true) = 55
int B (mutable=false) = 10
int C (mutable="guarded") = 11
int D = 6

div dr {

    button Prev (label="Update") {
        on click {
            set (path="/dr/E") = 20

        }
    }

    int E (mutable="guarded") = 10
   // timestamp selected_date (mutable="guarded", precision="day") = "2025-09-23"

}
