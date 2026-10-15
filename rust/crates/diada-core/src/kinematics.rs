use crate::expression::{c, i, r};
use crate::geometry::*;
use crate::*;
use diada_compat::constants::*;
const Z: Real48 = Real48::ZERO;

impl Mechanism {
    pub fn calculate_input(&mut self, id: GroupId, step: InputPosition) -> Result<(), CoreError> {
        let mut g = self
            .group(id)
            .ok_or(CoreError::InvalidStructure("input group absent"))?
            .clone();
        let mut l = self
            .link(g.link_a)
            .ok_or(CoreError::MissingLink(g.link_a))?
            .clone();
        let base_id = g
            .pair_x
            .ok_or(CoreError::InvalidStructure("input base pair absent"))?;
        let base = self
            .pair(base_id)
            .ok_or(CoreError::MissingPair(base_id))?
            .state;
        let out_id = g
            .more
            .ok_or(CoreError::InvalidStructure("input output pair absent"))?;
        let mut out = self
            .pair(out_id)
            .ok_or(CoreError::MissingPair(out_id))?
            .state;
        let t = g.input_type;
        let mut position = step.position;
        let mut origin_x = base.x;
        let mut origin_y = base.y;
        let mut radius;
        let mut angle;
        if t == b'0' || t == b'2' {
            radius = r(l.geometry.parameter).abs().real()?;
            let (v, a) = wrap_deg(position)?;
            position = v;
            angle = a;
        } else {
            radius = position;
            angle = base.fi;
        }
        let [mut x, mut y] = if t == b'2' && l.geometry.parameter.is_zero() {
            [origin_x, origin_y]
        } else {
            offset_point(origin_x, origin_y, radius, angle)?
        };
        if t == b'1' || t == b'2' {
            l.geometry.origin_x = x;
            l.geometry.origin_y = y;
        }
        if t == b'1' && !l.geometry.parameter.is_zero() {
            origin_x = x;
            origin_y = y;
            radius = r(l.geometry.parameter).abs().real()?;
            angle = if r(l.geometry.parameter).gt(i(0))? {
                (r(angle) - c(HALF_PI_ROUNDED)).real()?
            } else {
                (r(angle) + c(HALF_PI_ROUNDED)).real()?
            };
            [x, y] = offset_point(origin_x, origin_y, radius, angle)?;
            angle = (r(angle) - c(PI)).real()?;
        }
        l.geometry.angle = normalize_angle(angle)?;
        if t == b'0' || t == b'2' {
            l.geometry.rate = step.rate;
            l.geometry.rate_derivative = step.rate_derivative;
        }
        out.x = x;
        out.y = y;
        if t == b'3' {
            angle = (r(angle) + r(l.geometry.parameter)).real()?;
        }
        if t == b'2' && !l.geometry.parameter.is_zero() {
            angle = if r(l.geometry.parameter).gt(i(0))? {
                (r(angle) - c(HALF_PI_ROUNDED)).real()?
            } else {
                (r(angle) + c(HALF_PI_ROUNDED)).real()?
            };
        }
        if t == b'2' || t == b'3' {
            out.fi = normalize_angle(angle)?;
        }
        let speed;
        let speed_angle;
        let acceleration;
        let accel_angle;
        // In the defined self-output alias, More.Fi is written before the
        // original reads PairX.Fi for translational velocity/acceleration.
        let current_base_fi = if out_id == base_id { out.fi } else { base.fi };
        if t == b'1' || t == b'3' {
            speed_angle = if r(step.rate).gt(i(0))? {
                current_base_fi
            } else {
                (r(current_base_fi) + c(PI)).real()?
            };
            speed = r(step.rate).abs().real()?;
        } else if t == b'2' && l.geometry.parameter.is_zero() {
            speed = Z;
            speed_angle = Z;
        } else {
            speed = (r(step.rate) * r(l.geometry.parameter)).abs().real()?;
            angle = wrap_deg(position)?.1;
            speed_angle = add_pi_sign(angle, step.rate)?;
        }
        if t == b'1' || t == b'3' {
            accel_angle = sub_pi_if_pos(current_base_fi, step.rate_derivative)?;
            acceleration = r(step.rate_derivative).abs().real()?;
        } else if t == b'2' && l.geometry.parameter.is_zero() {
            acceleration = Z;
            accel_angle = Z;
        } else {
            let radial = (r(radius).abs() * r(step.rate) * r(step.rate)).real()?;
            let tangent = (r(radius) * r(step.rate_derivative)).abs().real()?;
            let a = add_pi_sign(angle, step.rate_derivative)?;
            let mut ax = (r(tangent) * r(a).cos()).real()?;
            let mut ay = (r(tangent) * r(a).sin()).real()?;
            let a = (r(angle) + c(PI)).real()?;
            g.values[4] = radial;
            g.values[5] = a;
            ax = (r(ax) + r(radial) * r(a).cos()).real()?;
            ay = (r(ay) + r(radial) * r(a).sin()).real()?;
            acceleration = hypot(ax, ay)?;
            accel_angle = atan2(ax, ay)?;
        }
        out.speed = speed;
        out.speed_angle = normalize_angle(speed_angle)?;
        out.acceleration = acceleration;
        out.acceleration_angle = normalize_angle(accel_angle)?;
        if t == b'2' {
            out.px = step.rate;
            out.py = step.rate_derivative;
        }
        self.links[g.link_a.0] = l;
        self.pairs[out_id.0].state = out;
        self.groups[id.0] = g;
        self.sync_named_pair(out_id)
    }

    pub fn calculate_position(&mut self, inputs: &[InputPosition]) -> Result<(), CoreError> {
        let first = self
            .groups
            .iter()
            .position(|g| g.kind == GroupKind::Input)
            .ok_or(CoreError::InvalidAnalysis("no input group"))?;
        let mut input_index = 0;
        for index in first..self.groups.len() {
            let id = GroupId(index);
            match self.groups[index].kind {
                GroupKind::Input => {
                    let step = inputs
                        .get(input_index)
                        .ok_or(CoreError::InvalidAnalysis("not enough input steps"))?;
                    self.calculate_input(id, *step)?;
                    input_index += 1;
                }
                GroupKind::Dyad => self.calculate_dyad(id)?,
                GroupKind::Attached => self.calculate_attached(id)?,
                GroupKind::Ground => {}
            }
        }
        if input_index != inputs.len() {
            return Err(CoreError::InvalidAnalysis(
                "input step count differs from input group count",
            ));
        }
        Ok(())
    }
    pub fn calculate_attached(&mut self, id: GroupId) -> Result<(), CoreError> {
        let g = self
            .group(id)
            .ok_or(CoreError::InvalidStructure("attached group absent"))?
            .clone();
        let pt_id = g
            .attached_point
            .ok_or(CoreError::InvalidStructure("auxiliary geometry absent"))?;
        let pt = self
            .point(pt_id)
            .ok_or(CoreError::MissingPoint(pt_id))?
            .clone();
        let owner = g
            .link_b
            .ok_or(CoreError::InvalidStructure("attached owner absent"))?;
        let l = self
            .link(owner)
            .ok_or(CoreError::MissingLink(owner))?
            .clone();
        let t = g.input_type;
        let mut slot = 0;
        if t == b'1' || t == b'4' {
            let p = l.pairs[0].ok_or(CoreError::InvalidStructure("owner first pair absent"))?;
            if self.pairs[p.0].kind == PairKind::Prismatic {
                slot = 1;
            }
        }
        if t == b'2' || t == b'5' {
            slot = 1;
        }
        let reference = l.pairs[slot].ok_or(CoreError::InvalidStructure(
            "attached reference pair absent",
        ))?;
        let base = self
            .pair(reference)
            .ok_or(CoreError::MissingPair(reference))?
            .state;
        let mut point_angle = (r(l.geometry.angle) + r(pt.geometry.angle)).real()?;
        let [x, y] = offset_point(
            base.x,
            base.y,
            r(pt.geometry.parameter).abs().real()?,
            point_angle,
        )?;
        let [v, va, a, aa] = polar_kinematics([
            base.x,
            base.y,
            base.speed,
            base.speed_angle,
            base.acceleration,
            base.acceleration_angle,
            l.geometry.rate,
            l.geometry.rate_derivative,
            x,
            y,
        ])?;
        if (t == b'3' || t == b'4') && !pt.geometry.parameter.is_zero() {
            point_angle = add_pi_sign(point_angle, (-r(pt.geometry.parameter)).real()?)?;
        }
        let out = g
            .more
            .ok_or(CoreError::InvalidStructure("attached pair absent"))?;
        let s = &mut self.pairs[out.0].state;
        s.x = x;
        s.y = y;
        s.speed = v;
        s.speed_angle = va;
        s.acceleration = a;
        s.acceleration_angle = aa;
        if t == b'3' || t == b'4' || t == b'5' {
            s.fi = point_angle;
            s.px = l.geometry.rate;
            s.py = l.geometry.rate_derivative;
            self.points[pt_id.0].geometry.origin_x = x;
            self.points[pt_id.0].geometry.origin_y = y;
        }
        self.sync_named_pair(out)
    }
    pub fn calculate_dyad(&mut self, id: GroupId) -> Result<(), CoreError> {
        let g = self
            .group(id)
            .ok_or(CoreError::InvalidStructure("dyad group absent"))?
            .clone();
        let mut w = DyadWork::new(self, g)?;
        let result = match w.g.input_type {
            b'1' => w.revolute(),
            b'2' => w.sliding(),
            b'3' => w.coupled(),
            b'4' => w.cross_guides(),
            b'5' => w.angled_guides(),
            _ => Err(CoreError::InvalidStructure("invalid dyad type")),
        };
        result.map_err(|e| match e {
            CoreError::UndefinedPosition { reason, .. } => CoreError::UndefinedPosition {
                group: Some(id),
                reason,
            },
            CoreError::Unassemblable { .. } => CoreError::Unassemblable { group: Some(id) },
            e => e,
        })?;
        let out =
            w.g.more
                .ok_or(CoreError::InvalidStructure("dyad output absent"))?;
        self.links[w.a.0] = w.la;
        self.links[w.b.0] = w.lb;
        self.pairs[out.0].state = w.output;
        self.groups[id.0] = w.g;
        self.sync_named_pair(out)
    }
}
struct DyadWork {
    g: Group,
    a: LinkId,
    b: LinkId,
    la: Link,
    lb: Link,
    p: PairState,
    q: PairState,
    output: PairState,
    first_kind: PairKind,
}
impl DyadWork {
    fn new(m: &Mechanism, g: Group) -> Result<Self, CoreError> {
        let a = g.link_a;
        let b = g
            .link_b
            .ok_or(CoreError::InvalidStructure("dyad second link absent"))?;
        let la = m.link(a).ok_or(CoreError::MissingLink(a))?.clone();
        let lb = m.link(b).ok_or(CoreError::MissingLink(b))?.clone();
        let p = la.pairs[0].ok_or(CoreError::InvalidStructure("dyad first outer pair absent"))?;
        let q = lb.pairs[0].ok_or(CoreError::InvalidStructure("dyad second outer pair absent"))?;
        let out = g
            .more
            .ok_or(CoreError::InvalidStructure("dyad inner pair absent"))?;
        Ok(Self {
            g,
            a,
            b,
            la,
            lb,
            p: m.pairs[p.0].state,
            q: m.pairs[q.0].state,
            output: m.pairs[out.0].state,
            first_kind: m.pairs[p.0].kind,
        })
    }
    fn select_revolute_first(&mut self) {
        if self.first_kind != PairKind::Revolute {
            std::mem::swap(&mut self.a, &mut self.b);
            std::mem::swap(&mut self.la, &mut self.lb);
            std::mem::swap(&mut self.p, &mut self.q);
        }
    }
    fn set_output(&mut self, x: Real48, y: Real48, v: [Real48; 2], a: [Real48; 2]) {
        self.output.x = x;
        self.output.y = y;
        self.output.speed = v[0];
        self.output.speed_angle = v[1];
        self.output.acceleration = a[0];
        self.output.acceleration_angle = a[1];
    }
    fn set_members(
        &mut self,
        aa: Real48,
        ab: Real48,
        wa: Real48,
        wb: Real48,
        ea: Real48,
        eb: Real48,
    ) {
        self.la.geometry.angle = aa;
        self.la.geometry.rate = wa;
        self.la.geometry.rate_derivative = ea;
        self.lb.geometry.angle = ab;
        self.lb.geometry.rate = wb;
        self.lb.geometry.rate_derivative = eb;
    }
    fn undefined(reason: &'static str) -> CoreError {
        CoreError::UndefinedPosition {
            group: None,
            reason,
        }
    }
    fn impossible() -> CoreError {
        CoreError::Unassemblable { group: None }
    }
    fn revolute(&mut self) -> Result<(), CoreError> {
        let p = self.p;
        let q = self.q;
        let la = self.la.geometry.parameter;
        let lb = self.lb.geometry.parameter;
        let distance = hypot((r(q.x) - r(p.x)).real()?, (r(q.y) - r(p.y)).real()?)?;
        if r(distance).lt(c(EPSILON))? {
            return Err(Self::undefined("coincident outer pairs"));
        }
        let axis = (r(atan2((r(p.x) - r(q.x)).real()?, (r(p.y) - r(q.y)).real()?)?)
            - c(HALF_PI_ROUNDED))
        .real()?;
        if r(distance).ge(r(la) + r(lb))? || r(distance).le((r(la) - r(lb)).abs())? {
            return Err(Self::impossible());
        }
        let along =
            ((r(la).sqr() + r(distance).sqr() - r(lb).sqr()) / r(distance) / i(2)).real()?;
        let local_y = (r(distance) - r(along)).real()?;
        let mut local_x = (r(la) * (i(1) - (r(along) / r(la)).sqr()).abs().sqrt()).real()?;
        if self.g.assembly != b'+' {
            local_x = (-r(local_x)).real()?;
        }
        let [x, y] = transform_point(axis, q.x, q.y, local_x, local_y)?;
        let aa = atan2((r(x) - r(p.x)).real()?, (r(y) - r(p.y)).real()?)?;
        let ab = atan2((r(x) - r(q.x)).real()?, (r(y) - r(q.y)).real()?)?;
        let [mut va, mut vb] = resolve_polar([
            (r(aa) + c(HALF_PI_ROUNDED)).real()?,
            (r(ab) + c(HALF_PI_ROUNDED)).real()?,
            q.speed,
            q.speed_angle,
            (-r(p.speed)).real()?,
            p.speed_angle,
            Z,
            Z,
            Z,
            Z,
        ])?;
        let av = add_pi_sign(aa, va)?;
        let bv = add_pi_sign(ab, vb)?;
        va = r(va).abs().real()?;
        vb = r(vb).abs().real()?;
        let v = sum_polar([p.speed, p.speed_angle, va, av, Z, Z])?;
        let mut wa = (-r(va) / r(la)).real()?;
        if angle_difference_negative(aa, av)? != 0 {
            wa = (-r(wa)).real()?;
        }
        let mut wb = (-r(vb) / r(lb)).real()?;
        if angle_difference_negative(ab, bv)? != 0 {
            wb = (-r(wb)).real()?;
        }
        let ra = (r(wa).sqr() * r(la)).real()?;
        let rb = (r(wb).sqr() * r(lb)).real()?;
        let aapi = (r(aa) + c(PI)).real()?;
        let abpi = (r(ab) + c(PI)).real()?;
        self.g.values[4] = ra;
        self.g.values[8] = rb;
        self.g.values[5] = aapi;
        self.g.values[9] = abpi;
        let [mut ta, mut tb] = resolve_polar([
            av,
            bv,
            q.acceleration,
            q.acceleration_angle,
            (-r(p.acceleration)).real()?,
            p.acceleration_angle,
            rb,
            abpi,
            (-r(ra)).real()?,
            aapi,
        ])?;
        let ae = sub_pi_if_pos(av, ta)?;
        let be = sub_pi_if_pos(bv, tb)?;
        ta = r(ta).abs().real()?;
        tb = r(tb).abs().real()?;
        let acc = sum_polar([p.acceleration, p.acceleration_angle, ra, aapi, ta, ae])?;
        let mut ea = (-r(ta) / r(la)).real()?;
        if angle_difference_negative(aa, ae)? != 0 {
            ea = (-r(ea)).real()?;
        }
        let mut eb = (-r(tb) / r(lb)).real()?;
        if angle_difference_negative(ab, be)? != 0 {
            eb = (-r(eb)).real()?;
        }
        self.set_output(x, y, v, acc);
        self.set_members(aa, ab, wa, wb, ea, eb);
        Ok(())
    }
    fn sliding(&mut self) -> Result<(), CoreError> {
        self.select_revolute_first();
        let p = self.p;
        let mut q = self.q;
        let length = self.la.geometry.parameter;
        let offset = self.lb.geometry.parameter;
        let guide = q.fi;
        let guide_rate = q.px;
        let guide_derivative = q.py;
        let aa = (r(guide) + c(HALF_PI_ROUNDED)).real()?;
        let [mut gx, mut gy] = intersect_moving_lines([q.x, q.y, guide, p.x, p.y, aa])?;
        if !offset.is_zero() {
            [gx, gy] = offset_point(
                gx,
                gy,
                (-r(offset)).real()?,
                (r(guide) + c(HALF_PI_ROUNDED)).real()?,
            )?;
        }
        let mut distance = hypot((r(p.x) - r(gx)).real()?, (r(p.y) - r(gy)).real()?)?;
        if r(length).lt(r(distance))? {
            return Err(Self::impossible());
        }
        distance = (r(length).sqr() - r(distance).sqr()).sqrt().real()?;
        if self.g.assembly != b'+' {
            distance = (-r(distance)).real()?;
        }
        let [x, y] = offset_point(gx, gy, distance, guide)?;
        let aa = atan2((r(x) - r(p.x)).real()?, (r(y) - r(p.y)).real()?)?;
        let ab = if offset.is_zero() {
            self.lb.geometry.origin_x = x;
            self.lb.geometry.origin_y = y;
            guide
        } else {
            let ab = add_pi_sign(guide, offset)?;
            [gx, gy] = offset_point(x, y, r(offset).abs().real()?, ab)?;
            self.lb.geometry.origin_x = gx;
            self.lb.geometry.origin_y = gy;
            ab
        };
        let index_b = if self.first_kind == PairKind::Prismatic {
            1
        } else {
            3
        };
        let index_a = if index_b == 1 { 1 } else { 2 };
        [q.speed, q.speed_angle, q.acceleration, q.acceleration_angle] = polar_kinematics([
            q.x,
            q.y,
            q.speed,
            q.speed_angle,
            q.acceleration,
            q.acceleration_angle,
            guide_rate,
            guide_derivative,
            gx,
            gy,
        ])?;
        q.x = gx;
        q.y = gy;
        let [
            mut speed,
            mut speed_angle,
            mut acceleration,
            mut accel_angle,
        ] = polar_kinematics([
            q.x,
            q.y,
            q.speed,
            q.speed_angle,
            q.acceleration,
            q.acceleration_angle,
            guide_rate,
            guide_derivative,
            x,
            y,
        ])?;
        self.g.values[index_a * 2 - 2] = speed;
        self.g.values[index_a * 2 - 1] = speed_angle;
        let slot = if index_b == 1 { 4 } else { 8 };
        self.g.values[slot] = acceleration;
        self.g.values[slot + 1] = accel_angle;
        let [mut va, mut vb] = resolve_polar([
            (r(aa) + c(HALF_PI_ROUNDED)).real()?,
            guide,
            speed,
            speed_angle,
            (-r(p.speed)).real()?,
            p.speed_angle,
            Z,
            Z,
            Z,
            Z,
        ])?;
        let av = add_pi_sign(aa, va)?;
        let bv = sub_pi_if_pos(guide, vb)?;
        va = r(va).abs().real()?;
        vb = r(vb).abs().real()?;
        let mut wa = (-r(va) / r(length)).real()?;
        if angle_difference_negative(aa, av)? != 0 {
            wa = (-r(wa)).real()?;
        }
        [speed, speed_angle] = sum_polar([p.speed, p.speed_angle, va, av, Z, Z])?;
        let radial = (r(wa).sqr() * r(length)).real()?;
        let aa_pi = (r(aa) + c(PI)).real()?;
        let slot = if index_b == 1 { 8 } else { 4 };
        self.g.values[slot] = radial;
        self.g.values[slot + 1] = aa_pi;
        let coriolis = ((r(guide_rate) * r(vb)).abs() * i(2)).real()?;
        let ca = add_pi_sign(bv, guide_rate)?;
        let slot = if index_b == 1 { 6 } else { 10 };
        self.g.values[slot] = coriolis;
        self.g.values[slot + 1] = ca;
        let [mut ta, mut tb] = resolve_polar([
            av,
            bv,
            acceleration,
            accel_angle,
            (-r(p.acceleration)).real()?,
            p.acceleration_angle,
            coriolis,
            ca,
            (-r(radial)).real()?,
            aa_pi,
        ])?;
        let tangent_angle = sub_pi_if_pos(av, ta)?;
        ta = r(ta).abs().real()?;
        let mut ea = (-r(ta) / r(length)).real()?;
        if angle_difference_negative(aa, tangent_angle)? != 0 {
            ea = (-r(ea)).real()?;
        }
        let tangent_angle = sub_pi_if_pos(bv, tb)?;
        tb = r(tb).abs().real()?;
        [acceleration, accel_angle] =
            sum_polar([acceleration, accel_angle, coriolis, ca, tb, tangent_angle])?;
        self.set_output(x, y, [speed, speed_angle], [acceleration, accel_angle]);
        self.set_members(aa, ab, wa, guide_rate, ea, guide_derivative);
        Ok(())
    }
    fn coupled(&mut self) -> Result<(), CoreError> {
        let p = self.p;
        let q = self.q;
        let oa = self.la.geometry.parameter;
        let ob = self.lb.geometry.parameter;
        let distance = hypot((r(q.x) - r(p.x)).real()?, (r(q.y) - r(p.y)).real()?)?;
        if r(distance).lt(c(EPSILON))? {
            return Err(Self::undefined("coincident outer pairs"));
        }
        let axis = atan2((r(p.x) - r(q.x)).real()?, (r(p.y) - r(q.y)).real()?)?;
        if (r(oa) - r(ob)).abs().gt(r(distance))? {
            return Err(Self::impossible());
        }
        let tilt = dist_ang(((r(oa) - r(ob)) / r(distance)).real()?)?;
        let connection = if self.g.assembly == b'+' {
            (r(axis) + r(tilt)).real()?
        } else {
            (r(axis) + c(PI) - r(tilt)).real()?
        };
        let aa = if oa.is_zero() {
            self.la.geometry.origin_x = p.x;
            self.la.geometry.origin_y = p.y;
            connection
        } else {
            let a = add_pi_sign(connection, oa)?;
            let [x, y] = offset_point(p.x, p.y, r(oa).abs().real()?, a)?;
            self.la.geometry.origin_x = x;
            self.la.geometry.origin_y = y;
            a
        };
        let ab = if ob.is_zero() {
            connection
        } else {
            add_pi_sign(connection, ob)?
        };
        let [x, y] = if ob.is_zero() {
            [q.x, q.y]
        } else {
            offset_point(q.x, q.y, r(ob).abs().real()?, ab)?
        };
        self.lb.geometry.origin_x = x;
        self.lb.geometry.origin_y = y;
        let [mut va, mut vb] = resolve_polar([
            connection,
            (r(axis) + c(HALF_PI_ROUNDED)).real()?,
            q.speed,
            q.speed_angle,
            (-r(p.speed)).real()?,
            p.speed_angle,
            Z,
            Z,
            Z,
            Z,
        ])?;
        let av = sub_pi_if_pos(connection, va)?;
        let bv = add_pi_sign(axis, vb)?;
        va = r(va).abs().real()?;
        vb = r(vb).abs().real()?;
        let [speed, speed_angle] = sum_polar([p.speed, p.speed_angle, va, av, Z, Z])?;
        self.g.values[0] = speed;
        self.g.values[1] = speed_angle;
        let mut rate = (-r(vb) / r(distance)).real()?;
        if angle_difference_negative(axis, bv)? != 0 {
            rate = (-r(rate)).real()?;
        }
        let radial = (r(rate) * r(rate) * r(distance)).real()?;
        let coriolis = (r(rate) * i(2) * r(va)).abs().real()?;
        let ca = add_pi_sign(av, rate)?;
        let axis_pi = (r(axis) + c(PI)).real()?;
        self.g.values[10] = radial;
        self.g.values[6] = coriolis;
        self.g.values[7] = ca;
        self.g.values[11] = axis_pi;
        let [ta, mut tb] = resolve_polar([
            connection,
            bv,
            q.acceleration,
            q.acceleration_angle,
            (-r(coriolis)).real()?,
            ca,
            radial,
            axis_pi,
            (-r(p.acceleration)).real()?,
            p.acceleration_angle,
        ])?;
        let tba = sub_pi_if_pos(bv, tb)?;
        let _ta = r(ta).abs().real()?;
        tb = r(tb).abs().real()?;
        let [acceleration, accel_angle] = sum_polar([
            q.acceleration,
            q.acceleration_angle,
            radial,
            axis_pi,
            tb,
            tba,
        ])?;
        self.g.values[4] = acceleration;
        self.g.values[5] = accel_angle;
        let mut derivative = (-r(tb) / r(distance)).real()?;
        if angle_difference_negative(axis, tba)? != 0 {
            derivative = (-r(derivative)).real()?;
        }
        let [v, va, a, aa0] = polar_kinematics([
            q.x,
            q.y,
            q.speed,
            q.speed_angle,
            q.acceleration,
            q.acceleration_angle,
            rate,
            derivative,
            x,
            y,
        ])?;
        self.set_output(x, y, [v, va], [a, aa0]);
        self.output.fi = connection;
        self.output.px = rate;
        self.output.py = derivative;
        self.set_members(aa, ab, rate, rate, derivative, derivative);
        Ok(())
    }
    fn cross_guides(&mut self) -> Result<(), CoreError> {
        let mut p = self.p;
        let mut q = self.q;
        let oa = self.la.geometry.parameter;
        let ob = self.lb.geometry.parameter;
        let ga = p.fi;
        let gb = q.fi;
        let wa = p.px;
        let wb = q.px;
        let ea = p.py;
        let eb = q.py;
        let axis = r(normalize_angle((r(ga).abs() - r(gb).abs()).real()?)?)
            .abs()
            .real()?;
        if r(axis).le(c(EPSILON))? || (r(axis) - c(PI)).abs().le(c(EPSILON))? {
            return Err(Self::undefined("parallel outer guides"));
        }
        let aa = if oa.is_zero() {
            ga
        } else {
            add_pi_sign(ga, oa)?
        };
        let ab = if ob.is_zero() {
            gb
        } else {
            add_pi_sign(gb, ob)?
        };
        let [mut ax, mut ay] =
            offset_point(p.x, p.y, r(oa).abs().real()?, (r(aa) + c(PI)).real()?)?;
        let [mut bx, mut by] =
            offset_point(q.x, q.y, r(ob).abs().real()?, (r(ab) + c(PI)).real()?)?;
        let [x, y] = intersect_moving_lines([ax, ay, ga, bx, by, gb])?;
        if oa.is_zero() {
            self.la.geometry.origin_x = x;
            self.la.geometry.origin_y = y;
        } else {
            ax = p.x;
            ay = p.y;
            [p.x, p.y] = offset_point(x, y, r(oa).abs().real()?, aa)?;
            self.la.geometry.origin_x = p.x;
            self.la.geometry.origin_y = p.y;
        }
        if ob.is_zero() {
            self.lb.geometry.origin_x = x;
            self.lb.geometry.origin_y = y;
        } else {
            bx = q.x;
            by = q.y;
            [q.x, q.y] = offset_point(x, y, r(ob).abs().real()?, ab)?;
            self.lb.geometry.origin_x = q.x;
            self.lb.geometry.origin_y = q.y;
        }
        [p.speed, p.speed_angle, p.acceleration, p.acceleration_angle] = polar_kinematics([
            ax,
            ay,
            p.speed,
            p.speed_angle,
            p.acceleration,
            p.acceleration_angle,
            wa,
            ea,
            p.x,
            p.y,
        ])?;
        let [sa, saa, aca, acaa] = polar_kinematics([
            p.x,
            p.y,
            p.speed,
            p.speed_angle,
            p.acceleration,
            p.acceleration_angle,
            wa,
            ea,
            x,
            y,
        ])?;
        self.g.values[0] = sa;
        self.g.values[1] = saa;
        self.g.values[4] = aca;
        self.g.values[5] = acaa;
        [q.speed, q.speed_angle, q.acceleration, q.acceleration_angle] = polar_kinematics([
            bx,
            by,
            q.speed,
            q.speed_angle,
            q.acceleration,
            q.acceleration_angle,
            wb,
            eb,
            q.x,
            q.y,
        ])?;
        let [sb, sba, acb, acba] = polar_kinematics([
            q.x,
            q.y,
            q.speed,
            q.speed_angle,
            q.acceleration,
            q.acceleration_angle,
            wb,
            eb,
            x,
            y,
        ])?;
        self.g.values[2] = sb;
        self.g.values[3] = sba;
        self.g.values[8] = acb;
        self.g.values[9] = acba;
        let [mut va, mut vb] = resolve_polar([ga, gb, (-r(sa)).real()?, saa, sb, sba, Z, Z, Z, Z])?;
        let av = sub_pi_if_pos(ga, va)?;
        let bv = sub_pi_if_pos(gb, vb)?;
        va = r(va).abs().real()?;
        vb = r(vb).abs().real()?;
        let v = sum_polar([sa, saa, va, av, Z, Z])?;
        let ca = (r(wa) * i(2) * r(va)).abs().real()?;
        let cb = (r(wb) * i(2) * r(vb)).abs().real()?;
        let caa = add_pi_sign(av, wa)?;
        let cba = add_pi_sign(bv, wb)?;
        self.g.values[6] = ca;
        self.g.values[7] = caa;
        self.g.values[10] = cb;
        self.g.values[11] = cba;
        let [ta, mut tb] = resolve_polar([
            av,
            bv,
            acb,
            acba,
            (-r(aca)).real()?,
            acaa,
            cb,
            cba,
            (-r(ca)).real()?,
            caa,
        ])?;
        let _taa = sub_pi_if_pos(av, ta)?;
        let tba = sub_pi_if_pos(bv, tb)?;
        let _ta = r(ta).abs().real()?;
        tb = r(tb).abs().real()?;
        let acc = sum_polar([acb, acba, cb, cba, tb, tba])?;
        self.set_output(x, y, v, acc);
        self.set_members(aa, ab, wa, wb, ea, eb);
        Ok(())
    }
    fn angled_guides(&mut self) -> Result<(), CoreError> {
        self.select_revolute_first();
        let p = self.p;
        let q = self.q;
        let oa = self.la.geometry.parameter;
        let offset = self.lb.geometry.parameter;
        let guide = q.fi;
        let rate = q.px;
        let derivative = q.py;
        let connection = (r(guide) + r(offset)).real()?;
        let aa = if oa.is_zero() {
            connection
        } else {
            add_pi_sign(connection, oa)?
        };
        let ab = guide;
        let [ax, ay] = offset_point(p.x, p.y, r(oa).abs().real()?, aa)?;
        self.la.geometry.origin_x = ax;
        self.la.geometry.origin_y = ay;
        let [x, y] = intersect_moving_lines([ax, ay, connection, q.x, q.y, guide])?;
        self.lb.geometry.origin_x = x;
        self.lb.geometry.origin_y = y;
        let [
            mut speed,
            mut speed_angle,
            mut acceleration,
            mut accel_angle,
        ] = polar_kinematics([
            q.x,
            q.y,
            q.speed,
            q.speed_angle,
            q.acceleration,
            q.acceleration_angle,
            rate,
            derivative,
            p.x,
            p.y,
        ])?;
        self.g.values[2] = speed;
        self.g.values[3] = speed_angle;
        self.g.values[8] = acceleration;
        self.g.values[9] = accel_angle;
        let [mut va, mut vb] = resolve_polar([
            connection,
            guide,
            speed,
            speed_angle,
            (-r(p.speed)).real()?,
            p.speed_angle,
            Z,
            Z,
            Z,
            Z,
        ])?;
        let av = sub_pi_if_pos(connection, va)?;
        let bv = sub_pi_if_pos(guide, vb)?;
        va = r(va).abs().real()?;
        vb = r(vb).abs().real()?;
        [speed, speed_angle] = sum_polar([speed, speed_angle, vb, bv, Z, Z])?;
        self.g.values[0] = speed;
        self.g.values[1] = speed_angle;
        self.g.values[8] = acceleration;
        self.g.values[9] = accel_angle;
        let ca = (r(va) * i(2) * r(rate)).abs().real()?;
        let cb = (r(vb) * i(2) * r(rate)).abs().real()?;
        let caa = add_pi_sign(av, rate)?;
        let cba = add_pi_sign(bv, rate)?;
        self.g.values[6] = ca;
        self.g.values[7] = caa;
        self.g.values[10] = cb;
        self.g.values[11] = cba;
        let [ta, mut tb] = resolve_polar([
            av,
            bv,
            acceleration,
            accel_angle,
            (-r(p.acceleration)).real()?,
            p.acceleration_angle,
            cb,
            cba,
            (-r(ca)).real()?,
            caa,
        ])?;
        let _taa = sub_pi_if_pos(av, ta)?;
        let tba = sub_pi_if_pos(bv, tb)?;
        let _ta = r(ta).abs().real()?;
        tb = r(tb).abs().real()?;
        [acceleration, accel_angle] = sum_polar([acceleration, accel_angle, cb, cba, tb, tba])?;
        self.g.values[4] = acceleration;
        self.g.values[5] = accel_angle;
        [speed, speed_angle, acceleration, accel_angle] = polar_kinematics([
            p.x,
            p.y,
            speed,
            speed_angle,
            acceleration,
            accel_angle,
            rate,
            derivative,
            x,
            y,
        ])?;
        self.set_output(x, y, [speed, speed_angle], [acceleration, accel_angle]);
        self.output.fi = connection;
        self.output.px = rate;
        self.output.py = derivative;
        self.set_members(aa, ab, rate, rate, derivative, derivative);
        Ok(())
    }
}
