//! Test-only comparison with original Math2315 binary snapshots.
use diada_core::*;
use std::{env, fs};
mod support;
fn real(bytes: &[u8], at: usize) -> Real48 {
    Real48::from_le_bytes(bytes[at..at + 6].try_into().unwrap())
}
fn state(fields: [Real48; 12]) -> PairState {
    let [
        x,
        y,
        speed,
        speed_angle,
        acceleration,
        acceleration_angle,
        fi,
        px,
        py,
        rx,
        ry,
        rm,
    ] = fields;
    PairState {
        x,
        y,
        speed,
        speed_angle,
        acceleration,
        acceleration_angle,
        fi,
        px,
        py,
        rx,
        ry,
        rm,
    }
}
fn fields(s: PairState) -> [Real48; 12] {
    [
        s.x,
        s.y,
        s.speed,
        s.speed_angle,
        s.acceleration,
        s.acceleration_angle,
        s.fi,
        s.px,
        s.py,
        s.rx,
        s.ry,
        s.rm,
    ]
}
pub fn verify(t: u8, bytes: &[u8]) {
    let mut success = 0;
    let mut rejected = 0;
    let mut mismatches = 0;
    for (case, b) in support::cases(bytes, 880) {
        let alias = (case / 2) % 4;
        if alias == 1 || alias == 2 {
            continue;
        }
        let assembly = (case / 24) % 2;
        let angle: [Real48; 6] = std::array::from_fn(|i| real(b, 3 + i * 6));
        let mut m = Mechanism::default();
        for n in 1..=2 {
            m.links.push(Link::new(Name([b'L', b'0' + n])));
        }
        for index in 0..6 {
            let initial = std::array::from_fn(|k| angle[(k + index) % 6]);
            let s = if index == 1 || index == 2 {
                state(std::array::from_fn(|k| real(b, index * 75 + 3 + k * 6)))
            } else {
                state(initial)
            };
            m.pairs.push(Pair {
                kind: PairKind::from_code(b[index * 75]).unwrap(),
                name: Name([b[index * 75 + 1], b[index * 75 + 2]]),
                links: [LinkId(0), LinkId(1)],
                state: s,
            });
        }
        for index in 0..2 {
            let at = 450 + index * 42;
            let l = &mut m.links[index];
            l.pairs = [Some(PairId(index + 1)), Some(PairId(3))];
            let n = |v| Extended80::from_i64(v).store_real().unwrap();
            l.geometry = LinkGeometry {
                parameter: real(b, at),
                angle: n(-123),
                rate: n(77),
                rate_derivative: n(88),
                origin_x: n(99),
                origin_y: n(-99),
                last_value: n(42),
            };
        }
        let mut g = Group::new(GroupKind::Dyad, LinkId(if alias == 3 { 1 } else { 0 }));
        g.link_b = Some(LinkId(if alias == 3 { 0 } else { 1 }));
        g.assembly = if assembly == 1 { b'+' } else { b'-' };
        g.input_type = t;
        g.pair_x = Some(PairId(1));
        g.pair_y = Some(PairId(2));
        g.more = Some(PairId(3));
        for k in 0..12 {
            g.values[k] = angle[k % 6];
        }
        m.groups.push(g);
        let result = m.calculate_dyad(GroupId(0));
        let expects_error = b[876] != 0;
        if expects_error {
            if result.is_ok() {
                mismatches += 1;
                if mismatches < 8 {
                    eprintln!("case {case}: expected failure");
                }
            } else {
                rejected += 1;
            }
            continue;
        }
        if let Err(e) = result {
            mismatches += 1;
            if mismatches < 8 {
                eprintln!("case {case}: {e}");
            }
            continue;
        }
        success += 1;
        let mut different = Vec::new();
        for index in 1..6 {
            for (k, value) in fields(m.pairs[index].state).into_iter().enumerate() {
                let expected = real(b, index * 75 + 3 + k * 6);
                if value != expected {
                    different.push(format!(
                        "pair{index}.{k}: {:?} != {:?}",
                        value.to_le_bytes(),
                        expected.to_le_bytes()
                    ));
                }
            }
        }
        for index in 0..2 {
            let a = m.links[index].geometry;
            let values = [
                a.parameter,
                a.angle,
                a.rate,
                a.rate_derivative,
                a.origin_x,
                a.origin_y,
                a.last_value,
            ];
            for (k, value) in values.into_iter().enumerate() {
                if value != real(b, 450 + index * 42 + k * 6) {
                    different.push(format!(
                        "link{index}.{k}: {:?} != {:?}",
                        value.to_le_bytes(),
                        real(b, 450 + index * 42 + k * 6).to_le_bytes()
                    ));
                }
            }
        }
        for k in 0..12 {
            if m.groups[0].values[k] != real(b, 534 + k * 6) {
                different.push(format!("group.{k}"));
            }
        }
        if !different.is_empty() {
            mismatches += 1;
            if mismatches < 8 {
                eprintln!("case {case}: {}", different.join("; "));
            }
        }
    }
    println!(
        "type {}: {success} valid cases, {rejected} explicit rejections, {mismatches} mismatching cases",
        char::from(t)
    );
    assert_eq!(mismatches, 0, "original differential mismatches");
}

fn main() {
    let args = env::args().collect::<Vec<_>>();
    verify(args[1].as_bytes()[0], &fs::read(&args[2]).unwrap());
}
