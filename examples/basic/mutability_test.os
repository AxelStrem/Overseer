int A (mutable=true) = 44
int B (mutable=false) = 10
int C (mutable="guarded") = 11
int D = 6

div nested (mutable=true)
{
    int F = 88
    string G = "asdff"
    
    list L1 (entry=string)
    {
        string = "V1"
        string = "V2"
    }

    div T
    {
        int X = 4
    }

    list L2 (entry=<T>)
    {
        - { int X = 5 }
        - { int X = 7 }
    }
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
