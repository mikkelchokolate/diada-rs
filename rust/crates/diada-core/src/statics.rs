//! U04 force calculation. Source proof is original U04 instructions, with
//! SYSTEM:32AA/32AF correctly identified as Sin/Cos.
use crate::expression::{c, r};
use crate::geometry::*;
use crate::*;
use diada_compat::constants::*;
const Z: Real48 = Real48::ZERO;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PolarLoad {
    pub magnitude: Real48,
    pub angle: Real48,
    pub moment: Real48,
}
impl Default for PolarLoad {
    fn default() -> Self {
        Self {
            magnitude: Z,
            angle: Z,
            moment: Z,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Resultant {
    pub x: Real48,
    pub y: Real48,
    pub moment: Real48,
}
impl Default for Resultant {
    fn default() -> Self {
        Self {
            x: Z,
            y: Z,
            moment: Z,
        }
    }
}
/// U04:0000. Sin/cos products, both displacement differences, and all three
/// accumulator assignments are separate original six-byte stores.
pub fn compose(
    load: PolarLoad,
    at: [Real48; 2],
    about: [Real48; 2],
    acc: &mut Resultant,
) -> Result<(), CoreError> {
    let t1 = (r(load.magnitude) * r(load.angle).sin()).real()?;
    let t2 = (r(load.magnitude) * r(load.angle).cos()).real()?;
    let t3 = (r(at[0]) - r(about[0])).real()?;
    let t4 = (r(at[1]) - r(about[1])).real()?;
    acc.x = (r(acc.x) + r(t2)).real()?;
    acc.y = (r(acc.y) + r(t1)).real()?;
    acc.moment = (r(acc.moment) + r(load.moment) + r(t1) * r(t3) - r(t2) * r(t4)).real()?;
    Ok(())
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AssurReactions {
    pub outer_a: PolarLoad,
    pub outer_b: PolarLoad,
    pub inner: PolarLoad,
}
fn sum(a: [Real48; 2], b: [Real48; 2]) -> Result<[Real48; 2], CoreError> {
    sum_polar([a[0], a[1], b[0], b[1], Z, Z])
}
fn load(v: [Real48; 2], moment: Real48) -> PolarLoad {
    PolarLoad {
        magnitude: v[0],
        angle: v[1],
        moment,
    }
}
fn pi_angle(a: Real48) -> Result<Real48, CoreError> {
    normalize_angle((r(a) + c(PI)).real()?)
}
/// U04:0A24..1E5A. `a` is the selected revolute member in dyads 2 and 5.
/// Resultant moments already include each member's externally applied moment.
#[allow(clippy::too_many_arguments)]
pub fn solve_assur(
    typ: u8,
    a: LinkGeometry,
    b: LinkGeometry,
    pa: PairState,
    pb: PairState,
    inner: PairState,
    acc_a: Resultant,
    acc_b: Resultant,
) -> Result<AssurReactions, CoreError> {
    let ma = acc_a.moment;
    let mb = acc_b.moment;
    let fa = hypot(acc_a.x, acc_a.y)?;
    let aa = atan2(acc_a.x, acc_a.y)?;
    let fb = hypot(acc_b.x, acc_b.y)?;
    let ab = atan2(acc_b.x, acc_b.y)?;
    let result = match typ {
        b'1' => {
            let d1 = (r(ma) / r(a.parameter)).abs().real()?;
            let ang1 = add_pi_sign(a.angle, ma)?;
            let d2 = (r(mb) / r(b.parameter)).abs().real()?;
            let ang2 = add_pi_sign(b.angle, mb)?;
            let [q1, q2] = resolve_polar([a.angle, b.angle, d1, ang1, d2, ang2, fa, aa, fb, ab])?;
            let copy1 = sub_pi_if_pos(a.angle, (-r(q1)).real()?)?;
            let q1 = r(q1).abs().real()?;
            let copy2 = sub_pi_if_pos(b.angle, q2)?;
            let q2 = r(q2).abs().real()?;
            let ra = sum([q1, copy1], [d1, ang1])?;
            let rb = sum([q2, copy2], [d2, ang2])?;
            let mut ri = sum(ra, [fa, aa])?;
            ri[1] = pi_angle(ri[1])?;
            AssurReactions {
                outer_a: load(ra, Z),
                outer_b: load(rb, Z),
                inner: load(ri, Z),
            }
        }
        b'2' => {
            let d1 = (r(ma) / r(a.parameter)).abs().real()?;
            let ang1 = add_pi_sign(a.angle, ma)?;
            let mut rangb = (r(pb.fi) + c(HALF_PI_ROUNDED)).real()?;
            let [q1, mut rb] = resolve_polar([a.angle, rangb, d1, ang1, Z, Z, fa, aa, fb, ab])?;
            let copy1 = sub_pi_if_pos(a.angle, (-r(q1)).real()?)?;
            let q1 = r(q1).abs().real()?;
            rangb = sub_pi_if_pos(rangb, rb)?;
            rb = r(rb).abs().real()?;
            let rmb = (-r(mb)).real()?;
            let ra = sum([q1, copy1], [d1, ang1])?;
            let mut ri = sum(ra, [fa, aa])?;
            ri[1] = pi_angle(ri[1])?;
            AssurReactions {
                outer_a: load(ra, Z),
                outer_b: load([rb, rangb], rmb),
                inner: load(ri, Z),
            }
        }
        b'3' => {
            let distance = hypot((r(pb.x) - r(pa.x)).real()?, (r(pb.y) - r(pa.y)).real()?)?;
            let axis = atan2((r(pa.x) - r(pb.x)).real()?, (r(pa.y) - r(pb.y)).real()?)?;
            let d2 = ((r(ma) + r(mb)) / r(distance)).abs().real()?;
            let ang2 = add_pi_sign(axis, (r(ma) + r(mb)).real()?)?;
            let mut rangi = (r(inner.fi) + c(HALF_PI_ROUNDED)).real()?;
            let [mut ri, q2] = resolve_polar([rangi, axis, d2, ang2, fb, ab, Z, Z, Z, Z])?;
            let copy2 = sub_pi_if_pos(axis, q2)?;
            let q2 = r(q2).abs().real()?;
            rangi = sub_pi_if_pos(rangi, (-r(ri)).real()?)?;
            ri = r(ri).abs().real()?;
            rangi = pi_angle(rangi)?;
            let rmi = (-r(ma)).real()?;
            let rb = sum([q2, copy2], [d2, ang2])?;
            let mut ra = sum_polar([rb[0], rb[1], fa, aa, fb, ab])?;
            ra[1] = pi_angle(ra[1])?;
            AssurReactions {
                outer_a: load(ra, Z),
                outer_b: load(rb, Z),
                inner: load([ri, rangi], rmi),
            }
        }
        b'4' => {
            let mut ranga = (r(pa.fi) + c(HALF_PI_ROUNDED)).real()?;
            let mut rangb = (r(pb.fi) + c(HALF_PI_ROUNDED)).real()?;
            let [mut ra, mut rb] = resolve_polar([ranga, rangb, Z, Z, Z, Z, fa, aa, fb, ab])?;
            ranga = sub_pi_if_pos(ranga, (-r(ra)).real()?)?;
            ra = r(ra).abs().real()?;
            rangb = sub_pi_if_pos(rangb, rb)?;
            rb = r(rb).abs().real()?;
            let rma = (-r(ma)).real()?;
            let rmb = (-r(mb)).real()?;
            let mut ri = sum([ra, ranga], [fa, aa])?;
            ri[1] = pi_angle(ri[1])?;
            AssurReactions {
                outer_a: load([ra, ranga], rma),
                outer_b: load([rb, rangb], rmb),
                inner: load(ri, Z),
            }
        }
        b'5' => {
            let mut rangi = (r(inner.fi) + c(HALF_PI_ROUNDED)).real()?;
            let mut rangb = (r(pb.fi) + c(HALF_PI_ROUNDED)).real()?;
            let [mut ri, mut rb] = resolve_polar([rangi, rangb, Z, Z, Z, Z, Z, Z, fb, ab])?;
            rangi = sub_pi_if_pos(rangi, (-r(ri)).real()?)?;
            rangi = pi_angle(rangi)?;
            ri = r(ri).abs().real()?;
            let rmi = (-r(ma)).real()?;
            rangb = sub_pi_if_pos(rangb, rb)?;
            rb = r(rb).abs().real()?;
            let mut rmb = (-r(mb) - r(ma)).real()?;
            let mut ra = sum([ri, rangi], [fa, aa])?;
            ra[1] = pi_angle(ra[1])?;
            let mut offset_moment = Resultant::default();
            compose(
                load([rb, rangb], Z),
                [inner.x, inner.y],
                [pa.x, pa.y],
                &mut offset_moment,
            )?;
            rmb = (r(rmb) - r(offset_moment.moment)).real()?;
            AssurReactions {
                outer_a: load(ra, Z),
                outer_b: load([rb, rangb], rmb),
                inner: load([ri, rangi], rmi),
            }
        }
        _ => return Err(CoreError::InvalidStructure("invalid force dyad type")),
    };
    Ok(result)
}
