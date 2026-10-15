use diada_compat::{Extended80, Real48, format_extended, format_real, parse_real};

fn bytes(text: &str) -> Vec<u8> {
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).unwrap())
        .collect()
}

#[test]
fn matches_measured_str_extended() {
    let mut failures = Vec::new();
    for row in include_str!("../fixtures/format-extended.tsv")
        .lines()
        .skip(1)
    {
        let f: Vec<_> = row.split('\t').collect();
        let value = Extended80::from_le_bytes(bytes(f[1]).try_into().unwrap());
        let output =
            format_extended(value, f[2].parse().unwrap(), Some(f[3].parse().unwrap())).unwrap();
        let expected = String::from_utf8(bytes(f[4])).unwrap();
        if output != expected {
            failures.push(format!(
                "{} actual {:?}, expected {:?}",
                f[0], output, expected
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{} differences:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn matches_measured_val() {
    let mut count = 0;
    for row in include_str!("../fixtures/parse-real.tsv")
        .lines()
        .skip(1)
        .chain(
            include_str!("../fixtures/parse-real-wide.tsv")
                .lines()
                .skip(1),
        )
    {
        count += 1;
        let f: Vec<_> = row.split('\t').collect();
        let code: usize = f[2].parse().unwrap();
        let result = parse_real(&bytes(f[1]));
        if code == 0 {
            assert_eq!(
                result.unwrap().to_le_bytes().as_slice(),
                bytes(f[3]),
                "{}",
                f[0]
            );
        } else {
            assert_eq!(result.unwrap_err().position, code, "{}", f[0]);
        }
    }
    assert_eq!(count, 730);
}

#[test]
fn matches_measured_val_store_overflow() {
    for row in include_str!("../fixtures/parse-real-overflow.tsv")
        .lines()
        .skip(1)
    {
        let fields: Vec<_> = row.split('\t').collect();
        let error = parse_real(&bytes(fields[0])).unwrap_err();
        assert_eq!(error.position, 0);
        assert_eq!(
            error.numeric_error.unwrap().runtime_code().to_string(),
            fields[1]
        );
    }
}

#[test]
fn matches_measured_str_real() {
    let mut failures = Vec::new();
    let mut count = 0;
    for row in include_str!("../fixtures/format-real.tsv")
        .lines()
        .skip(1)
        .chain(
            include_str!("../fixtures/format-real-wide.tsv")
                .lines()
                .skip(1),
        )
    {
        count += 1;
        let f: Vec<_> = row.split('\t').collect();
        let value = Real48::from_le_bytes(bytes(f[1]).try_into().unwrap());
        let expected = String::from_utf8(bytes(f[4])).unwrap();
        let output =
            format_real(value, f[2].parse().unwrap(), Some(f[3].parse().unwrap())).unwrap();
        if output != expected {
            failures.push(format!(
                "{}: actual {:?}, expected {:?}",
                f[0], output, expected
            ));
        }
    }
    assert_eq!(count, 3119);
    assert!(
        failures.is_empty(),
        "{} differences:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
