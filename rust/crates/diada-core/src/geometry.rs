//! Original 028D resident geometry. Rounded constants and Real stores are
//! intentional and follow verified `src-pascal/GEOMETRY.PAS`.
use crate::expression::{c, i, r};
use crate::{CoreError, Real48};
use diada_compat::constants::*;
pub fn hypot(x: Real48, y: Real48) -> Result<Real48, CoreError> {
    (r(x).sqr() + r(y).sqr()).sqrt().real()
}
pub fn dist_ang(x: Real48) -> Result<Real48, CoreError> {
    let ratio = (r(x) / (r(x).sqr() - i(1)).abs().sqrt()).real()?;
    r(ratio).atan().real()
}
pub fn add_pi_sign(x: Real48, y: Real48) -> Result<Real48, CoreError> {
    if r(y).ge(i(0))? {
        (r(x) + c(HALF_PI_ROUNDED)).real()
    } else {
        (r(x) - c(HALF_PI_ROUNDED)).real()
    }
}
pub fn sub_pi_if_pos(x: Real48, y: Real48) -> Result<Real48, CoreError> {
    if r(y).ge(i(0))? {
        Ok(x)
    } else {
        (r(x) - c(PI)).real()
    }
}
/// Historical argument order is x,y, including the x=0,y=0 half-turn rule.
pub fn atan2(x: Real48, y: Real48) -> Result<Real48, CoreError> {
    if x.is_zero() {
        if r(y).ge(i(0))? {
            c(HALF_PI_ROUNDED).real()
        } else {
            (-c(HALF_PI_ROUNDED)).real()
        }
    } else {
        let angle = (r(y) / r(x)).atan().real()?;
        if r(x).ge(i(0))? {
            Ok(angle)
        } else {
            (r(angle) + c(PI)).real()
        }
    }
}
pub fn sum_polar(v: [Real48; 6]) -> Result<[Real48; 2], CoreError> {
    let [a, b, d, e, f, g] = v;
    let mut x = (r(a) * r(b).cos() + r(d) * r(e).cos()).real()?;
    let mut y = (r(a) * r(b).sin() + r(d) * r(e).sin()).real()?;
    if !f.is_zero() {
        x = (r(x) + r(f) * r(g).cos()).real()?;
        y = (r(y) + r(f) * r(g).sin()).real()?;
    }
    Ok([hypot(x, y)?, atan2(x, y)?])
}
/// Return both the normalized argument and the conversion result.
pub fn wrap_deg(mut v: Real48) -> Result<(Real48, Real48), CoreError> {
    while r(v).gt(i(180))? {
        let next = (r(v) - i(360)).real()?;
        if next == v {
            return Err(CoreError::InvalidAnalysis(
                "angle normalization no longer progresses",
            ));
        }
        v = next;
    }
    while r(v).lt(i(-180))? {
        let next = (r(v) + i(360)).real()?;
        if next == v {
            return Err(CoreError::InvalidAnalysis(
                "angle normalization no longer progresses",
            ));
        }
        v = next;
    }
    Ok((v, (r(v) * c(DEG_TO_RAD)).real()?))
}
pub fn wrap_rad(mut v: Real48) -> Result<(Real48, Real48), CoreError> {
    while r(v).gt(c(PI))? {
        let next = (r(v) - i(2) * c(PI)).real()?;
        if next == v {
            return Err(CoreError::InvalidAnalysis(
                "angle normalization no longer progresses",
            ));
        }
        v = next;
    }
    while r(v).lt(-c(PI))? {
        let next = (r(v) + i(2) * c(PI)).real()?;
        if next == v {
            return Err(CoreError::InvalidAnalysis(
                "angle normalization no longer progresses",
            ));
        }
        v = next;
    }
    Ok((v, (r(v) * c(RAD_TO_DEG)).real()?))
}
pub fn normalize_angle(mut angle: Real48) -> Result<Real48, CoreError> {
    if r(angle).abs().gt(c(PI))? {
        while r(angle).lt(i(0))? {
            let next = (r(angle) + c(TAU_ROUNDED)).real()?;
            if next == angle {
                return Err(CoreError::InvalidAnalysis(
                    "angle normalization no longer progresses",
                ));
            }
            angle = next;
        }
        while r(angle).gt(c(PI))? {
            let next = (r(angle) - c(TAU_ROUNDED)).real()?;
            if next == angle {
                return Err(CoreError::InvalidAnalysis(
                    "angle normalization no longer progresses",
                ));
            }
            angle = next;
        }
    }
    Ok(angle)
}
pub fn polar_kinematics(v: [Real48; 10]) -> Result<[Real48; 4], CoreError> {
    let [x2, y2, h, g, f, e, d, c0, x1, y1] = v;
    let dx = (r(x1) - r(x2)).real()?;
    let dy = (r(y1) - r(y2)).real()?;
    let length = hypot(dx, dy)?;
    let direction = atan2(dx, dy)?;
    let tangent = (r(d) * r(length)).abs().real()?;
    let speed_direction = add_pi_sign(direction, d)?;
    let [speed, speed_angle] =
        sum_polar([h, g, tangent, speed_direction, Real48::ZERO, Real48::ZERO])?;
    let centripetal = (r(tangent) * r(d)).abs().real()?;
    let opposite = (r(direction) + c(PI)).real()?;
    let tangential = (r(c0) * r(length)).abs().real()?;
    let accel_direction = add_pi_sign(direction, c0)?;
    let [acceleration, accel_angle] =
        sum_polar([f, e, centripetal, opposite, tangential, accel_direction])?;
    Ok([speed, speed_angle, acceleration, accel_angle])
}
pub fn offset_point(
    x: Real48,
    y: Real48,
    radius: Real48,
    angle: Real48,
) -> Result<[Real48; 2], CoreError> {
    Ok([
        (r(x) + r(radius) * r(angle).cos()).real()?,
        (r(y) + r(radius) * r(angle).sin()).real()?,
    ])
}
pub fn intersect_moving_lines(v: [Real48; 6]) -> Result<[Real48; 2], CoreError> {
    let [f, e, d, b0, b, a] = v;
    let determinant = (r(d) - r(a)).sin().real()?;
    let sd = r(d).sin().real()?;
    let sa = r(a).sin().real()?;
    let cd = r(d).cos().real()?;
    let ca = r(a).cos().real()?;
    let x =
        ((r(b) - r(e)) * r(cd) * r(ca) - r(b0) * r(cd) * r(sa) + r(f) * r(sd) * r(ca)).real()?;
    let y =
        (-(r(b0) - r(f)) * r(sd) * r(sa) + r(b) * r(ca) * r(sd) - r(e) * r(sa) * r(cd)).real()?;
    Ok([
        (r(x) / r(determinant)).real()?,
        (r(y) / r(determinant)).real()?,
    ])
}
pub fn angle_difference_negative(a: Real48, b: Real48) -> Result<u8, CoreError> {
    let difference = normalize_angle((r(a) - r(b)).real()?)?;
    Ok(u8::from(r(difference).lt(i(0))?))
}
/// 028D:0828 equation resolver. Undefined original output is an explicit error.
pub fn resolve_polar(v: [Real48; 10]) -> Result<[Real48; 2], CoreError> {
    let [r1, r2, r3, r4, r5, r6, r7, r8, r9, r10] = v;
    let determinant = (r(r1) - r(r2)).sin().real()?;
    if r(determinant).abs().lt(c(EPSILON))? {
        return Err(CoreError::UndefinedPosition {
            group: None,
            reason: "nearly parallel equation directions",
        });
    }
    let mut o2 = (r(r3) * (r(r4) - r(r1)).sin() + r(r5) * (r(r6) - r(r1)).sin()).real()?;
    let mut o1 = (r(r3) * (r(r4) - r(r2)).sin() + r(r5) * (r(r6) - r(r2)).sin()).real()?;
    if !r7.is_zero() {
        o2 = (r(o2) + r(r7) * (r(r8) - r(r1)).sin()).real()?;
        o1 = (r(o1) + r(r7) * (r(r8) - r(r2)).sin()).real()?;
    }
    if !r9.is_zero() {
        o2 = (r(o2) + r(r9) * (r(r10) - r(r1)).sin()).real()?;
        o1 = (r(o1) + r(r9) * (r(r10) - r(r2)).sin()).real()?;
    }
    Ok([
        (r(o1) / r(determinant)).real()?,
        (r(o2) / r(determinant)).real()?,
    ])
}
pub fn transform_point(
    angle: Real48,
    x: Real48,
    y: Real48,
    local_x: Real48,
    local_y: Real48,
) -> Result<[Real48; 2], CoreError> {
    let sine = r(angle).sin().real()?;
    let cosine = r(angle).cos().real()?;
    Ok([
        (r(local_x) * r(cosine) - r(local_y) * r(sine) + r(x)).real()?,
        (r(local_x) * r(sine) + r(local_y) * r(cosine) + r(y)).real()?,
    ])
}
