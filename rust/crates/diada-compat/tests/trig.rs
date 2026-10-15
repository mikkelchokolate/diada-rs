use diada_compat::{Extended80, atan, cos, sin};
fn hex<const N: usize>(s: &str) -> [u8; N] {
    std::array::from_fn(|i| u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).unwrap())
}
#[test]
fn measured_trig() {
    let mut count = 0;
    let mut failures = Vec::new();
    let mut real_diffs = 0;
    for row in include_str!("../fixtures/trig.tsv")
        .lines()
        .skip(1)
        .chain(include_str!("../fixtures/trig-wide.tsv").lines().skip(1))
    {
        count += 1;
        let f: Vec<_> = row.split('\t').collect();
        let input = Extended80::from_le_bytes(hex(f[2]));
        let expected = Extended80::from_le_bytes(hex(f[3]));
        let result = match f[1] {
            "S" => sin(input),
            "C" => cos(input),
            "A" => atan(input),
            _ => unreachable!(),
        }
        .unwrap();
        if result != expected {
            if result.store_real() != expected.store_real() {
                real_diffs += 1;
            }
            if failures.len() < 20 {
                failures.push(format!(
                    "{} input {} actual {:02x?}, expected {}",
                    f[0],
                    f[2],
                    result.to_le_bytes(),
                    f[3]
                ));
            }
        }
    }
    assert_eq!(count, 9081);
    assert!(
        failures.is_empty(),
        "first differences ({real_diffs} differ after Real48 store):\n{}",
        failures.join("\n")
    );
}
