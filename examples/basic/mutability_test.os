int A (mutable=true) = 66
int B (mutable=false) = 10
int C (mutable="guarded") = 11
int D = 6

list L (mutable=true, entry=int) {
    - 10
    - 20
}

div dr {

    button Prev (label="Update") {
        on click {
            set (path="/dr/E") = 20

        }
    }

    int E (mutable="guarded") = 10
   // timestamp selected_date (mutable="guarded", precision="day") = "2025-09-23"

}
