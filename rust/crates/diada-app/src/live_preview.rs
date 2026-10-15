//! A disposable drawing candidate while the original dialogue is incomplete.
//! Unknown values live only in this copied model and never become answers.
use crate::analysis::InputFields;
use crate::drawing::{MemberPath, ObjectKey, Scene};
use diada_core::*;
use diada_io::{AnswerValue, FieldBinding, FieldPath};
use eframe::egui::Pos2;

pub struct LivePreview {
    pub scene: Scene,
    pub display_model: Mechanism,
    pub provisional: Vec<ObjectKey>,
    pub notice: String,
    pub geometry_ready: bool,
    /// Only the initial graph layout needs the detached question sketches.
    pub schematic: bool,
}

fn has(bindings: &[FieldBinding], path: FieldPath) -> bool {
    bindings.iter().any(|binding| binding.path == path)
}
fn yes(bindings: &[FieldBinding], path: FieldPath) -> bool {
    bindings
        .iter()
        .rev()
        .find(|binding| binding.path == path)
        .is_some_and(|binding| binding.value == AnswerValue::YesNo(true))
}
fn number(value: f64) -> Real48 {
    // This conversion creates drawing placeholders only. Defined geometry is
    // always calculated from its original Real48 values by the original core.
    diada_compat::parse_real(format!("{value:.12e}").as_bytes()).unwrap_or(Real48::ZERO)
}
fn display(value: Real48) -> f64 {
    value.load_extended().to_display_f64()
}
fn parameter_known(model: &Mechanism, bindings: &[FieldBinding], id: LinkId) -> bool {
    match model.classify_link(id) {
        Ok(b'2') => {
            !model.groups.iter().any(|group| {
                (group.kind == GroupKind::Input && group.input_type == b'3'
                    || group.kind == GroupKind::Dyad && group.input_type == b'5')
                    && (group.link_a == id || group.link_b == Some(id))
            }) || has(bindings, FieldPath::InputGuideAngle(id))
        }
        Ok(b'0') => {
            !model.links[id.0].geometry.parameter.is_zero()
                && has(bindings, FieldPath::LinkParameter(id))
        }
        Ok(_) => {
            has(bindings, FieldPath::LinkParameter(id)) || yes(bindings, FieldPath::LinkOnGuide(id))
        }
        Err(_) => false,
    }
}
fn offset_known(model: &Mechanism, bindings: &[FieldBinding], id: LinkId) -> bool {
    model
        .link(id)
        .is_some_and(|link| link.geometry.parameter.is_zero())
        || has(bindings, FieldPath::SignedOffset(id))
}
fn geometry_fields(bindings: &[FieldBinding]) -> bool {
    bindings.iter().any(|binding| {
        matches!(
            binding.path,
            FieldPath::GroundX(_)
                | FieldPath::GroundY(_)
                | FieldPath::GroundAngle(_)
                | FieldPath::LinkParameter(_)
                | FieldPath::LinkOnGuide(_)
                | FieldPath::SignedOffset(_)
                | FieldPath::Assembly(_)
                | FieldPath::InputGuideAngle(_)
                | FieldPath::PointDistance(_)
                | FieldPath::PointAngle(_)
                | FieldPath::PointOnGuide(_)
        )
    })
}

fn initial(model: &Mechanism) -> LivePreview {
    let mut scene = Scene::topology(model);
    if !model.groups.is_empty() {
        // Once group entry starts, these are visibly unknown dimensions too.
        // Keep the table's independent topology drawing unchanged.
        if scene.paths.is_empty() {
            scene.paths = scene
                .members
                .iter()
                .filter_map(|member| {
                    let points: Vec<_> = member
                        .nodes
                        .iter()
                        .filter_map(|index| scene.nodes.get(*index).map(|node| node.position))
                        .collect();
                    (points.len() >= 2).then_some(MemberPath {
                        key: member.key,
                        points,
                        dashed: true,
                    })
                })
                .collect();
        }
        for path in &mut scene.paths {
            path.dashed = true;
        }
    }
    LivePreview {
        scene,
        display_model: model.clone(),
        provisional: (0..model.links.len()).map(ObjectKey::Link).collect(),
        notice: "Структурный предпросмотр · размеры ещё не заданы".into(),
        geometry_ready: false,
        schematic: true,
    }
}

/// Translate the graph layout into a local, disposable attachment. Following
/// the owner's frame keeps a ternary body triangular when its primary endpoints
/// have already moved to a provisional calculated position.
fn attachment_seed(
    model: &Mechanism,
    topology: &Scene,
    point_id: PointId,
    typ: u8,
    scale: f64,
) -> (f64, f64) {
    let point = &model.points[point_id.0];
    let owner = &model.links[point.owner.0];
    let slot = if matches!(typ, b'2' | b'5') {
        1
    } else {
        usize::from(
            matches!(typ, b'1' | b'4')
                && owner.pairs[0].is_some_and(|id| model.pairs[id.0].kind == PairKind::Prismatic),
        )
    };
    let positions = owner.pairs[slot]
        .zip(owner.pairs[1 - slot])
        .and_then(|(reference, other)| {
            Some((
                topology.nodes.get(reference.0)?.position,
                topology.nodes.get(other.0)?.position,
                topology.nodes.get(point.pair.0)?.position,
            ))
        });
    let Some((reference, other, target)) = positions else {
        return (scale * 0.3, std::f64::consts::FRAC_PI_3);
    };
    let local = target - reference;
    let axis = other - reference;
    let relative = f64::from(local.y.atan2(local.x) - axis.y.atan2(axis.x));
    let mut distance = f64::from(local.length()) * scale / 0.2;
    if typ == b'0' && axis.length() > 1.0e-6 {
        // The RR reference is its first primary joint and the owner direction
        // points to the second, exactly as CalculateAttached expects.
        distance =
            f64::from(local.length() / axis.length()) * display(owner.geometry.parameter).abs();
    }
    let angle = if relative.is_finite() && relative.sin().abs() > 0.1 {
        relative
    } else {
        // A future graph may place an auxiliary point along an edge. Unknown
        // geometry must not accidentally imply a confirmed straight body.
        std::f64::consts::FRAC_PI_3
    };
    (distance.max(scale * 0.15), angle)
}

/// Build a full, visibly provisional mechanism from accepted fields. A caller
/// may first answer the current question on a cloned controller; this function
/// never consumes, writes or accepts a journal record.
pub fn build(model: &Mechanism, bindings: &[FieldBinding], inputs: &[InputFields]) -> LivePreview {
    let assigned: Vec<_> = (0..model.links.len())
        .map(|index| {
            let id = LinkId(index);
            model.ground == Some(id)
                || model.groups.iter().any(|group| {
                    matches!(group.kind, GroupKind::Input | GroupKind::Dyad)
                        && (group.link_a == id || group.link_b == Some(id))
                })
        })
        .collect();
    // The first accepted dyad already defines a construction order. Show its
    // provisional construction immediately, rather than changing drawing modes
    // only when the first coordinate is typed after ConfirmGroups. Mechanisms
    // without dyads can start once all moving links belong to input groups.
    let construction_started = model
        .groups
        .iter()
        .any(|group| group.kind == GroupKind::Dyad)
        || assigned.iter().all(|assigned| *assigned);
    if model.groups.is_empty() || !geometry_fields(bindings) && !construction_started {
        return initial(model);
    }
    let topology = Scene::topology(model);
    let mut candidate = model.clone();
    let mut pair_pending = vec![true; model.pairs.len()];
    // A link not assigned to a group has no calculated construction yet. In
    // particular a PP link needs no length, but is still wholly provisional.
    let mut link_pending: Vec<_> = assigned.iter().map(|assigned| !assigned).collect();
    let mut scale = model
        .links
        .iter()
        .enumerate()
        .filter(|(index, _)| has(bindings, FieldPath::LinkParameter(LinkId(*index))))
        .map(|(_, link)| display(link.geometry.parameter).abs())
        .filter(|value| value.is_finite() && *value > 0.0 && *value < 1.0e15)
        .fold(0.2_f64, f64::max);
    for binding in bindings {
        if let FieldPath::GroundX(id) | FieldPath::GroundY(id) = binding.path
            && let Some(pair) = model.pair(id)
        {
            let value = if matches!(binding.path, FieldPath::GroundX(_)) {
                display(pair.state.x).abs()
            } else {
                display(pair.state.y).abs()
            };
            if value.is_finite() && value < 1.0e15 {
                scale = scale.max(value);
            }
        }
    }
    // Seed every future node with the existing named graph layout. Calculated
    // groups replace these positions in order; failed groups retain this layout.
    for (index, pair) in candidate.pairs.iter_mut().enumerate() {
        if let Some(node) = topology.nodes.get(index) {
            pair.state.x = number(f64::from(node.position.x) * scale / 0.2);
            pair.state.y = number(f64::from(node.position.y) * scale / 0.2);
        }
    }
    let ground_pairs: Vec<_> = model
        .ground
        .into_iter()
        .flat_map(|ground| {
            model
                .pairs
                .iter()
                .enumerate()
                .filter(move |(_, pair)| pair.links.contains(&ground))
                .map(|(index, _)| PairId(index))
        })
        .collect();
    let known_ground = |path: fn(PairId) -> FieldPath, id: PairId| {
        model.pair(id).is_some_and(|pair| {
            bindings.iter().any(|binding| {
                if let FieldPath::GroundX(other)
                | FieldPath::GroundY(other)
                | FieldPath::GroundAngle(other) = binding.path
                {
                    binding.path == path(other)
                        && model
                            .pair(other)
                            .is_some_and(|other| other.name == pair.name)
                } else {
                    false
                }
            })
        })
    };
    let mut guide_index = 0;
    for id in ground_pairs {
        let x_known = known_ground(FieldPath::GroundX, id);
        let y_known = known_ground(FieldPath::GroundY, id);
        let angle_known = model.pairs[id.0].kind == PairKind::Revolute
            || known_ground(FieldPath::GroundAngle, id);
        // The binding's pair can be an arena alias; its partially read state
        // precedes the original end-of-pair synchronization.
        for binding in bindings {
            if let FieldPath::GroundX(other)
            | FieldPath::GroundY(other)
            | FieldPath::GroundAngle(other) = binding.path
                && model.pairs[other.0].name == model.pairs[id.0].name
            {
                match binding.path {
                    FieldPath::GroundX(_) => {
                        candidate.pairs[id.0].state.x = model.pairs[other.0].state.x
                    }
                    FieldPath::GroundY(_) => {
                        candidate.pairs[id.0].state.y = model.pairs[other.0].state.y
                    }
                    FieldPath::GroundAngle(_) => {
                        candidate.pairs[id.0].state.fi = model.pairs[other.0].state.fi
                    }
                    _ => {}
                }
            }
        }
        if model.pairs[id.0].kind == PairKind::Prismatic {
            if !angle_known {
                candidate.pairs[id.0].state.fi = number(if guide_index % 2 == 0 {
                    0.0
                } else {
                    std::f64::consts::FRAC_PI_2
                });
            }
            guide_index += 1;
        }
        let pending = !(x_known && y_known && angle_known);
        let name = candidate.pairs[id.0].name;
        let _ = candidate.sync_named_pair(id);
        for (index, pair) in candidate.pairs.iter().enumerate() {
            if pair.name == name {
                pair_pending[index] = pending;
            }
        }
    }
    for (index, pending) in link_pending.iter_mut().enumerate() {
        let id = LinkId(index);
        if candidate.ground == Some(id) {
            continue;
        }
        if !parameter_known(model, bindings, id) {
            *pending = true;
            candidate.links[index].geometry.parameter = if model.classify_link(id) == Ok(b'0') {
                number(scale * 0.45)
            } else if model.classify_link(id) == Ok(b'2') {
                number(std::f64::consts::FRAC_PI_3)
            } else {
                // Until OnGuide is answered, zero would merge two distinct
                // joints. Accepted Yes already made parameter_known true.
                number(scale * 0.2)
            };
        }
    }
    let mut fallback_links = vec![false; model.links.len()];
    let mut failure = None;
    let mut failure_group = None;
    let mut failure_was_provisional = false;
    let mut input_index = 0;
    let mut solved_all = true;
    let first_input = candidate
        .groups
        .iter()
        .position(|group| group.kind == GroupKind::Input)
        .unwrap_or(candidate.groups.len());
    for index in 0..candidate.groups.len() {
        let group = candidate.groups[index].clone();
        if index < first_input || group.kind == GroupKind::Ground {
            continue;
        }
        let mut pending = [group.pair_x, group.pair_y]
            .into_iter()
            .flatten()
            .any(|id| pair_pending.get(id.0).copied().unwrap_or(true));
        let mut group_links = vec![group.link_a];
        if group.kind == GroupKind::Dyad {
            group_links.extend(group.link_b);
        }
        pending |= group_links.iter().any(|id| link_pending[id.0]);
        let offsets: Vec<_> = match (group.kind, group.input_type) {
            (GroupKind::Input, b'1') => vec![group.link_a],
            (GroupKind::Dyad, b'2') => group_links
                .iter()
                .copied()
                .filter(|id| model.classify_link(*id) == Ok(b'1'))
                .collect(),
            (GroupKind::Dyad, b'3' | b'4' | b'5') => group_links
                .iter()
                .copied()
                .filter(|id| model.classify_link(*id) != Ok(b'2'))
                .collect(),
            _ => vec![],
        };
        pending |= offsets
            .into_iter()
            .any(|id| !offset_known(model, bindings, id));
        if group.kind == GroupKind::Dyad {
            let asks_assembly = matches!(group.input_type, b'1' | b'2')
                || group.input_type == b'3'
                    && group_links
                        .iter()
                        .any(|id| !candidate.links[id.0].geometry.parameter.is_zero());
            if asks_assembly && !has(bindings, FieldPath::Assembly(GroupId(index))) {
                pending = true;
                candidate.groups[index].assembly = b'+';
            }
            if group.input_type == b'3' && !asks_assembly {
                candidate.groups[index].assembly = b'+';
            }
            // A missing RR length follows the separation of its currently
            // positioned outer pairs, so the provisional triangle can assemble.
            if let Some(second) = group.link_b {
                let a = group.link_a;
                let b = second;
                let distance = candidate.links[a.0].pairs[0]
                    .zip(candidate.links[b.0].pairs[0])
                    .map(|(p, q)| {
                        let p = candidate.pairs[p.0].state.display_xy();
                        let q = candidate.pairs[q.0].state.display_xy();
                        (p[0] - q[0]).hypot(p[1] - q[1])
                    })
                    .filter(|value| value.is_finite())
                    .unwrap_or(scale)
                    .max(scale * 0.1);
                let missing_a = !parameter_known(model, bindings, a);
                let missing_b = !parameter_known(model, bindings, b);
                for (id, other, missing, other_missing) in
                    [(a, b, missing_a, missing_b), (b, a, missing_b, missing_a)]
                {
                    if missing && model.classify_link(id) == Ok(b'0') {
                        let radius = if group.input_type == b'1' && !other_missing {
                            distance
                                .hypot(display(candidate.links[other.0].geometry.parameter).abs())
                        } else if group.input_type == b'1' {
                            distance * 0.75
                        } else {
                            distance + scale * 0.25
                        };
                        candidate.links[id.0].geometry.parameter = number(radius);
                    }
                }
            }
        }
        if group.kind == GroupKind::Attached
            && let Some(point_id) = group.attached_point
        {
            let point = &candidate.points[point_id.0];
            let distance_known = has(bindings, FieldPath::PointDistance(point_id))
                || yes(bindings, FieldPath::PointOnGuide(point_id));
            let angle_known = has(bindings, FieldPath::PointAngle(point_id))
                || matches!(group.assembly, b'M' | b'F') && point.geometry.parameter.is_zero();
            let owner = point.owner;
            pending = link_pending[owner.0] || !distance_known || !angle_known;
            let (seed_distance, seed_angle) =
                attachment_seed(&candidate, &topology, point_id, group.input_type, scale);
            if !distance_known {
                candidate.points[point_id.0].geometry.parameter =
                    if matches!(group.input_type, b'3' | b'4') {
                        number(scale * 0.2)
                    } else {
                        number(seed_distance)
                    };
            }
            if !angle_known {
                candidate.points[point_id.0].geometry.angle = number(seed_angle);
            }
            group_links = vec![owner];
        }
        let mut next = candidate.clone();
        let result = if !solved_all {
            Err(CoreError::InvalidStructure(
                "earlier preview group is undefined",
            ))
        } else {
            match group.kind {
                GroupKind::Input => {
                    let field = inputs
                        .iter()
                        .find(|field| field.name == candidate.links[group.link_a.0].name.display());
                    let position = field.and_then(|field| field.series().ok());
                    if field.is_some() && position.is_none() {
                        pending = true;
                    }
                    let step = position.map_or(
                        InputPosition {
                            position: Real48::ZERO,
                            other_position: Real48::ZERO,
                            rate: number(1.0),
                            rate_derivative: Real48::ZERO,
                        },
                        |series| InputPosition {
                            position: series.start,
                            other_position: series.other_start,
                            rate: series.rate,
                            rate_derivative: series.rate_derivative,
                        },
                    );
                    input_index += 1;
                    next.calculate_input(GroupId(index), step)
                }
                GroupKind::Dyad => next.calculate_dyad(GroupId(index)),
                GroupKind::Attached => next.calculate_attached(GroupId(index)),
                GroupKind::Ground => Ok(()),
            }
        };
        match result {
            Ok(()) => candidate = next,
            Err(error) => {
                if failure.is_none() {
                    failure = Some(error);
                    failure_group = Some(index);
                    failure_was_provisional = pending;
                }
                solved_all = false;
                pending = true;
                for id in &group_links {
                    fallback_links[id.0] = true;
                }
            }
        }
        if group.kind != GroupKind::Attached {
            for id in &group_links {
                link_pending[id.0] |= pending;
            }
        }
        if let Some(output) = group.more {
            let name = candidate.pairs[output.0].name;
            for (pair_index, pair) in candidate.pairs.iter().enumerate() {
                if pair.name == name {
                    pair_pending[pair_index] = pending;
                }
            }
        }
    }
    let geometry_ready = solved_all
        && input_index == candidate.inputs.len()
        && !link_pending.iter().any(|pending| *pending)
        && !pair_pending.iter().any(|pending| *pending);
    if candidate.pairs.iter().any(|pair| {
        pair.state
            .display_xy()
            .iter()
            .any(|value| !value.is_finite() || value.abs() > 1.0e15)
    }) {
        let mut preview = initial(model);
        preview.notice = "Числа слишком велики для отображения. Показана условная схема.".into();
        return preview;
    }
    let mut scene = Scene::mechanism(&candidate);
    let prefix_scene = failure_group.map(|stop| {
        let mut prefix = candidate.clone();
        prefix.groups.truncate(stop);
        Scene::mechanism(&prefix)
    });
    if let (Some(prefix), Some(stop)) = (&prefix_scene, failure_group) {
        for group in &candidate.groups[..stop] {
            if !matches!(group.kind, GroupKind::Input | GroupKind::Dyad) {
                continue;
            }
            for link in [Some(group.link_a), group.link_b].into_iter().flatten() {
                if !fallback_links[link.0] {
                    let key = ObjectKey::Link(link.0);
                    scene.paths.retain(|path| path.key != key);
                    scene
                        .paths
                        .extend(prefix.paths.iter().filter(|path| path.key == key).cloned());
                }
            }
        }
    }
    // Failed groups have no valid stored foot/angle. Keep their original graph
    // construction, connected to the successfully positioned prefix endpoints.
    for (index, fallback) in fallback_links.iter().enumerate() {
        if !fallback {
            continue;
        }
        let key = ObjectKey::Link(index);
        scene.paths.retain(|path| path.key != key);
        for id in candidate.links[index].pairs.iter().flatten() {
            if candidate.pairs[id.0].kind == PairKind::Prismatic {
                let at = prefix_scene
                    .as_ref()
                    .map(|prefix| prefix.nodes[id.0].position)
                    .unwrap_or(scene.nodes[id.0].position);
                let name = candidate.pairs[id.0].name;
                let known_body =
                    candidate.groups[..failure_group.unwrap_or(0)]
                        .iter()
                        .any(|group| {
                            matches!(group.kind, GroupKind::Input | GroupKind::Dyad)
                                && [group.pair_x, group.pair_y, group.more]
                                    .into_iter()
                                    .flatten()
                                    .any(|pair| candidate.pairs[pair.0].name == name)
                        });
                for (alias, pair) in candidate.pairs.iter().enumerate() {
                    if pair.name == name {
                        scene.nodes[alias].position = at;
                        pair_pending[alias] |= !known_body;
                    }
                }
            } else if pair_pending[id.0] {
                let [x, y] = candidate.pairs[id.0].state.display_xy();
                scene.nodes[id.0].position = Pos2::new(x as f32, y as f32);
            }
        }
        let points: Vec<_> = candidate.links[index]
            .pairs
            .iter()
            .flatten()
            .map(|id| scene.nodes[id.0].position)
            .collect();
        if points.len() >= 2 {
            scene.paths.push(MemberPath {
                key,
                points,
                dashed: true,
            });
        }
    }
    for path in &mut scene.paths {
        if let ObjectKey::Link(id) = path.key {
            path.dashed |= link_pending.get(id).copied().unwrap_or(true);
        }
        // An unknown auxiliary point does not make its already known owner
        // body uncertain. Only its attachment rays inherit that uncertainty.
        path.dashed |= candidate.points.iter().any(|point| {
            pair_pending[point.pair.0]
                && path.key == ObjectKey::Link(point.owner.0)
                && scene.nodes.get(point.pair.0).is_some_and(|node| {
                    path.points.contains(&node.position)
                        && path
                            .points
                            .iter()
                            .any(|position| *position != node.position)
                })
        });
    }
    let mut provisional: Vec<_> = pair_pending
        .iter()
        .enumerate()
        .filter(|(_, pending)| **pending)
        .map(|(index, _)| ObjectKey::Pair(index))
        .collect();
    provisional.extend(
        link_pending
            .iter()
            .enumerate()
            .filter(|(_, pending)| **pending)
            .map(|(index, _)| ObjectKey::Link(index)),
    );
    // The annotation layer reads stored XY; for a provisional P the visible
    // slider foot is only a drawing assumption, so use that same drawing point.
    for (index, pending) in pair_pending.iter().enumerate() {
        if *pending {
            candidate.pairs[index].state.x = number(f64::from(scene.nodes[index].position.x));
            candidate.pairs[index].state.y = number(f64::from(scene.nodes[index].position.y));
        }
    }
    let notice = if failure_was_provisional {
        "Для оставшейся части ещё не хватает размеров или выбора сборки. Она показана условно пунктиром.".into()
    } else if let Some(error) = failure {
        format!(
            "{} Определённая часть сохранена; остальное показано условно пунктиром.",
            crate::analysis::error(&error)
        )
    } else if geometry_ready {
        "Предпросмотр по введённым размерам".into()
    } else {
        "Живой предпросмотр · ещё не заданные размеры и зависящие от них звенья показаны условно пунктиром".into()
    };
    LivePreview {
        scene,
        display_model: candidate,
        provisional,
        notice,
        geometry_ready,
        schematic: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use diada_io::{DialogueController, DialogueState, Journal, LoadedDialogue, TpNumbers};

    const RR: &[u8] =
        include_bytes!("../../../../tests/fixtures/diada-v320-derived/dyad-01/SCENE.DI");
    const RP: &[u8] =
        include_bytes!("../../../../tests/fixtures/diada-v320-derived/dyad-02/SCENE.DI");
    fn loaded(bytes: &[u8]) -> LoadedDialogue {
        diada_io::load_dialogue(&Journal::from_bytes(bytes), "SCENE", &TpNumbers).unwrap()
    }
    fn before(loaded: &LoadedDialogue, path: &FieldPath) -> DialogueController {
        let binding = loaded
            .bindings
            .iter()
            .find(|binding| &binding.path == path)
            .unwrap();
        let mut bytes = Vec::new();
        for line in &loaded.journal.lines()[..binding.start_line - 1] {
            bytes.extend_from_slice(&line.bytes);
            bytes.extend_from_slice(b"\r\n");
        }
        let prefix = Journal::from_bytes(&bytes);
        let mut controller = DialogueController::resume(prefix, "SCENE");
        let state = controller.next(&TpNumbers).unwrap();
        assert!(
            matches!(state, DialogueState::Question { question, .. } if &question.path == path)
        );
        controller
    }
    fn preview(controller: &DialogueController) -> LivePreview {
        let model = controller.preview().unwrap();
        build(model, controller.bindings(), &InputFields::for_model(model))
    }
    fn pending(loaded: &LoadedDialogue, predicate: impl Fn(&FieldPath) -> bool) -> FieldPath {
        loaded
            .bindings
            .iter()
            .find(|binding| predicate(&binding.path))
            .unwrap()
            .path
            .clone()
    }
    fn current_candidate(controller: &DialogueController, answer: &str) -> LivePreview {
        let mut candidate = controller.clone();
        candidate.answer(answer, &TpNumbers).unwrap();
        preview(&candidate)
    }

    fn user_structure(pp_suffix: bool) -> DialogueController {
        let mut controller = DialogueController::new("USER");
        controller.next(&TpNumbers).unwrap();
        let mut rows = [
            ["R", "0", "1", "O"],
            ["R", "1", "2", "A"],
            ["R", "2", "3", "B"],
            ["P", "3", "0", "C"],
            ["R", "2", "4", "D"],
            ["R", "4", "5", "E"],
            ["P", "5", "0", "F"],
        ];
        if pp_suffix {
            rows[4][0] = "P";
            rows[5][0] = "P";
            rows[6][0] = "R";
        }
        for row in rows {
            for text in row {
                controller.answer(text, &TpNumbers).unwrap();
            }
        }
        for text in [" ", "Y", "0", "1", "N", "N"] {
            controller.answer(text, &TpNumbers).unwrap();
        }
        assert_eq!(
            controller.question().unwrap().path,
            FieldPath::GroupLink(0, 0)
        );
        controller
    }

    fn assert_same_geometry(a: &LivePreview, b: &LivePreview) {
        assert_eq!(a.scene.nodes.len(), b.scene.nodes.len());
        for (a, b) in a.scene.nodes.iter().zip(&b.scene.nodes) {
            assert_eq!((a.key, &a.name, a.position), (b.key, &b.name, b.position));
            assert_eq!(format!("{:?}", a.kind), format!("{:?}", b.kind));
        }
        assert_eq!(a.scene.paths.len(), b.scene.paths.len());
        for (a, b) in a.scene.paths.iter().zip(&b.scene.paths) {
            assert_eq!((a.key, &a.points, a.dashed), (b.key, &b.points, b.dashed));
        }
        assert_eq!(a.scene.outlined_sliders, b.scene.outlined_sliders);
    }

    #[test]
    fn each_accepted_dyad_updates_the_provisional_body_before_group_confirmation() {
        let mut controller = user_structure(false);
        let initial = preview(&controller);
        assert!(initial.schematic);
        assert!(!initial.scene.paths.is_empty());
        assert!(initial.scene.paths.iter().all(|path| path.dashed));
        controller.answer("2", &TpNumbers).unwrap();
        controller.answer("3", &TpNumbers).unwrap();
        assert_eq!(
            controller.question().unwrap().path,
            FieldPath::GroupLink(1, 0)
        );
        let journal = controller.journal().to_bytes();
        let model = controller.preview().unwrap().clone();
        let first = preview(&controller);
        assert!(!first.schematic);
        assert!(!first.geometry_ready);
        assert!(
            first
                .scene
                .nodes
                .iter()
                .zip(&initial.scene.nodes)
                .any(|(a, b)| a.position != b.position)
        );
        // All seven named pairs and every moving body remain visible, even
        // while links 4/5 have not yet been assigned to a dyad.
        assert_eq!(first.scene.nodes.len(), 7);
        for index in 1..6 {
            assert!(first.provisional.contains(&ObjectKey::Link(index)));
            let paths: Vec<_> = first
                .scene
                .paths
                .iter()
                .filter(|path| path.key == ObjectKey::Link(index))
                .collect();
            assert!(!paths.is_empty(), "link {index} disappeared");
            assert!(paths.iter().all(|path| path.dashed));
        }
        assert_eq!(controller.journal().to_bytes(), journal);
        assert_eq!(controller.preview().unwrap(), &model);
        controller.answer("4", &TpNumbers).unwrap();
        controller.answer("5", &TpNumbers).unwrap();
        assert_eq!(
            controller.question().unwrap().path,
            FieldPath::ConfirmGroups
        );
        let second = preview(&controller);
        assert!(!second.schematic);
        assert!(!second.geometry_ready);
        assert!(
            second
                .scene
                .nodes
                .iter()
                .zip(&first.scene.nodes)
                .any(|(a, b)| a.position != b.position)
        );
    }

    #[test]
    fn confirming_groups_and_the_first_zero_coordinate_do_not_switch_body_geometry() {
        for order in [["2", "3", "4", "5"], ["3", "2", "5", "4"]] {
            let mut controller = user_structure(false);
            for text in order {
                controller.answer(text, &TpNumbers).unwrap();
            }
            assert_eq!(
                controller.question().unwrap().path,
                FieldPath::ConfirmGroups
            );
            let before = preview(&controller);
            assert!(!before.schematic);
            controller.answer("Y", &TpNumbers).unwrap();
            assert!(matches!(
                controller.question().unwrap().path,
                FieldPath::GroundX(_)
            ));
            assert_same_geometry(&before, &preview(&controller));
            // Workbench also projects the prefilled numeric zero before Enter.
            assert_same_geometry(&before, &current_candidate(&controller, "0"));
            assert!(!before.geometry_ready);
            assert!(before.scene.paths.iter().all(|path| path.dashed));
        }
    }

    #[test]
    fn an_unassigned_pp_link_remains_dashed_even_though_it_has_no_length_question() {
        let mut controller = user_structure(true);
        controller.answer("2", &TpNumbers).unwrap();
        controller.answer("3", &TpNumbers).unwrap();
        let model = controller.preview().unwrap();
        let id = model.link_by_name(Name(*b"4 ")).unwrap();
        assert_eq!(model.classify_link(id), Ok(b'2'));
        assert!(parameter_known(model, controller.bindings(), id));
        assert!(
            !model
                .groups
                .iter()
                .any(|group| group.kind == GroupKind::Dyad
                    && (group.link_a == id || group.link_b == Some(id)))
        );
        let result = preview(&controller);
        assert!(!result.schematic);
        assert!(!result.geometry_ready);
        assert!(result.provisional.contains(&ObjectKey::Link(id.0)));
        let paths: Vec<_> = result
            .scene
            .paths
            .iter()
            .filter(|path| path.key == ObjectKey::Link(id.0))
            .collect();
        assert!(!paths.is_empty());
        assert!(paths.iter().all(|path| path.dashed));
    }

    #[test]
    fn unknown_ternary_body_remains_a_polygon_and_unknown_slider_offsets_do_not_merge_joints() {
        let mut controller = user_structure(false);
        for text in ["2", "3"] {
            controller.answer(text, &TpNumbers).unwrap();
        }
        for stage in 0..2 {
            let model = controller.preview().unwrap();
            let result = preview(&controller);
            let at = |name| {
                let id = model.pair_by_name(Name([name, b' '])).unwrap();
                result.scene.nodes[id.0].position
            };
            let a = at(b'A');
            let b = at(b'B');
            let d = at(b'D');
            let ab = b - a;
            let ad = d - a;
            let cross = ab.x * ad.y - ab.y * ad.x;
            assert!(
                cross.abs() > ab.length() * ad.length() * 0.1,
                "stage {stage}: A={a:?} B={b:?} D={d:?}"
            );
            assert!(
                at(b'B').distance(at(b'C')) > 0.001,
                "stage {stage}: unanswered OnGuide collapsed B/C"
            );
            assert!(result.scene.paths.iter().all(|path| path.dashed));
            assert!(!result.geometry_ready);
            if stage == 0 {
                controller.answer("4", &TpNumbers).unwrap();
                controller.answer("5", &TpNumbers).unwrap();
            }
        }
    }

    #[test]
    fn an_explicit_zero_attachment_angle_stays_collinear_instead_of_using_the_polygon_placeholder()
    {
        let loaded = loaded(include_bytes!(
            "../../../../tests/fixtures/diada-v320-derived/SCENE.DI"
        ));
        let id = loaded
            .mechanism
            .points
            .iter()
            .enumerate()
            .find_map(|(index, point)| {
                (point.source == AttachmentSource::ExtraPair
                    && loaded.mechanism.classify_link(point.owner) == Ok(b'0'))
                .then_some(PointId(index))
            })
            .unwrap();
        let path = FieldPath::PointAngle(id);
        let controller = before(&loaded, &path);
        let result = current_candidate(&controller, "0");
        let point = &result.display_model.points[id.0];
        assert!(point.geometry.angle.is_zero());
        let owner = &result.display_model.links[point.owner.0];
        let a = result.scene.nodes[owner.pairs[0].unwrap().0].position;
        let b = result.scene.nodes[owner.pairs[1].unwrap().0].position;
        let d = result.scene.nodes[point.pair.0].position;
        let ab = b - a;
        let ad = d - a;
        assert!((ab.x * ad.y - ab.y * ad.x).abs() < 1e-6);
        assert!((d.distance(a) - display(point.geometry.parameter).abs() as f32).abs() < 1e-6);
        // The source controller is still waiting for the angle; provisional
        // construction never accepts a drawing assumption as a journal answer.
        assert_eq!(controller.question().unwrap().path, path);
    }

    #[test]
    fn typed_coordinates_move_the_real_scene_before_acceptance_and_survive_next_question() {
        let loaded = loaded(RR);
        let path = pending(&loaded, |path| matches!(path, FieldPath::GroundX(_)));
        let FieldPath::GroundX(id) = path else {
            unreachable!()
        };
        let mut controller = before(&loaded, &path);
        let original = controller.journal().to_bytes();
        let proposed = current_candidate(&controller, "2.25");
        assert!(!proposed.schematic);
        assert_eq!(proposed.scene.nodes[id.0].position.x, 2.25);
        assert_eq!(controller.journal().to_bytes(), original);
        controller.answer("2.25", &TpNumbers).unwrap();
        let accepted = preview(&controller);
        assert_eq!(accepted.scene.nodes[id.0].position.x, 2.25);
        let y = current_candidate(&controller, "-0.75");
        assert_eq!(y.scene.nodes[id.0].position, Pos2::new(2.25, -0.75));
    }

    #[test]
    fn changing_an_input_rr_length_moves_its_endpoint_and_future_parts_are_dashed() {
        let loaded = loaded(RR);
        let input = loaded.mechanism.inputs[0];
        let controller = before(&loaded, &FieldPath::LinkParameter(input));
        let a = current_candidate(&controller, "0.2");
        let b = current_candidate(&controller, "0.4");
        let out = loaded
            .mechanism
            .groups
            .iter()
            .find(|group| group.kind == GroupKind::Input)
            .unwrap()
            .more
            .unwrap();
        assert!(
            (b.scene.nodes[out.0].position.x - a.scene.nodes[out.0].position.x - 0.2).abs() < 1e-6
        );
        assert!(
            b.scene
                .paths
                .iter()
                .filter(|path| path.key != ObjectKey::Link(input.0))
                .all(|path| path.dashed)
        );
        assert!(
            !b.scene
                .paths
                .iter()
                .find(|path| path.key == ObjectKey::Link(input.0))
                .unwrap()
                .dashed
        );
    }

    #[test]
    fn typed_guide_angle_rotates_the_slider_glyph_without_accepting_the_answer() {
        let loaded = loaded(RP);
        let path = pending(&loaded, |path| matches!(path, FieldPath::GroundAngle(_)));
        let FieldPath::GroundAngle(id) = path else {
            unreachable!()
        };
        let controller = before(&loaded, &path);
        let original = controller.journal().to_bytes();
        let a = current_candidate(&controller, "0");
        let b = current_candidate(&controller, "90");
        let crate::drawing::NodeKind::Slider { angle: aa, .. } = a.scene.nodes[id.0].kind else {
            panic!("not a slider")
        };
        let crate::drawing::NodeKind::Slider { angle: ba, .. } = b.scene.nodes[id.0].kind else {
            panic!("not a slider")
        };
        assert_eq!(aa, 0.0);
        assert!((ba - std::f32::consts::FRAC_PI_2).abs() < 1e-6);
        assert_eq!(controller.journal().to_bytes(), original);
    }

    #[test]
    fn assembly_answers_switch_the_full_candidate_between_the_two_branches() {
        let loaded = loaded(RR);
        let path = pending(&loaded, |path| matches!(path, FieldPath::Assembly(_)));
        let FieldPath::Assembly(id) = path else {
            unreachable!()
        };
        let controller = before(&loaded, &path);
        let a = current_candidate(&controller, "Y");
        let b = current_candidate(&controller, "N");
        let out = loaded.mechanism.groups[id.0].more.unwrap();
        assert!(
            a.scene.nodes[out.0]
                .position
                .distance(b.scene.nodes[out.0].position)
                > 0.01
        );
        assert!(!a.scene.paths.iter().any(|path| path.dashed));
        assert!(!b.scene.paths.iter().any(|path| path.dashed));
    }

    #[test]
    fn no_on_guide_uses_an_explicitly_provisional_nonzero_offset_and_yes_uses_zero() {
        let loaded = loaded(RP);
        let path = pending(&loaded, |path| matches!(path, FieldPath::LinkOnGuide(_)));
        let FieldPath::LinkOnGuide(id) = path else {
            unreachable!()
        };
        let controller = before(&loaded, &path);
        let original_model = controller.preview().unwrap().clone();
        let original_journal = controller.journal().to_bytes();
        let a = current_candidate(&controller, "Y");
        let b = current_candidate(&controller, "N");
        assert!(a.display_model.links[id.0].geometry.parameter.is_zero());
        assert!(!b.display_model.links[id.0].geometry.parameter.is_zero());
        assert!(b.provisional.contains(&ObjectKey::Link(id.0)));
        assert!(
            b.scene
                .paths
                .iter()
                .filter(|path| path.key == ObjectKey::Link(id.0))
                .all(|path| path.dashed)
        );
        assert!(
            a.scene
                .nodes
                .iter()
                .zip(&b.scene.nodes)
                .any(|(a, b)| a.position.distance(b.position) > 0.01)
        );
        assert_eq!(controller.preview().unwrap(), &original_model);
        assert_eq!(controller.journal().to_bytes(), original_journal);
    }

    #[test]
    fn a_failed_provisional_group_does_not_report_the_mechanism_as_unassemblable() {
        let loaded = loaded(RR);
        let controller = before(
            &loaded,
            &FieldPath::LinkParameter(loaded.mechanism.inputs[0]),
        );
        let result = current_candidate(&controller, "3");
        assert!(!result.geometry_ready);
        assert!(
            result.notice.contains("ещё не хватает"),
            "{}",
            result.notice
        );
        assert!(!result.notice.contains("Механизм не собирается"));
        assert!(result.scene.paths.iter().any(|path| path.dashed));
    }

    #[test]
    fn stale_input_arena_ids_are_matched_by_the_visible_link_name() {
        let loaded = loaded(RR);
        let mut fields = InputFields::for_model(&loaded.mechanism);
        fields[0].start = "20".into();
        let step = fields[0].series().unwrap();
        let mut expected = loaded.mechanism.clone();
        expected
            .calculate_position(&[InputPosition {
                position: step.start,
                other_position: step.other_start,
                rate: step.rate,
                rate_derivative: step.rate_derivative,
            }])
            .unwrap();
        fields[0].link = LinkId(999);
        let result = build(&loaded.mechanism, &loaded.bindings, &fields);
        assert!(result.geometry_ready);
        assert_eq!(result.display_model, expected);
    }

    #[test]
    fn partial_ground_coordinates_synchronize_same_name_aliases_before_y_is_given() {
        let loaded = loaded(RR);
        let path = pending(&loaded, |path| matches!(path, FieldPath::GroundX(_)));
        let FieldPath::GroundX(id) = path else {
            unreachable!()
        };
        let controller = before(&loaded, &path);
        let mut candidate = controller.clone();
        candidate.answer("1.125", &TpNumbers).unwrap();
        let mut model = candidate.preview().unwrap().clone();
        let alias = model.pairs.len();
        let mut extra = model.pairs[id.0].clone();
        extra.state.x = number(90.0);
        model.pairs.push(extra);
        let result = build(
            &model,
            candidate.bindings(),
            &InputFields::for_model(&model),
        );
        assert_eq!(result.scene.nodes[id.0].position.x, 1.125);
        assert_eq!(result.scene.nodes[alias].position.x, 1.125);
        assert_eq!(model.pairs[alias].state.x, number(90.0));
    }

    #[test]
    fn unaccepted_numbers_and_rejected_zero_rr_leave_the_confirmed_model_untouched() {
        let loaded = loaded(RR);
        let controller = before(
            &loaded,
            &FieldPath::LinkParameter(loaded.mechanism.inputs[0]),
        );
        let bytes = controller.journal().to_bytes();
        let model = controller.preview().unwrap().clone();
        for text in ["-", "1e", "1e100"] {
            let mut proposed = controller.clone();
            assert!(proposed.answer(text, &TpNumbers).is_err(), "{text}");
            assert_eq!(controller.journal().to_bytes(), bytes);
            assert_eq!(controller.preview().unwrap(), &model);
        }
        let zero = current_candidate(&controller, "0");
        assert!(!zero.geometry_ready);
        assert!(
            zero.provisional
                .contains(&ObjectKey::Link(loaded.mechanism.inputs[0].0))
        );
        assert_eq!(controller.journal().to_bytes(), bytes);
    }

    #[test]
    fn a_failed_known_dyad_preserves_the_calculated_input_and_draws_the_rest_provisionally() {
        let mut loaded = loaded(RR);
        let dyad = loaded
            .mechanism
            .groups
            .iter()
            .find(|group| group.kind == GroupKind::Dyad)
            .unwrap()
            .clone();
        loaded.mechanism.links[dyad.link_a.0].geometry.parameter = number(0.00001);
        loaded.mechanism.links[dyad.link_b.unwrap().0]
            .geometry
            .parameter = number(0.00001);
        let input = loaded
            .mechanism
            .groups
            .iter()
            .find(|group| group.kind == GroupKind::Input)
            .unwrap();
        let mut expected = loaded.mechanism.clone();
        expected
            .calculate_input(
                GroupId(1),
                InputPosition {
                    position: Real48::ZERO,
                    other_position: Real48::ZERO,
                    rate: number(1.0),
                    rate_derivative: Real48::ZERO,
                },
            )
            .unwrap();
        let result = build(
            &loaded.mechanism,
            &loaded.bindings,
            &InputFields::for_model(&loaded.mechanism),
        );
        assert!(!result.geometry_ready);
        assert!(result.notice.contains("не собирается"), "{}", result.notice);
        let out = input.more.unwrap();
        assert_eq!(
            result.display_model.pairs[out.0].state,
            expected.pairs[out.0].state
        );
        assert!(
            !result
                .scene
                .paths
                .iter()
                .find(|path| path.key == ObjectKey::Link(input.link_a.0))
                .unwrap()
                .dashed
        );
        assert!(
            result
                .scene
                .paths
                .iter()
                .filter(|path| path.key != ObjectKey::Link(input.link_a.0))
                .all(|path| path.dashed)
        );
    }

    #[test]
    fn a_failed_rp_dyad_keeps_the_ground_slider_and_its_body_connected_to_the_known_carrier() {
        let mut document = diada_io::EditableDialogue::open(RP, "SCENE", &TpNumbers).unwrap();
        let model = &document.loaded().mechanism;
        let group = model
            .groups
            .iter()
            .find(|group| group.kind == GroupKind::Dyad)
            .unwrap()
            .clone();
        let links = [group.link_a, group.link_b.unwrap()];
        let rr = *links
            .iter()
            .find(|id| model.classify_link(**id) == Ok(b'0'))
            .unwrap();
        let sliding = *links
            .iter()
            .find(|id| model.classify_link(**id) == Ok(b'1'))
            .unwrap();
        let guide = model.links[sliding.0]
            .pairs
            .iter()
            .flatten()
            .copied()
            .find(|id| model.pairs[id.0].kind == PairKind::Prismatic)
            .unwrap();
        document
            .edit_text(&FieldPath::GroundY(guide), "2", &TpNumbers)
            .unwrap();
        document
            .edit_text(&FieldPath::LinkParameter(rr), "0.00001", &TpNumbers)
            .unwrap();
        let loaded = document.loaded();
        let accepted = loaded.mechanism.clone();
        let journal = loaded.journal.to_bytes();
        let mut expected = accepted.clone();
        let input = expected
            .groups
            .iter()
            .position(|group| group.kind == GroupKind::Input)
            .unwrap();
        let output = expected.groups[input].more.unwrap();
        expected
            .calculate_input(
                GroupId(input),
                InputPosition {
                    position: Real48::ZERO,
                    other_position: Real48::ZERO,
                    rate: number(1.0),
                    rate_derivative: Real48::ZERO,
                },
            )
            .unwrap();
        let result = build(
            &accepted,
            &loaded.bindings,
            &InputFields::for_model(&accepted),
        );
        assert!(!result.geometry_ready);
        assert_eq!(
            result.display_model.pairs[output.0].state,
            expected.pairs[output.0].state
        );
        let path = result
            .scene
            .paths
            .iter()
            .find(|path| path.key == ObjectKey::Link(sliding.0))
            .unwrap();
        assert!(path.dashed);
        for (position, pair) in path
            .points
            .iter()
            .zip(accepted.links[sliding.0].pairs.iter().flatten())
        {
            assert_eq!(*position, result.scene.nodes[pair.0].position);
        }
        let carrier = accepted.pairs[guide.0].state.display_xy();
        assert_eq!(
            result.scene.nodes[guide.0].position,
            Pos2::new(carrier[0] as f32, carrier[1] as f32)
        );
        assert!(result.provisional.contains(&ObjectKey::Pair(guide.0)));
        assert_eq!(document.loaded().mechanism, accepted);
        assert_eq!(document.loaded().journal.to_bytes(), journal);
    }

    #[test]
    fn completed_geometry_matches_the_exact_core_for_all_five_dyads_and_two_inputs() {
        let fixtures: [&[u8]; 8] = [
            RR,
            RP,
            include_bytes!("../../../../tests/fixtures/diada-v320-derived/dyad-03/SCENE.DI"),
            include_bytes!("../../../../tests/fixtures/diada-v320-derived/dyad-04/SCENE.DI"),
            include_bytes!("../../../../tests/fixtures/diada-v320-derived/dyad-05/SCENE.DI"),
            include_bytes!("../../../../tests/fixtures/diada-v320-derived/two-inputs-b/SCENE.DI"),
            include_bytes!("../../../../tests/fixtures/diada-v320-derived/force-fixed-02/SCENE.DI"),
            include_bytes!(
                "../../../../tests/fixtures/diada-v320-edge-runtime/pr-input-zero-step/SCENE.DI"
            ),
        ];
        for (index, bytes) in fixtures.into_iter().enumerate() {
            let loaded = loaded(bytes);
            let mut fields = InputFields::for_model(&loaded.mechanism);
            let mut checked = false;
            for start in [
                -2.5, 0.0, 0.5, 1.0, 2.5, 15.0, 30.0, 45.0, 60.0, 75.0, 90.0, 120.0, 180.0,
            ] {
                for field in &mut fields {
                    field.start = start.to_string();
                }
                let steps: Vec<_> = fields
                    .iter()
                    .map(|field| {
                        let input = field.series().unwrap();
                        InputPosition {
                            position: input.start,
                            other_position: input.other_start,
                            rate: input.rate,
                            rate_derivative: input.rate_derivative,
                        }
                    })
                    .collect();
                let mut expected = loaded.mechanism.clone();
                if expected.calculate_position(&steps).is_err() {
                    continue;
                }
                let result = build(&loaded.mechanism, &loaded.bindings, &fields);
                assert!(
                    result.geometry_ready,
                    "fixture {index}: {} {:?}",
                    result.notice, result.provisional
                );
                assert_eq!(result.display_model, expected, "fixture {index}");
                let exact_scene = Scene::mechanism(&expected);
                for (a, b) in result.scene.nodes.iter().zip(&exact_scene.nodes) {
                    assert_eq!(a.position, b.position, "fixture {index}");
                }
                assert!(!result.scene.paths.iter().any(|path| path.dashed));
                checked = true;
                break;
            }
            assert!(checked, "fixture {index} has no tested defined position");
        }
    }
}
