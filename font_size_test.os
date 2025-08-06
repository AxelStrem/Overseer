div MyContainer {
    font-size: 20px
    background-color: #E8F4FD
    
    list MyList {
        font-size: 24px
        font-color: red
        
        "Large text item" {
            font-size: 32px
        }
        "Medium text item" {
            font-size: 18px
        }
        "Small text item" {
            font-size: 12px
        }
    }
    
    div NestedDiv {
        font-size: 16px
        font-color: blue
        background-color: #FFF2CC
        
        "This should be 16px blue text"
        "This should also be 16px blue text"
    }
    
    "This should be 20px text (inherits from MyContainer)"
}

div AnotherContainer {
    font-size: 2em
    font-color: green
    
    "This should be 2em green text"
    
    div SmallText {
        font-size: 10px
        "This should be 10px green text (inherits color)"
    }
}
