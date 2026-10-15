//! Decimal presentation only. Original answers and exact numeric states are
//! retained separately; these strings never format a DI/DIF file.
use diada_compat::Real48;

/// Remove padding and redundant decimal zeros without rounding any digits.
/// Incomplete input and nonnumeric names are left as written.
pub fn compact(text: &str) -> String {
    normalized(text).unwrap_or_else(|| text.trim().to_owned())
}

fn normalized(text: &str) -> Option<String> {
    let text = text.trim();
    let (negative, unsigned) = match text.as_bytes().first()? {
        b'-' => (true, &text[1..]),
        b'+' => (false, &text[1..]),
        _ => (false, text),
    };
    let (mantissa, exponent) = if let Some(index) = unsigned.find(['e', 'E']) {
        let (mantissa, exponent) = unsigned.split_at(index);
        let exponent = &exponent[1..];
        if exponent.is_empty() {
            return None;
        }
        (mantissa, exponent.parse::<i32>().ok()?)
    } else {
        (unsigned, 0)
    };
    let mut digits = String::new();
    let mut integer_digits = None;
    for byte in mantissa.bytes() {
        match byte {
            b'0'..=b'9' => digits.push(char::from(byte)),
            b'.' if integer_digits.is_none() => integer_digits = Some(digits.len()),
            _ => return None,
        }
    }
    if digits.is_empty() {
        return None;
    }
    let Some(first) = digits.find(|ch| ch != '0') else {
        return Some(if negative { "-0" } else { "0" }.into());
    };
    let decimal_exponent = i64::from(exponent)
        + i64::try_from(integer_digits.unwrap_or(digits.len())).ok()?
        - i64::try_from(first).ok()?
        - 1;
    let significant = digits[first..].trim_end_matches('0');
    let mut result = if negative { "-" } else { "" }.to_owned();
    if (-6..=8).contains(&decimal_exponent) {
        let point = decimal_exponent + 1;
        if point <= 0 {
            result.push_str("0.");
            result.push_str(&"0".repeat((-point) as usize));
            result.push_str(significant);
        } else {
            let point = point as usize;
            if point >= significant.len() {
                result.push_str(significant);
                result.push_str(&"0".repeat(point - significant.len()));
            } else {
                result.push_str(&significant[..point]);
                result.push('.');
                result.push_str(&significant[point..]);
            }
        }
    } else {
        result.push_str(&significant[..1]);
        if significant.len() > 1 {
            result.push('.');
            result.push_str(&significant[1..]);
        }
        result.push('E');
        result.push_str(&decimal_exponent.to_string());
    }
    Some(result)
}

/// Prefer a short decimal that the original parser reads back to the same
/// six bytes. No hardware floating point or low-precision Str default is used.
pub fn real(value: Real48) -> String {
    for width in 10..=26 {
        if let Ok(text) = diada_compat::format_real(value, width, None) {
            let text = compact(&text);
            if diada_compat::parse_real(text.as_bytes()).ok() == Some(value) {
                return text;
            }
        }
    }
    diada_compat::format_real(value, 26, None)
        .map(|text| compact(&text))
        .unwrap_or_else(|_| "***".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decimal_spelling_is_compact_without_rounding_or_discarding_significant_digits() {
        for (raw, expected) in [
            (" 0.00000000000000000E+0000", "0"),
            ("-0.00000000000000000E+0000", "-0"),
            (" 1.00000000000000000E+0000", "1"),
            ("+2.50000000000000000E-0001", "0.25"),
            ("00123.45000", "123.45"),
            (".500000", "0.5"),
            ("1.23456789012345678E+0000", "1.23456789012345678"),
            ("1000000000", "1E9"),
            ("0.0000001", "1E-7"),
            ("1E-2147483648", "1E-2147483648"),
            ("1000E2147483647", "1E2147483650"),
        ] {
            assert_eq!(compact(raw), expected, "{raw}");
        }
    }

    #[test]
    fn incomplete_or_invalid_answers_are_not_reinterpreted() {
        for raw in [
            "",
            "-",
            "+",
            ".",
            "1e",
            "1e-",
            "1.2.3",
            "NaN",
            "O",
            "1 2",
            "1E9999999999",
        ] {
            assert_eq!(compact(raw), raw);
        }
    }

    #[test]
    fn ordinary_notation_has_bounded_padding_and_extreme_numbers_have_short_exponents() {
        assert_eq!(compact("1e-6"), "0.000001");
        assert_eq!(compact("1e8"), "100000000");
        assert_eq!(compact("1.2300E+0038"), "1.23E38");
        assert_eq!(compact("1.2300E-0038"), "1.23E-38");
    }

    #[test]
    fn numeric_defaults_round_trip_to_the_same_real48_bytes() {
        for raw in [
            "0",
            "-0",
            "0.1",
            "-0.25",
            "1.23456789012",
            "123456789012",
            "1e-38",
            "1e38",
        ] {
            let value = diada_compat::parse_real(raw.as_bytes()).unwrap();
            assert_eq!(
                diada_compat::parse_real(real(value).as_bytes()).unwrap(),
                value
            );
        }
        for exponent in [1, 2, 64, 128, 254, 255] {
            for fraction in [[0, 0, 0, 0, 0], [1, 0, 0, 0, 0], [255, 255, 255, 255, 127]] {
                for sign in [0, 128] {
                    let value = Real48::from_le_bytes([
                        exponent,
                        fraction[0],
                        fraction[1],
                        fraction[2],
                        fraction[3],
                        fraction[4] | sign,
                    ]);
                    let text = real(value);
                    assert_eq!(
                        diada_compat::parse_real(text.as_bytes()).unwrap(),
                        value,
                        "{text} must preserve {:?}",
                        value.to_le_bytes(),
                    );
                }
            }
        }
        assert_eq!(
            real(diada_compat::parse_real(b"1.23456789012").unwrap()),
            "1.23456789012"
        );
    }
}
