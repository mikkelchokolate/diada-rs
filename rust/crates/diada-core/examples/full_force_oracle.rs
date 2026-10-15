//! Entire original U04 calculation, including reverse traversal and input.
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
fn geom(b: &[u8], at: usize) -> LinkGeometry {
    LinkGeometry {
        parameter: real(b, at),
        angle: real(b, at + 6),
        rate: real(b, at + 12),
        rate_derivative: real(b, at + 18),
        origin_x: real(b, at + 24),
        origin_y: real(b, at + 30),
        last_value: real(b, at + 36),
    }
}
fn word(b: &[u8], at: usize) -> usize {
    usize::from(u16::from_le_bytes(b[at..at + 2].try_into().unwrap()))
}
pub fn verify(bytes: &[u8]) {
    let mut good = 0;
    let mut errors = 0;
    for (case, pair) in support::cases(bytes, 3516) {
        let (b, expected) = pair.split_at(1758);
        let mut m = Mechanism::default();
        for index in 0..9 {
            let mut l = Link::new(Name([b'L', b'0' + index as u8]));
            l.geometry = geom(b, 675 + index * 42);
            m.links.push(l);
        }
        let owners = [
            [0, 1],
            [1, 2],
            [0, 3],
            [2, 3],
            [2, 2],
            [2, 2],
            [3, 3],
            [3, 3],
            [1, 1],
        ];
        for j in 0..9 {
            let mut links = owners[j].map(LinkId);
            if (case / 4) % 2 == 1 && j < 4 {
                links.swap(0, 1);
            }
            m.pairs.push(Pair {
                kind: PairKind::from_code(b[j * 75]).unwrap(),
                name: Name([b[j * 75 + 1], b[j * 75 + 2]]),
                links,
                state: state(b, j * 75 + 3),
            });
        }
        m.links[0].pairs = [Some(PairId(0)), Some(PairId(2))];
        m.links[1].pairs = [Some(PairId(0)), Some(PairId(1))];
        m.links[2].pairs = [Some(PairId(1)), Some(PairId(3))];
        m.links[3].pairs = [Some(PairId(2)), Some(PairId(3))];
        for j in 4..9 {
            m.links[j].pairs[0] = Some(PairId(j));
        }
        for j in 1..8 {
            let at = 1077 + j * 85;
            let kind = match b[at] {
                b'I' => GroupKind::Input,
                b'D' => GroupKind::Dyad,
                _ => GroupKind::Attached,
            };
            let original_a = LinkId(word(b, at + 3) - 1);
            let original_b = word(b, at + 5).checked_sub(1).map(LinkId);
            let p = |off| word(b, at + off).checked_sub(1).map(PairId);
            let owner = if kind == GroupKind::Attached {
                original_b.unwrap()
            } else {
                original_a
            };
            let mut g = Group::new(kind, owner);
            g.input_type = b[at + 2];
            g.assembly = b[at + 1];
            g.link_b = original_b;
            g.pair_x = p(7);
            g.pair_y = p(9);
            g.more = p(11);
            g.values = std::array::from_fn(|i| real(b, at + 13 + i * 6));
            if kind == GroupKind::Attached {
                let id = PointId(m.points.len());
                g.attached_point = Some(id);
                m.points.push(AttachedPoint {
                    owner,
                    pair: g.more.unwrap(),
                    source: AttachmentSource::AdditionalPoint,
                    geometry: m.links[original_a.0].geometry,
                });
            }
            m.groups.push(g);
        }
        for (j, link) in [2, 3].into_iter().enumerate() {
            let center = m
                .points
                .iter()
                .position(|p| {
                    p.owner == LinkId(link) && m.pairs[p.pair.0].kind == PairKind::MassCenter
                })
                .map(PointId);
            m.links[link].mass = Some(MassProperties {
                mass: real(b, 1053 + j * 12),
                inertia: real(b, 1059 + j * 12),
                center,
            });
        }
        m.ground = Some(LinkId(0));
        m.inputs = vec![LinkId(1)];
        m.force_analysis = true;
        if let Err(e) = m.calculate_forces() {
            errors += 1;
            eprintln!("case {case}: {e}");
            continue;
        }
        let mut bad = Vec::new();
        for j in 0..9 {
            for (k, v) in [
                m.pairs[j].state.rx,
                m.pairs[j].state.ry,
                m.pairs[j].state.rm,
            ]
            .into_iter()
            .enumerate()
            {
                let expected = real(expected, j * 75 + 3 + 54 + k * 6);
                if v != expected {
                    bad.push(format!(
                        "pair{}.{} {:?} != {:?}",
                        j + 1,
                        k,
                        v.to_le_bytes(),
                        expected.to_le_bytes()
                    ));
                }
            }
        }
        if bad.is_empty() {
            good += 1;
        } else {
            errors += 1;
            if errors < 12 {
                eprintln!("case{case}: {}", bad.join("; "));
            }
        }
    }
    println!("{good} entire U04 cases exact, {errors} differing/error cases");
    assert_eq!(errors, 0, "original differential mismatches");
}

fn main() {
    verify(&fs::read(env::args().nth(1).unwrap()).unwrap());
}
