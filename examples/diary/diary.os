// Diary: a day at a time, in two voices.
// -
// One entry per day, holding two accounts of it: the bot's, written each morning from what the
// other documents recorded, and yours, given whenever you feel like giving it. Neither is
// authoritative. The point of keeping both is that they disagree - the figures say a quiet day
// and you remember a hard one, and the gap between those is the thing worth having.
// -
// No figures are kept here on purpose. They live in the documents that own them, where they
// are already correct, and copying them would only create a second version to drift. What is
// kept is what nothing else holds: prose about the day.
// -
// Blood pressure is not read for this and is not mentioned. It is recorded for a doctor, not
// for a narrative.

tab diary (label="Diary", mutable=true) {

    text header (markdown=true) = "# Diary"

    div (hidden=true) {

        div Day (layout="vertical", margin=0, spacing=6, padding=12, shadow="soft") {

            date day (label="", font-size=20px) = "2026-01-01"

            // The bot's account, written the morning after. Long-form, so `text` rather than
            // `string` - it is read as prose, not as a field.
            text recap (markdown=true) = ""

            // Yours. Hidden until there is one: an empty box under every day would suggest a
            // chore, and there is no obligation to answer.
            text mine (markdown=true, hidden=$(mine == ""),
                       background-color="#1b2a1b", padding=8) = ""
        }

        // Something said during the day: how it felt, what was happening, what was being done.
        //
        // Kept in a list of its own rather than inside the day it belongs to. A day's entry
        // does not exist until the morning after, and whether it exists is exactly how the
        // bot decides if the recap has been written yet - so a note arriving at lunchtime
        // would look like a recap that had already happened, and the day would never get one.
        //
        // The full instant is stored and the whole of it shown: these are read in a run
        // spanning days, where a bare clock time would say nothing about which day it was.
        div Note (layout="horizontal", margin=0, spacing=8, alignment="center") {
            timestamp at (label="", format="datetime", precision="minutes",
                          font-size=13px, width=16%) = "2026-01-01T00:00:00Z"
            string note (label="", width=84%) = ""
        }
    }

    // Newest last, like every other history here.
    list Days (entry=<Day>, key="day", layout="vertical", spacing=10) { }

    text notes_header (markdown=true) = "## Notes"

    // What the morning recap is mostly made of. The other documents say what was recorded;
    // these are the only place the day says how it went while it was going.
    list Notes (entry=<Note>, key="at", layout="vertical", spacing=4) { }
}
