// Link Proxy Basics: demonstrate soft links (views), list ordinals, relative paths, and action-driven switching

// Open this file and click the action button under ItemView to switch
// the linked item from the 2nd list entry (-#1) to the 1st (-#0).

tab Demo {
    // Source data to link into
    div Data {
        string Title = "Hello"
        list Items(entry=string) {
            - "First"
            - "Second"
        }
    }

    // 1) Basic link to a field via absolute path
    //    The container becomes a view into the target node; edits here update the source.
    div TitleView (link="/Demo/Data") {}

    // 2) Link to a list item by ordinal. "-#1" is the second anonymous list entry.
    //    Click the action button to switch the link to the first item (-#0).
    div ItemView (link="/Demo/Data/-#1") {
        on click {
            // Switch this view from second item to first item
            set (path="../link") = "/Demo/Data/-#0"
        }
    }

    // 3) Relative path link: resolved from this container's parent (the tab root here).
    div RelativeView (link="Data/Title") {}
}
