use diada_compat::Extended80;

fn hex<const N: usize>(text: &str) -> [u8; N] {
    std::array::from_fn(|index| u8::from_str_radix(&text[index * 2..index * 2 + 2], 16).unwrap())
}

#[test]
fn matches_measured_auto_extended_operations() {
    let mut count = 0;
    for row in include_str!("../fixtures/extended-ops.tsv").lines().skip(1) {
        let fields: Vec<_> = row.split('\t').collect();
        let a = Extended80::from_le_bytes(hex(fields[2]));
        let b = Extended80::from_le_bytes(hex(fields[3]));
        let result = match fields[1] {
            "add" => a.checked_add(b),
            "sub" => a.checked_sub(b),
            "mul" => a.checked_mul(b),
            "div" => a.checked_div(b),
            "sqrt" => a.checked_sqrt(),
            "compare" => a.compare(b).map(|order| {
                Extended80::from_i64(match order {
                    std::cmp::Ordering::Less => -1,
                    std::cmp::Ordering::Equal => 0,
                    std::cmp::Ordering::Greater => 1,
                })
            }),
            other => panic!("unknown operation {other}"),
        };
        assert_eq!(
            result.unwrap().to_le_bytes(),
            hex::<10>(fields[4]),
            "{} {}",
            fields[0],
            fields[1]
        );
        count += 1;
    }
    assert_eq!(count, 6801);
}
