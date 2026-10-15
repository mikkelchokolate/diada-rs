//! Verify the unmodified U00 numeric block against public graph values.
use diada_core::*;
use std::{env, fs};
mod support;
fn real(b: &[u8], at: usize) -> Real48 {
    Real48::from_le_bytes(b[at..at + 6].try_into().unwrap())
}
fn state(b: &[u8], at: usize) -> PairState {
    PairState {
        x: real(b, at),
        y: real(b, at + 6),
        speed: real(b, at + 12),
        speed_angle: real(b, at + 18),
        acceleration: real(b, at + 24),
        acceleration_angle: real(b, at + 30),
        fi: real(b, at + 36),
        px: real(b, at + 42),
        py: real(b, at + 48),
        rx: real(b, at + 54),
        ry: real(b, at + 60),
        rm: real(b, at + 66),
    }
}
pub fn verify(bytes: &[u8]) {
    let mut differences = 0;
    for (case, b) in support::cases(bytes, 225) {
        let mut m = Mechanism {
            ground: Some(LinkId(0)),
            force_analysis: true,
            ..Mechanism::default()
        };
        m.links.push(Link::new(Name([b'0', b' '])));
        let mut link = Link::new(Name([b'1', b' ']));
        link.pairs = [Some(PairId(0)), Some(PairId(1))];
        link.geometry = LinkGeometry {
            parameter: real(b, 3),
            angle: real(b, 9),
            rate: real(b, 15),
            rate_derivative: real(b, 21),
            origin_x: real(b, 27),
            origin_y: real(b, 33),
            last_value: real(b, 39),
        };
        m.links.push(link);
        m.pairs.push(Pair {
            kind: PairKind::Prismatic,
            name: Name([b'B', b' ']),
            links: [LinkId(0), LinkId(1)],
            state: state(b, 45),
        });
        m.pairs.push(Pair {
            kind: PairKind::Revolute,
            name: Name([b'A', b' ']),
            links: [LinkId(1), LinkId(1)],
            state: state(b, 117),
        });
        if b[1] != 0 {
            m.links[0].pairs[0] = Some(PairId(0));
        }
        let object = match b[0] {
            0 => GraphObject::Pair(PairId(1)),
            1 => GraphObject::Link(LinkId(1)),
            2 => GraphObject::Point(PairId(1)),
            _ => GraphObject::Force(PairId(1)),
        };
        let values = m.graph_values(object).unwrap();
        let positions: &[usize] = match values {
            GraphValues::Angular(_) | GraphValues::Linear(_) => &[0, 2, 4],
            GraphValues::Reaction(_) => &[0, 1, 2],
            _ => &[0, 1, 2, 3, 4, 5],
        };
        for (field, (value, index)) in values.values().iter().zip(positions).enumerate() {
            let expected = real(b, 189 + index * 6);
            if *value != expected {
                differences += 1;
                if differences < 15 {
                    eprintln!(
                        "case {case} mode{} linear{} sample{} field{field}: {:?} != {:?}",
                        b[0],
                        b[1],
                        b[2],
                        value.to_le_bytes(),
                        expected.to_le_bytes()
                    );
                }
            }
        }
    }
    println!(
        "{} U00 numeric graph cases; {differences} differing values",
        support::cases(bytes, 225).count()
    );
    assert_eq!(differences, 0, "original differential mismatches");
}

fn main() {
    verify(&fs::read(env::args().nth(1).unwrap()).unwrap());
}
