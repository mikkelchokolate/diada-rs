use crate::{
    BalanceMoment, LegacyName, LinkReport, PointReport, PositionReport, ReactionApplication,
    ReactionReport, ReportError, ReportNumber, legacy_link_order,
};
use diada_core::{
    CalculationSnapshot, GroupKind, LinkId, Mechanism, PairId, PairKind, Real48, geometry,
};

fn error(error: impl std::fmt::Display) -> ReportError {
    ReportError::Numeric(error.to_string())
}
fn degrees(angle: Real48) -> Result<(Real48, Real48), ReportError> {
    geometry::wrap_rad(angle).map_err(error)
}
fn projection(magnitude: Real48, angle: Real48, sine: bool) -> Result<Real48, ReportError> {
    let direction = if sine {
        diada_compat::sin(angle.load_extended())
    } else {
        diada_compat::cos(angle.load_extended())
    }
    .map_err(error)?;
    magnitude
        .load_extended()
        .checked_mul(direction)
        .and_then(|value| value.store_real())
        .map_err(error)
}
fn point(model: &Mechanism, id: PairId) -> Result<PointReport, ReportError> {
    let pair = &model.pairs[id.0];
    let state = pair.state;
    let (speed_angle, speed_degrees) = degrees(state.speed_angle)?;
    let (acceleration_angle, acceleration_degrees) = degrees(state.acceleration_angle)?;
    let values = [
        state.x,
        state.y,
        state.speed,
        speed_degrees,
        projection(state.speed, speed_angle, false)?,
        projection(state.speed, speed_angle, true)?,
        state.acceleration,
        acceleration_degrees,
        projection(state.acceleration, acceleration_angle, false)?,
        projection(state.acceleration, acceleration_angle, true)?,
    ]
    .map(ReportNumber::from);
    Ok(PointReport {
        name: LegacyName(pair.name.0),
        values,
    })
}
fn application(
    model: &Mechanism,
    id: PairId,
    owner: LinkId,
    reference: Option<PairId>,
) -> Result<ReactionApplication, ReportError> {
    let pair = &model.pairs[id.0];
    if pair.kind == PairKind::Revolute {
        return Ok(ReactionApplication::Rotation);
    }
    if pair.kind != PairKind::Prismatic {
        return Err(ReportError::UndefinedLegacyPath(
            "a non-kinematic point has no reaction report row".into(),
        ));
    }
    let link = &model.links[owner.0];
    let reference = reference
        .or_else(|| link.pairs.into_iter().flatten().find(|value| *value != id))
        .ok_or_else(|| {
            ReportError::UndefinedLegacyPath(
                "a prismatic reaction has no application reference".into(),
            )
        })?;
    let other = &model.pairs[reference.0];
    Ok(if other.kind == PairKind::Revolute {
        ReactionApplication::PerpendicularFoot {
            rotating_pair: LegacyName(other.name.0),
            guide: LegacyName(pair.name.0),
        }
    } else {
        ReactionApplication::GuideIntersection {
            first: LegacyName(pair.name.0),
            second: LegacyName(other.name.0),
        }
    })
}
fn reaction(
    model: &Mechanism,
    id: PairId,
    ground: bool,
    owner: LinkId,
    reference: Option<PairId>,
) -> Result<ReactionReport, ReportError> {
    let pair = &model.pairs[id.0];
    let state = pair.state;
    let (angle, angle_degrees) = degrees(state.ry)?;
    Ok(ReactionReport {
        name: LegacyName(pair.name.0),
        on_link: LegacyName(model.links[pair.links[0].0].name.0),
        from_link: LegacyName(model.links[pair.links[1].0].name.0),
        on_ground: ground,
        values: [
            state.rx,
            angle_degrees,
            projection(state.rx, angle, false)?,
            projection(state.rx, angle, true)?,
        ]
        .map(ReportNumber::from),
        application: application(model, id, owner, reference)?,
        moment: if pair.kind == PairKind::Prismatic {
            Some(state.rm.into())
        } else {
            None
        },
    })
}
/// Convert calculated source-order snapshots to the original U03 report view.
/// Every derived field uses compatibility arithmetic and the same Real stores.
pub fn position_report(
    snapshot: &CalculationSnapshot,
    forces: bool,
) -> Result<PositionReport, ReportError> {
    let model = &snapshot.mechanism;
    let ground = model
        .ground
        .ok_or_else(|| ReportError::UndefinedLegacyPath("ground is not selected".into()))?;
    let first_input = model
        .groups
        .iter()
        .position(|group| group.kind == GroupKind::Input)
        .ok_or_else(|| ReportError::UndefinedLegacyPath("no leading group exists".into()))?;
    let mut report = PositionReport::default();
    for id in legacy_link_order(model).into_iter().skip(1) {
        let link = &model.links[id.0];
        report.links.push(LinkReport {
            name: LegacyName(link.name.0),
            angle_degrees: degrees(link.geometry.angle)?.1.into(),
            omega: link.geometry.rate.into(),
            eps: link.geometry.rate_derivative.into(),
        });
    }
    for group in &model.groups[first_input..] {
        if let Some(id) = group.more
            && matches!(
                model.pairs[id.0].kind,
                PairKind::Revolute | PairKind::Prismatic | PairKind::Point
            )
        {
            report.points.push(point(model, id)?);
        }
    }
    if !forces {
        return Ok(report);
    }
    let input = &model.groups[first_input];
    if input.input_type == b'0' || input.input_type == b'2' {
        let id = input.pair_x.ok_or_else(|| {
            ReportError::UndefinedLegacyPath("leading group has no ground pair".into())
        })?;
        report.balance_moment = Some(BalanceMoment {
            on_link: LegacyName(model.links[input.link_a.0].name.0),
            value: model.pairs[id.0].state.rm.into(),
        });
    }
    for group in &model.groups[..first_input] {
        for id in [group.pair_x, group.pair_y, group.more]
            .into_iter()
            .flatten()
        {
            let pair = &model.pairs[id.0];
            let owner = if pair.links[0] == ground {
                pair.links[1]
            } else {
                pair.links[0]
            };
            report
                .reactions
                .push(reaction(model, id, true, owner, None)?);
        }
    }
    for group in &model.groups[first_input..] {
        let Some(id) = group.more else {
            continue;
        };
        if !matches!(
            model.pairs[id.0].kind,
            PairKind::Revolute | PairKind::Prismatic
        ) {
            continue;
        }
        let (owner, reference) = if group.kind == GroupKind::Dyad {
            let b = group.link_b.ok_or_else(|| {
                ReportError::UndefinedLegacyPath("dyad second link is absent".into())
            })?;
            let first = model.links[group.link_a.0].pairs[0];
            if first.is_some_and(|value| model.pairs[value.0].kind == PairKind::Revolute) {
                (group.link_a, first)
            } else {
                (b, model.links[b.0].pairs[0])
            }
        } else {
            // U03 searches subsequent outer-pair occurrences to identify the
            // receiving link for an input/extra-pair reaction application.
            let mut found = None;
            for later in &model.groups[first_input..] {
                if later
                    .pair_x
                    .is_some_and(|value| model.pairs[value.0].name == model.pairs[id.0].name)
                {
                    found = Some((later.link_a, later.more));
                }
                if later
                    .pair_y
                    .is_some_and(|value| model.pairs[value.0].name == model.pairs[id.0].name)
                    && let Some(link) = later.link_b
                {
                    found = Some((link, later.more));
                }
            }
            found.unwrap_or((group.link_a, model.links[group.link_a.0].pairs[0]))
        };
        report
            .reactions
            .push(reaction(model, id, false, owner, reference)?);
    }
    Ok(report)
}
pub fn position_reports(
    snapshots: &[CalculationSnapshot],
    forces: bool,
) -> Result<Vec<PositionReport>, ReportError> {
    snapshots
        .iter()
        .map(|snapshot| position_report(snapshot, forces))
        .collect()
}
