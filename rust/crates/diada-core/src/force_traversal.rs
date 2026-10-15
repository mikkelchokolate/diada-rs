//! Original U04 reverse group traversal. Cache order and six-byte assignment
//! points are part of the computation, not a generic equilibrium substitute.

use crate::expression::{c, r};
use crate::geometry::{atan2, hypot, normalize_angle};
use crate::statics::{PolarLoad, Resultant, compose, solve_assur};
use crate::*;
use diada_compat::constants::{GRAVITY, PI};

#[derive(Default)]
struct Scratch {
    at: Option<[Real48; 2]>,
    moment: Option<Real48>,
}

fn undefined(group: usize, reason: &'static str) -> CoreError {
    CoreError::UndefinedPosition {
        group: Some(GroupId(group)),
        reason,
    }
}

fn angle_opposite(angle: Real48) -> Result<Real48, CoreError> {
    normalize_angle((r(angle) + c(PI)).real()?)
}

impl Mechanism {
    pub fn calculate_forces(&mut self) -> Result<(), CoreError> {
        let first = self
            .groups
            .iter()
            .position(|group| group.kind == GroupKind::Input)
            .ok_or(CoreError::InvalidAnalysis(
                "force calculation requires input groups",
            ))?;
        let mut cursor = self.groups.len();
        let mut scratch = Scratch::default();
        let mut cached: Vec<PairId> = Vec::new();
        while cursor > first {
            let main = (first..cursor)
                .rev()
                .find(|index| {
                    matches!(self.groups[*index].kind, GroupKind::Input | GroupKind::Dyad)
                })
                .ok_or(CoreError::InvalidAnalysis(
                    "force group sequence has no preceding main group",
                ))?;
            let group = self.groups[main].clone();
            match group.kind {
                GroupKind::Dyad => {
                    let mut a = group.link_a;
                    let mut b = group
                        .link_b
                        .ok_or(CoreError::InvalidStructure("dyad second link absent"))?;
                    if matches!(group.input_type, b'2' | b'5') {
                        let first_pair = self.links[a.0].pairs[0]
                            .ok_or(CoreError::InvalidStructure("dyad primary pair absent"))?;
                        if self.pairs[first_pair.0].kind == PairKind::Prismatic {
                            std::mem::swap(&mut a, &mut b);
                        }
                    }
                    let inner = group
                        .more
                        .ok_or(CoreError::InvalidStructure("dyad inner pair absent"))?;
                    let about = if matches!(group.input_type, b'1' | b'2' | b'4') {
                        let state = self.pairs[inner.0].state;
                        [state.x, state.y]
                    } else {
                        let pair = self.links[a.0].pairs[0]
                            .ok_or(CoreError::InvalidStructure("force reference pair absent"))?;
                        let state = self.pairs[pair.0].state;
                        [state.x, state.y]
                    };
                    let mut aa = Resultant::default();
                    let mut ab = Resultant::default();
                    for trailing in (main + 1..cursor).rev() {
                        let owner = self.groups[trailing]
                            .link_b
                            .ok_or(CoreError::InvalidStructure("attached force owner absent"))?;
                        let accumulator = if self.links[owner.0].name == self.links[a.0].name {
                            &mut aa
                        } else {
                            &mut ab
                        };
                        self.accumulate_force_group(trailing, about, accumulator, &mut scratch)?;
                    }
                    for pair in [group.pair_x, group.pair_y, group.more]
                        .into_iter()
                        .flatten()
                    {
                        if self.pairs[pair.0].kind != PairKind::Revolute {
                            continue;
                        }
                        let name = self.pairs[pair.0].name;
                        if let Some(previous) = cached
                            .iter()
                            .copied()
                            .find(|previous| self.pairs[previous.0].name == name)
                        {
                            self.bind_force_cache(
                                main,
                                pair,
                                previous,
                                a,
                                about,
                                &mut aa,
                                &mut ab,
                                &mut scratch,
                            )?;
                        }
                    }
                    aa.moment = (r(aa.moment) + r(self.links[a.0].geometry.last_value)).real()?;
                    ab.moment = (r(ab.moment) + r(self.links[b.0].geometry.last_value)).real()?;
                    let pa = self.links[a.0].pairs[0]
                        .ok_or(CoreError::InvalidStructure("first force outer pair absent"))?;
                    let pb = self.links[b.0].pairs[0].ok_or(CoreError::InvalidStructure(
                        "second force outer pair absent",
                    ))?;
                    let reactions = solve_assur(
                        group.input_type,
                        self.links[a.0].geometry,
                        self.links[b.0].geometry,
                        self.pairs[pa.0].state,
                        self.pairs[pb.0].state,
                        self.pairs[inner.0].state,
                        aa,
                        ab,
                    )?;
                    if group.input_type == b'5' {
                        // Original SolveAssur5 writes parent TmpV to More.xy
                        // and Rot.Z to zero for its final Compose correction.
                        let state = self.pairs[inner.0].state;
                        scratch.at = Some([state.x, state.y]);
                        scratch.moment = Some(Real48::ZERO);
                    }
                    // Cache nodes refer to pairs; values are assigned afterwards.
                    for pair in [group.pair_x, group.pair_y, group.more]
                        .into_iter()
                        .flatten()
                    {
                        if self.pairs[pair.0].kind == PairKind::Revolute {
                            cached.push(pair);
                        }
                    }
                    self.assign_reaction(inner, a, reactions.inner)?;
                    self.assign_reaction(pa, a, reactions.outer_a)?;
                    self.assign_reaction(pb, b, reactions.outer_b)?;
                }
                GroupKind::Input => {
                    if !matches!(group.input_type, b'0' | b'2') {
                        return Err(undefined(
                            main,
                            "original force traversal does not solve translational inputs",
                        ));
                    }
                    let a = group.link_a;
                    let ground_pair = group
                        .pair_x
                        .ok_or(CoreError::InvalidStructure("input ground pair absent"))?;
                    let state = self.pairs[ground_pair.0].state;
                    let about = [state.x, state.y];
                    let mut accumulator = Resultant::default();
                    for trailing in (main + 1..cursor).rev() {
                        self.accumulate_force_group(
                            trailing,
                            about,
                            &mut accumulator,
                            &mut scratch,
                        )?;
                    }
                    let moving = group
                        .more
                        .ok_or(CoreError::InvalidStructure("input moving pair absent"))?;
                    let pair = self.pairs[moving.0].clone();
                    let at = if group.input_type == b'0' {
                        [pair.state.x, pair.state.y]
                    } else {
                        let other = if self.links[pair.links[0].0].name == self.links[a.0].name {
                            pair.links[1]
                        } else {
                            pair.links[0]
                        };
                        let geometry = self.links[other.0].geometry;
                        [geometry.origin_x, geometry.origin_y]
                    };
                    let mut load = PolarLoad {
                        magnitude: pair.state.rx,
                        angle: pair.state.ry,
                        moment: pair.state.rm,
                    };
                    if self.links[pair.links[0].0].name != self.links[a.0].name {
                        load.angle = angle_opposite(load.angle)?;
                        load.moment = (-r(load.moment)).real()?;
                    }
                    scratch.at = Some(at);
                    scratch.moment = Some(load.moment);
                    compose(load, at, about, &mut accumulator)?;
                    let magnitude = hypot(accumulator.x, accumulator.y)?;
                    let angle = atan2(accumulator.x, accumulator.y)?;
                    let owner_is_first = self.links[self.pairs[ground_pair.0].links[0].0].name
                        == self.links[a.0].name;
                    let pair = &mut self.pairs[ground_pair.0];
                    pair.state.rx = magnitude;
                    // U04:2E79 first stores the opposite, then restores the
                    // resultant angle only when the first owner differs.
                    pair.state.ry = if owner_is_first {
                        angle_opposite(angle)?
                    } else {
                        angle
                    };
                    pair.state.rm = (-r(accumulator.moment)).real()?;
                }
                _ => unreachable!(),
            }
            cursor = main;
        }
        Ok(())
    }

    fn assign_reaction(
        &mut self,
        id: PairId,
        owner: LinkId,
        mut load: PolarLoad,
    ) -> Result<(), CoreError> {
        let pair = self.pair(id).ok_or(CoreError::MissingPair(id))?;
        if self.links[pair.links[0].0].name != self.links[owner.0].name {
            load.angle = angle_opposite(load.angle)?;
            load.moment = (-r(load.moment)).real()?;
        }
        let state = &mut self.pairs[id.0].state;
        state.rx = load.magnitude;
        state.ry = load.angle;
        state.rm = load.moment;
        Ok(())
    }

    fn accumulate_force_group(
        &mut self,
        index: usize,
        about: [Real48; 2],
        accumulator: &mut Resultant,
        scratch: &mut Scratch,
    ) -> Result<(), CoreError> {
        let group = self.groups[index].clone();
        let owner = group
            .link_b
            .ok_or(CoreError::InvalidStructure("attached force link absent"))?;
        let id = group
            .more
            .ok_or(CoreError::InvalidStructure("attached force pair absent"))?;
        let pair = self.pairs[id.0].clone();
        match pair.kind {
            PairKind::Point => Ok(()),
            PairKind::MassCenter => {
                let mass = self.links[owner.0]
                    .mass
                    .clone()
                    .ok_or(CoreError::InvalidStructure(
                        "mass center has no mass properties",
                    ))?;
                let center = mass
                    .center
                    .ok_or(CoreError::InvalidStructure("mass properties lack center"))?;
                let center_pair = self.points[center.0].pair;
                let at = [
                    self.pairs[center_pair.0].state.x,
                    self.pairs[center_pair.0].state.y,
                ];
                let inertia = PolarLoad {
                    magnitude: (r(mass.mass) * r(pair.state.acceleration)).real()?,
                    angle: angle_opposite(pair.state.acceleration_angle)?,
                    moment: (-r(mass.inertia) * r(self.links[owner.0].geometry.rate_derivative))
                        .real()?,
                };
                let cached = &mut self.pairs[center_pair.0].state;
                cached.rx = inertia.magnitude;
                cached.ry = inertia.angle;
                cached.rm = inertia.moment;
                compose(inertia, at, about, accumulator)?;
                let gravity = PolarLoad {
                    magnitude: (r(mass.mass) * c(GRAVITY)).real()?,
                    angle: Real48::from_le_bytes([0x81, 0x58, 0xe4, 0xd7, 0x0f, 0xc9]),
                    moment: Real48::ZERO,
                };
                compose(gravity, at, about, accumulator)?;
                scratch.at = Some(at);
                scratch.moment = Some(Real48::ZERO);
                Ok(())
            }
            PairKind::ForcePoint => {
                let at = [pair.state.x, pair.state.y];
                let load = PolarLoad {
                    magnitude: pair.state.rx,
                    angle: pair.state.ry,
                    moment: pair.state.rm,
                };
                compose(load, at, about, accumulator)?;
                scratch.at = Some(at);
                scratch.moment = Some(load.moment);
                Ok(())
            }
            PairKind::Revolute | PairKind::Prismatic => {
                if pair.kind == PairKind::Revolute {
                    scratch.at = Some([pair.state.x, pair.state.y]);
                }
                let same = self.links[pair.links[0].0].name == self.links[owner.0].name;
                let angle = if same {
                    pair.state.ry
                } else {
                    angle_opposite(pair.state.ry)?
                };
                if pair.kind == PairKind::Prismatic {
                    let other = if same { pair.links[1] } else { pair.links[0] };
                    let geometry = self.links[other.0].geometry;
                    scratch.at = Some([geometry.origin_x, geometry.origin_y]);
                    scratch.moment = Some(if same {
                        pair.state.rm
                    } else {
                        (-r(pair.state.rm)).real()?
                    });
                }
                let at = scratch.at.ok_or_else(|| {
                    undefined(
                        index,
                        "force application coordinate has no defined original predecessor",
                    )
                })?;
                let moment = scratch.moment.ok_or_else(|| {
                    undefined(
                        index,
                        "original additional revolute reaction reads an undefined moment",
                    )
                })?;
                compose(
                    PolarLoad {
                        magnitude: pair.state.rx,
                        angle,
                        moment,
                    },
                    at,
                    about,
                    accumulator,
                )
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn bind_force_cache(
        &self,
        group: usize,
        current: PairId,
        cached: PairId,
        a: LinkId,
        about: [Real48; 2],
        aa: &mut Resultant,
        ab: &mut Resultant,
        scratch: &mut Scratch,
    ) -> Result<(), CoreError> {
        let current = &self.pairs[current.0];
        let previous = &self.pairs[cached.0];
        let current_first = current.links[0];
        let owner = if previous
            .links
            .iter()
            .any(|id| self.links[id.0].name == self.links[current_first.0].name)
        {
            current_first
        }
        // Original fourth comparison repeats the current first member;
        // after the preceding failures only second-to-cached-first wins.
        else if self.links[previous.links[0].0].name == self.links[current.links[1].0].name {
            current.links[1]
        } else {
            return Ok(());
        };
        let same = self.links[previous.links[0].0].name == self.links[owner.0].name;
        // BindLinks adds Pi without Math0709, unlike stored reaction reversal.
        let angle = if same {
            previous.state.ry
        } else {
            (r(previous.state.ry) + c(PI)).real()?
        };
        scratch.moment = Some(Real48::ZERO);
        let at = scratch.at.ok_or_else(|| {
            undefined(
                group,
                "original cached reaction reuses an undefined force coordinate",
            )
        })?;
        let accumulator = if self.links[owner.0].name == self.links[a.0].name {
            aa
        } else {
            ab
        };
        compose(
            PolarLoad {
                magnitude: previous.state.rx,
                angle,
                moment: Real48::ZERO,
            },
            at,
            about,
            accumulator,
        )
    }
}
