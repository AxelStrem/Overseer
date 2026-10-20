//! How a number is written: read the same way in a document's values and in its formulas, and
//! written back in a form that reads as the same number.

/// A number at the start of `input`: how many bytes it takes, and whether it is a float.
///
/// `12`, `-3` are whole numbers; a point or an exponent makes a float - `1.5`, `1.`, `.5`,
/// `2e3`, `1.5E-7` - and so do `inf`, `-inf` and `nan`, as whole words, so that `info` or
/// `nancy` stay names.
pub fn number_at_start(input: &str) -> Option<(usize, bool)> {
    let bytes = input.as_bytes();
    let mut i = 0;
    if bytes.first() == Some(&b'-') {
        i += 1;
    }
    for word in ["inf", "nan"] {
        if input[i..].starts_with(word) && !continues_a_word(bytes.get(i + word.len())) {
            if word == "nan" && i > 0 {
                return None;
            }
            return Some((i + word.len(), true));
        }
    }
    let digits = |from: usize| bytes[from..].iter().take_while(|b| b.is_ascii_digit()).count();
    let whole = digits(i);
    i += whole;
    let mut is_float = false;
    if bytes.get(i) == Some(&b'.') {
        let fraction = digits(i + 1);
        if whole == 0 && fraction == 0 {
            return None;
        }
        is_float = true;
        i += 1 + fraction;
    } else if whole == 0 {
        return None;
    }
    if matches!(bytes.get(i), Some(b'e' | b'E')) {
        let mut j = i + 1;
        if matches!(bytes.get(j), Some(b'+' | b'-')) {
            j += 1;
        }
        let exponent = digits(j);
        if exponent > 0 {
            is_float = true;
            i = j + exponent;
        }
    }
    Some((i, is_float))
}

fn continues_a_word(next: Option<&u8>) -> bool {
    matches!(next, Some(b) if b.is_ascii_alphanumeric() || *b == b'_')
}

/// The float a literal `number_at_start` found stands for.
pub fn float_of(literal: &str) -> Option<f64> {
    match literal {
        "inf" => Some(f64::INFINITY),
        "-inf" => Some(f64::NEG_INFINITY),
        "nan" => Some(f64::NAN),
        _ => literal.parse::<f64>().ok(),
    }
}

/// A float as a document writes it, which reads back as the same float.
///
/// The plain form for everyday sizes; outside them an exponent, since `1e20` written out in full
/// is too long for a whole number and reads back as nothing, and `1e-9` is a run of zeros.
pub fn write_float(f: f64) -> String {
    if f.is_nan() {
        return "nan".to_string();
    }
    if f.is_infinite() {
        return if f > 0.0 { "inf" } else { "-inf" }.to_string();
    }
    let size = f.abs();
    if size != 0.0 && !(1e-5..1e15).contains(&size) {
        format!("{:e}", f)
    } else {
        f.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn found(input: &str) -> Option<(&str, bool)> {
        number_at_start(input).map(|(len, is_float)| (&input[..len], is_float))
    }

    #[test]
    fn the_usual_shapes_are_numbers() {
        assert_eq!(found("12 "), Some(("12", false)));
        assert_eq!(found("-3)"), Some(("-3", false)));
        assert_eq!(found("1.5"), Some(("1.5", true)));
        assert_eq!(found("1."), Some(("1.", true)));
        assert_eq!(found(".5"), Some((".5", true)));
        assert_eq!(found("-.5"), Some(("-.5", true)));
        assert_eq!(found("2e3"), Some(("2e3", true)));
        assert_eq!(found("1.5E-7,"), Some(("1.5E-7", true)));
        assert_eq!(found("inf"), Some(("inf", true)));
        assert_eq!(found("-inf "), Some(("-inf", true)));
        assert_eq!(found("nan}"), Some(("nan", true)));
    }

    #[test]
    fn what_only_starts_like_one_is_not() {
        assert_eq!(found("info"), None);
        assert_eq!(found("nancy"), None);
        assert_eq!(found("inf_count"), None);
        assert_eq!(found("-nan"), None);
        assert_eq!(found("."), None);
        assert_eq!(found("-"), None);
        assert_eq!(found("2em"), Some(("2", false)));
        assert_eq!(found("2e"), Some(("2", false)));
    }

    #[test]
    fn a_written_float_reads_back_the_same() {
        for f in [0.0, 1.0, -2.5, 0.1, 1e-5, 123456.789, 1e14, 1e15, 1e20, -3.25e-9, f64::MAX, f64::MIN_POSITIVE, f64::INFINITY, f64::NEG_INFINITY] {
            let written = write_float(f);
            let (len, _) = number_at_start(&written).expect(&written);
            assert_eq!(len, written.len(), "{written}");
            assert_eq!(float_of(&written), Some(f), "{written}");
        }
        assert!(float_of(&write_float(f64::NAN)).unwrap().is_nan());
        assert_eq!(write_float(1e20), "1e20");
        assert_eq!(write_float(0.25), "0.25");
    }
}
