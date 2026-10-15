use diada_compat::{Extended80, NumericError, Real48};

fn parse_hex<const N: usize>(text: &str) -> [u8; N] {
    assert_eq!(text.len(), 2 * N);
    std::array::from_fn(|i| u8::from_str_radix(&text[2 * i..2 * i + 2], 16).unwrap())
}

#[test]
fn store_matches_measured_turbo_pascal_corpus() {
    let corpus = include_str!("fixtures/store-real.tsv");
    let mut count = 0;
    for line in corpus.lines().filter(|line| !line.starts_with('#')) {
        let columns: Vec<_> = line.split_whitespace().collect();
        assert_eq!(columns.len(), 3, "invalid fixture row: {line}");
        let id = columns[0];
        let input = Extended80::from_le_bytes(parse_hex(columns[1]));
        let expected = match columns[2] {
            "error:205" => Err(NumericError::Overflow),
            value => Ok(Real48::from_le_bytes(parse_hex(value))),
        };
        assert_eq!(input.store_real(), expected, "original StoreReal case {id}");
        count += 1;
    }
    assert_eq!(count, 20, "fixture must include finite and overflow cases");
}
