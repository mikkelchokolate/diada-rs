//! Checked original U04:0A24 inputs and reaction outputs; raw Real comparison.
use diada_core::statics::*;
use diada_core::*;
use std::{env, fs};
mod support;
fn real(b: &[u8], at: usize) -> Real48 {
    Real48::from_le_bytes(b[at..at + 6].try_into().unwrap())
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
fn pair(b: &[u8], at: usize) -> PairState {
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
fn acc(b: &[u8], at: usize) -> Resultant {
    Resultant {
        x: real(b, at),
        y: real(b, at + 6),
        moment: real(b, at + 12),
    }
}
pub fn verify(b: &[u8]) {
    let mut differences = 0;
    let mut success = 0;
    let mut rejected = 0;
    for (case, b) in support::cases(b, 392) {
        let result = solve_assur(
            b'0' + b[0],
            geom(b, 1),
            geom(b, 43),
            pair(b, 85),
            pair(b, 157),
            pair(b, 229),
            acc(b, 301),
            acc(b, 319),
        );
        if b[391] != 0 {
            if result.is_err() {
                rejected += 1;
            } else {
                differences += 1;
                eprintln!("case {case} expected rejection");
            }
            continue;
        }
        let result = match result {
            Ok(v) => v,
            Err(e) => {
                differences += 1;
                eprintln!("case {case} unexpected {e}");
                continue;
            }
        };
        success += 1;
        let values = [
            result.outer_a.magnitude,
            result.outer_a.angle,
            result.outer_a.moment,
            result.outer_b.magnitude,
            result.outer_b.angle,
            result.outer_b.moment,
            result.inner.magnitude,
            result.inner.angle,
            result.inner.moment,
        ];
        let bad = values
            .into_iter()
            .enumerate()
            .filter(|(i, v)| *v != real(b, 337 + i * 6))
            .map(|(i, v)| {
                format!(
                    "{i}: {:?} != {:?}",
                    v.to_le_bytes(),
                    real(b, 337 + i * 6).to_le_bytes()
                )
            })
            .collect::<Vec<_>>();
        if !bad.is_empty() {
            differences += 1;
            if differences < 10 {
                eprintln!("case {case}, type{}: {}", b[0], bad.join("; "));
            }
        }
    }
    println!(
        "{success} defined force cases, {rejected} explicit rejections, {differences} differing cases"
    );
    assert_eq!(differences, 0, "original differential mismatches");
}

fn main() {
    verify(&fs::read(env::args().nth(1).unwrap()).unwrap());
}
