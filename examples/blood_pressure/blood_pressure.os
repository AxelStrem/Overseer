// Blood pressure diary: measurements, grouped into readings.
// -
// A blood pressure meter is used two or three times in a row and the numbers differ each
// time, so a single measurement is not the thing worth looking at - the group is. Every
// measurement is kept with its own timestamp, and the group shows what they came to
// together. One group is one point on the chart whether it holds one measurement or four.
// -
// Grouping is the bot's job, not the document's. A document can average what is inside a
// group; it cannot decide that a new measurement belongs in a different one. The rule lives
// in the guide: within an hour of the group's first measurement, it joins that group.
// -
// The group's key is `started`, written once when the group is made, rather than derived
// from the measurements in it. A derived key would change the moment a second measurement
// arrived - and the address the bot appends that measurement to would stop existing halfway
// through the write.

tab blood_pressure (label="Blood Pressure", mutable=true) {

    text header (markdown=true) = "# Blood Pressure"

    div (hidden=true) {

        // One press of the button on the meter.
        div Measurement (layout="horizontal", margin=0) {
            timestamp time (label="At", format="time", width=18%) = "2026-01-01T08:00:00Z"
            int systolic (label="Sys", width=14%) = 120
            int diastolic (label="Dia", width=14%) = 80
            int pulse (label="Pulse", suffix=" bpm", width=16%) = 70
            bool arrhythmia (label="Irregular", width=16%) = false

            button rem (icon="cross", label="x", margin=0, width=8%) {
                on click {
                    remove (from="../../measurements", keyField="time", keyValue=$(../time))
                }
            }
        }

        // What was taken at one sitting. The figures below are all derived: the group states
        // only when it began, and everything else follows from what is in it.
        div Reading (layout="vertical", margin=0, border-radius=0, shadow="lifted") {

            // What the bot writes when it makes the group, and the only thing here it states
            // rather than works out. It is the key, so it has to hold still.
            timestamp started (hidden=true) = "2026-01-01T08:00:00Z"

            // Irregular if any one of them was. There is no `any`, so this counts the ones
            // that were flagged and the line below asks whether there were any.
            int flagged (hidden=true) = $(measurements.filter(|x| x/arrhythmia).count())

            string colour (hidden=true) = $(category == "Crisis" ? "#e63e11" : (category == "Stage 2" ? "#ee8100" : (category == "Stage 1" ? "#fecb02" : (category == "Elevated" ? "#85bb2f" : "#038141"))))

            // The reading as it is read: when, what it came to, and how that rates. These are
            // the figures themselves rather than copies of them - a second set styled for the
            // card would be a second set for anything reading the document to disagree with.
            div (layout="horizontal", margin=0, alignment="center") {
                timestamp first_at (format="datetime", width=20%) = $(measurements.map(|x| x/time).min())
                float avg_systolic (font-size=30px, precision=0, width=13%, font-color=$(colour)) = $(measurements.map(|x| x/systolic).average())
                float avg_diastolic (font-size=30px, precision=0, prefix="/", width=13%, font-color=$(colour)) = $(measurements.map(|x| x/diastolic).average())
                float avg_pulse (precision=0, suffix=" bpm", width=13%) = $(measurements.map(|x| x/pulse).average())
                // The categories on the back of every meter's leaflet, by the worse of
                // the two figures: a diastolic of 95 is stage 2 whatever the systolic
                // says. Tested from the top down, so the first that matches is the
                // worst that applies.
                string category (font-size=18px, width=16%, font-color=$(colour)) = $(avg_systolic >= 180 ? "Crisis" : (avg_diastolic >= 120 ? "Crisis" : (avg_systolic >= 140 ? "Stage 2" : (avg_diastolic >= 90 ? "Stage 2" : (avg_systolic >= 130 ? "Stage 1" : (avg_diastolic >= 80 ? "Stage 1" : (avg_systolic >= 120 ? "Elevated" : "Normal")))))))
                string irregular (font-size=14px, width=12%, font-color="#e63e11") = $(flagged > 0 ? "irregular" : "")
                int count (label="taken", font-size=12px, width=8%) = $(measurements.count())
            }

            // Anything worth knowing about the state the reading was taken in - what came
            // before it, how they felt. Empty on almost every group, which is why it is a
            // plain field with no label: a row of empty "Note:" headings would be noise on
            // every reading that had nothing to say.
            string commentary (width=100%) = ""

            // The individual presses. Kept because the spread between them says something the
            // average does not, and because a wrong one has to be removable.
            list measurements (entry=<Measurement>, key="time", layout="vertical")
        }
    }

    // Systolic and diastolic against time, one point per group.
    div charts (layout="horizontal", margin=0) {
        chart trend (height=320px, width=100%) {
            plot sys (color="#b42c22", label="Systolic", source=$( /blood_pressure/History ), x=$(|x| x/first_at), y=$(|x| x/avg_systolic))
            plot dia (color="#4A90E2", label="Diastolic", source=$( /blood_pressure/History ), x=$(|x| x/first_at), y=$(|x| x/avg_diastolic))
            plot bpm (color="#7ed321", label="Pulse", source=$( /blood_pressure/History ), x=$(|x| x/first_at), y=$(|x| x/avg_pulse))
        }
    }
    // Newest last, like everything else here: both the bot and the buttons append.
    // Newest first. `sort_by` sorts ascending only, so the instant is negated - the same way
    // `tasks/Open` negates its priority. `millis_since_epoch` rather than "how long ago",
    // because a key that follows the clock changes every minute and every entry of it then
    // lands in the delta of every interaction.
    //
    // Presentation only: the file keeps them in the order they happened, so appending stays an
    // append and the backup's line-wise merge sees what it expects.
    list History (entry=<Reading>, key="started", layout="vertical",
               sort_by=$(|x| 0 - millis_since_epoch(x/started))) {
        - {
            - started = "2026-08-09T08:02:00Z"
            list measurements {
                - {
                    - time = "2026-08-09T08:02:00Z"
                    - systolic = 118
                    - diastolic = 78
                    - pulse = 72
                }
                - {
                    - time = "2026-08-09T08:05:00Z"
                    - systolic = 124
                    - diastolic = 82
                    - pulse = 70
                }
            }
        }
        - {
            - started = "2026-08-10T21:14:00Z"
            list measurements {
                - {
                    - time = "2026-08-10T21:14:00Z"
                    - systolic = 116
                    - diastolic = 76
                    - pulse = 66
                    - arrhythmia = true
                }
            }
        }
    }

}
