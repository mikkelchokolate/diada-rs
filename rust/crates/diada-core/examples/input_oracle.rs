//! Original input solver snapshots; includes defined self-output aliases.
use diada_core::*;
use std::{env, fs};
mod support;
fn real(b: &[u8], at: usize) -> Real48 {
    Real48::from_le_bytes(b[at..at + 6].try_into().unwrap())
}
fn state(v: [Real48; 12]) -> PairState {
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
    ] = v;
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
pub fn verify(bytes: &[u8]) {
    let angles: [Real48; 9] =
        std::array::from_fn(|k| real(support::cases(bytes, 831).next().unwrap().1, 3 + k * 6));
    let integer = |n| Extended80::from_i64(n).store_real().unwrap();
    let mut checked = 0;
    let mut differences = 0;
    for (case, b) in support::cases(bytes, 831) {
        let alias = case % 6;
        if alias == 4 {
            continue;
        }
        let t = b'0' + (case / 2430) as u8;
        let length_no = (case / 54) % 5 + 1;
        let mut m = Mechanism::default();
        let mut l = Link::new(Name::default());
        l.geometry = LinkGeometry {
            parameter: real(b, 450),
            angle: integer(-31),
            rate: integer(-32),
            rate_derivative: integer(-33),
            origin_x: integer(-34),
            origin_y: integer(-35),
            last_value: Real48::ZERO,
        };
        m.links.push(l);
        for j in 1..=5 {
            m.pairs.push(Pair {
                kind: PairKind::Revolute,
                name: Name([b[j * 75 + 1], b[j * 75 + 2]]),
                links: [LinkId(0), LinkId(0)],
                state: state(std::array::from_fn(|k| angles[(k + j) % 9])),
            });
        }
        let mut g = Group::new(GroupKind::Input, LinkId(0));
        g.input_type = t;
        g.pair_x = Some(PairId(1));
        let out = if alias == 1 {
            1
        } else if alias == 3 {
            4
        } else {
            0
        };
        g.more = Some(PairId(out));
        for k in 0..12 {
            g.values[k] = angles[(k + length_no) % 9];
        }
        m.groups.push(g);
        let step = InputPosition {
            position: real(b, 564),
            other_position: real(b, 570),
            rate: real(b, 576),
            rate_derivative: real(b, 582),
        };
        if let Err(e) = m.calculate_input(GroupId(0), step) {
            differences += 1;
            if differences < 8 {
                eprintln!("case {case}: {e}");
            }
            continue;
        }
        checked += 1;
        let mut bad = Vec::new();
        for j in 1..=5 {
            for (k, v) in fields(m.pairs[j - 1].state).into_iter().enumerate() {
                if v != real(b, j * 75 + 3 + k * 6) {
                    bad.push(format!("pair{j}.{k}"));
                }
            }
        }
        let a = m.links[0].geometry;
        for (k, v) in [
            a.parameter,
            a.angle,
            a.rate,
            a.rate_derivative,
            a.origin_x,
            a.origin_y,
            a.last_value,
        ]
        .into_iter()
        .enumerate()
        {
            if v != real(b, 450 + k * 6) {
                bad.push(format!("link.{k}"));
            }
        }
        for (k, v) in m.groups[0].values.into_iter().enumerate() {
            if v != real(b, 492 + k * 6) {
                bad.push(format!("group.{k}"));
            }
        }
        if !bad.is_empty() {
            differences += 1;
            if differences < 8 {
                eprintln!("case {case}: {}", bad.join("; "));
            }
        }
    }
    println!("{checked} original input-link cases, {differences} differing cases");
    assert_eq!(differences, 0, "original differential mismatches");
}

fn main() {
    verify(&fs::read(env::args().nth(1).unwrap()).unwrap());
}
