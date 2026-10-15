//! DIADA 3.20 U00 graph values, independent from report presentation.
//! The original cycle has 73 samples, including both 0 and 360 degrees.
use crate::expression::{c, i, r};
use crate::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GraphCategory {
    Pair,
    Link,
    Point,
    Force,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GraphObject {
    Pair(PairId),
    Link(LinkId),
    Point(PairId),
    Force(PairId),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GraphValues {
    /// X, Y, Vx, Vy, ax, ay.
    Cartesian([Real48; 6]),
    /// Normalized angle in radians, angular speed, angular acceleration.
    Angular([Real48; 3]),
    /// Signed displacement, projected speed, projected acceleration.
    Linear([Real48; 3]),
    /// Cartesian reaction projections and reaction moment.
    Reaction([Real48; 3]),
}
impl GraphValues {
    pub fn values(&self) -> &[Real48] {
        match self {
            Self::Cartesian(v) => v,
            Self::Angular(v) | Self::Linear(v) | Self::Reaction(v) => v,
        }
    }
    pub fn labels(&self) -> &'static [&'static str] {
        match self {
            Self::Cartesian(_) => &["X", "Y", "Vx", "Vy", "ax", "ay"],
            Self::Angular(_) => &["Fi", "Omega", "Eps"],
            Self::Linear(_) => &["S", "u", "u'"],
            Self::Reaction(_) => &["Rx", "Ry", "M"],
        }
    }
}

impl Mechanism {
    /// Source linked-list order after original ground/input swaps. Arena IDs
    /// remain stable. The swaps are not a stable remove-and-insert operation.
    pub fn legacy_link_order(&self) -> Vec<LinkId> {
        let mut order: Vec<_> = (0..self.links.len()).map(LinkId).collect();
        for (position, id) in self.ground.iter().chain(&self.inputs).enumerate() {
            if let Some(found) = order.iter().position(|candidate| candidate == id) {
                order.swap(position, found);
            }
        }
        order
    }

    pub fn graph_objects(&self, category: GraphCategory) -> Vec<GraphObject> {
        if category == GraphCategory::Link {
            return self
                .legacy_link_order()
                .into_iter()
                .filter(|id| Some(*id) != self.ground)
                .map(GraphObject::Link)
                .collect();
        }
        self.pairs
            .iter()
            .enumerate()
            .filter_map(|(index, pair)| {
                let id = PairId(index);
                match category {
                    GraphCategory::Pair
                        if pair.kind == PairKind::Revolute
                            && self
                                .ground
                                .is_none_or(|ground| !pair.links.contains(&ground)) =>
                    {
                        Some(GraphObject::Pair(id))
                    }
                    GraphCategory::Point
                        if !matches!(pair.kind, PairKind::Revolute | PairKind::Prismatic) =>
                    {
                        Some(GraphObject::Point(id))
                    }
                    GraphCategory::Force
                        if self.force_analysis
                            && matches!(pair.kind, PairKind::Revolute | PairKind::Prismatic) =>
                    {
                        Some(GraphObject::Force(id))
                    }
                    _ => None,
                }
            })
            .collect()
    }

    pub fn graph_object_name(&self, object: GraphObject) -> Result<Name, CoreError> {
        match object {
            GraphObject::Link(id) => Ok(self.link(id).ok_or(CoreError::MissingLink(id))?.name),
            GraphObject::Pair(id) | GraphObject::Point(id) | GraphObject::Force(id) => {
                Ok(self.pair(id).ok_or(CoreError::MissingPair(id))?.name)
            }
        }
    }

    pub fn graph_values(&self, object: GraphObject) -> Result<GraphValues, CoreError> {
        match object {
            GraphObject::Pair(id) | GraphObject::Point(id) => {
                let p = self.pair(id).ok_or(CoreError::MissingPair(id))?.state;
                Ok(GraphValues::Cartesian([
                    p.x,
                    p.y,
                    (r(p.speed) * r(p.speed_angle).cos()).real()?,
                    (r(p.speed) * r(p.speed_angle).sin()).real()?,
                    (r(p.acceleration) * r(p.acceleration_angle).cos()).real()?,
                    (r(p.acceleration) * r(p.acceleration_angle).sin()).real()?,
                ]))
            }
            GraphObject::Force(id) => {
                if !self.force_analysis {
                    return Err(CoreError::InvalidAnalysis("reactions were not calculated"));
                }
                let p = self.pair(id).ok_or(CoreError::MissingPair(id))?.state;
                Ok(GraphValues::Reaction([
                    (r(p.rx) * r(p.ry).cos()).real()?,
                    (r(p.rx) * r(p.ry).sin()).real()?,
                    p.rm,
                ]))
            }
            GraphObject::Link(id) => {
                let link = self.link(id).ok_or(CoreError::MissingLink(id))?;
                let ground = self.ground.and_then(|id| self.link(id));
                let linear = ground.is_some_and(|ground| {
                    ground
                        .pairs
                        .into_iter()
                        .flatten()
                        .chain(ground.extra_pairs.iter().copied())
                        .filter_map(|id| self.pair(id))
                        .filter(|pair| pair.kind == PairKind::Prismatic)
                        .any(|known| {
                            link.pairs
                                .into_iter()
                                .flatten()
                                .filter_map(|id| self.pair(id))
                                .any(|pair| pair.name == known.name)
                        })
                });
                if !linear {
                    return Ok(GraphValues::Angular([
                        crate::geometry::normalize_angle(link.geometry.angle)?,
                        link.geometry.rate,
                        link.geometry.rate_derivative,
                    ]));
                }
                let first_id = link.pairs[0]
                    .ok_or(CoreError::InvalidStructure("graph link has no first pair"))?;
                let second_id = link.pairs[1]
                    .ok_or(CoreError::InvalidStructure("graph link has no second pair"))?;
                let first = self
                    .pair(first_id)
                    .ok_or(CoreError::MissingPair(first_id))?;
                let second = self
                    .pair(second_id)
                    .ok_or(CoreError::MissingPair(second_id))?;
                let (guide, moving) = if first.kind == PairKind::Prismatic {
                    (first.state, second.state)
                } else {
                    (second.state, first.state)
                };
                // Exact U00 CS:102D/1037 constants, not rounded Pi/4 multiples.
                let quarter = Extended80::from_le_bytes([
                    0x35, 0xc2, 0x68, 0x21, 0xa2, 0xda, 0x0f, 0xc9, 0xfe, 0x3f,
                ]);
                let three_quarters = Extended80::from_le_bytes([
                    0xa8, 0x91, 0x0e, 0x99, 0xf9, 0xe3, 0xcb, 0x96, 0x00, 0x40,
                ]);
                let horizontal =
                    r(guide.fi).abs().le(c(quarter))? || r(guide.fi).abs().ge(c(three_quarters))?;
                let displacement = if horizontal {
                    ((r(link.geometry.origin_x) - r(guide.x)) / r(guide.fi).cos()).real()?
                } else {
                    ((r(link.geometry.origin_y) - r(guide.y)) / r(guide.fi).sin()).real()?
                };
                Ok(GraphValues::Linear([
                    displacement,
                    (r(moving.speed) * (r(moving.speed_angle) - r(link.geometry.angle)).cos())
                        .real()?,
                    (r(moving.acceleration)
                        * (r(moving.acceleration_angle) - r(link.geometry.angle)).cos())
                    .real()?,
                ]))
            }
        }
    }

    /// Historical normalized graph cycle: one input, 0..360 degrees inclusive,
    /// five-degree stored increments, speed 1 and acceleration 0. U00 creates
    /// only one TKinematicStep; a second input reaches an undefined pointer.
    pub fn original_graph_config(&self, mode: AnalysisMode) -> Result<AnalysisConfig, CoreError> {
        if self.inputs.len() != 1 {
            return Err(CoreError::InvalidAnalysis(
                "original graph cycle defines exactly one input",
            ));
        }
        Ok(AnalysisConfig {
            positions: 73,
            inputs: vec![InputSeries {
                link: self.inputs[0],
                start: Real48::ZERO,
                other_start: i(5).real()?,
                rate: i(1).real()?,
                rate_derivative: Real48::ZERO,
                step: i(5).real()?,
            }],
            mode,
        })
    }

    /// Original U01 motion passes. A full turn has 72 positions, starting at
    /// zero. U01 first calculates provisional positions for any input, but its
    /// drawing branch explicitly refuses types 1/3 with a graphic error message
    /// (U01RUN, Kind I and Digit 1/3, CS:1A58/1A76). Only 0/2 reach playback.
    /// Oscillation starts at the first entered bound and travels with
    /// an absolute positive increment; the next pass starts exactly at the
    /// second bound and uses the negative increment. Each pass has 72 frames
    /// and omits its final endpoint. Bounds are retained in entered order.
    /// Rate has magnitude one and keeps the sign of the saved first-input
    /// rate (DS:1C84), including on the reverse oscillation pass.
    pub fn original_motion_configs(
        &self,
        rate_sign: Real48,
        range: Option<[Real48; 2]>,
        mode: AnalysisMode,
    ) -> Result<Vec<AnalysisConfig>, CoreError> {
        if self.inputs.len() != 1 {
            return Err(CoreError::InvalidAnalysis(
                "original motion defines exactly one input",
            ));
        }
        let link = self.inputs[0];
        let input = self
            .groups
            .iter()
            .find(|group| group.kind == GroupKind::Input && group.link_a == link)
            .ok_or(CoreError::InvalidStructure("motion input group absent"))?;
        if !matches!(input.input_type, b'0' | b'2') {
            return Err(CoreError::InvalidAnalysis(
                "original cyclic motion requires a rotating input",
            ));
        }
        let negative = r(rate_sign).lt(i(0))?;
        let rate = if negative {
            i(-1).real()?
        } else {
            i(1).real()?
        };
        let make = |start, step| AnalysisConfig {
            positions: 72,
            inputs: vec![InputSeries {
                link,
                start,
                other_start: step,
                rate,
                rate_derivative: Real48::ZERO,
                step,
            }],
            mode,
        };
        if let Some([first, second]) = range {
            // U01 stores (second-first)/72 before taking Abs at each pass.
            let stored_step = ((r(second) - r(first)) / i(72)).real()?;
            let forward = r(stored_step).abs().real()?;
            let reverse = (-r(stored_step).abs()).real()?;
            Ok(vec![make(first, forward), make(second, reverse)])
        } else {
            Ok(vec![make(
                Real48::ZERO,
                if negative {
                    i(-5).real()?
                } else {
                    i(5).real()?
                },
            )])
        }
    }
}
