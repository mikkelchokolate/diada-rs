//! Visual cues use display coordinates. Accepted dimensions are never changed
//! by a provisional canvas answer.
use crate::drawing::{
    Annotation, Dimension, LoadGlyph, LoadKind, MomentGlyph, ObjectKey, Scene, Vector,
    VectorLabelKind,
};
use diada_core::*;
use diada_io::{DialogueNumbers, FieldPath, QuestionKind, ReplayQuestion, TpNumbers};
use eframe::egui::{Color32, Pos2, Vec2};

const CUE: Color32 = Color32::from_rgb(27, 115, 166);

fn display(value: Real48) -> f32 {
    value.load_extended().to_display_f64() as f32
}
fn direction(angle: f32) -> Vec2 {
    Vec2::new(angle.cos(), angle.sin())
}
fn at(scene: &Scene, model: &Mechanism, id: PairId, schematic: bool) -> Option<Pos2> {
    let p = if schematic {
        scene.nodes.get(id.0)?.position
    } else {
        let [x, y] = model.pair(id)?.state.display_xy();
        Pos2::new(x as f32, y as f32)
    };
    (p.x.is_finite() && p.y.is_finite()).then_some(p)
}
fn name(model: &Mechanism, id: PairId) -> String {
    model.pair(id).map(|p| p.name.display()).unwrap_or_default()
}
fn note(scene: &mut Scene, position: Pos2, text: impl Into<String>) {
    scene.annotations.push(Annotation {
        position,
        text: text.into(),
    });
}
fn ray(scene: &mut Scene, key: ObjectKey, start: Pos2, end: Pos2, label: impl Into<String>) {
    scene.vectors.push(Vector {
        key,
        start,
        end,
        label: label.into(),
        label_kind: VectorLabelKind::Endpoint,
        color: CUE,
    });
}
fn length(scene: &mut Scene, start: Pos2, end: Pos2, label: impl Into<String>) {
    scene.dimensions.push(Dimension::Length {
        start,
        end,
        label: label.into(),
    });
}
fn arc(scene: &mut Scene, center: Pos2, from: f32, sweep: f32, label: impl Into<String>) {
    scene.dimensions.push(Dimension::Angle {
        center,
        from,
        sweep,
        label: label.into(),
    });
}

pub fn compare_dimension(scene: &mut Scene, question: &ReplayQuestion, old: &str, proposed: &str) {
    let units = question
        .units
        .map(|unit| format!(" {unit}"))
        .unwrap_or_default();
    let (old, proposed) = if question.kind == QuestionKind::Number {
        (
            crate::ui_numbers::compact(old),
            crate::ui_numbers::compact(proposed),
        )
    } else {
        (old.trim().to_owned(), proposed.trim().to_owned())
    };
    let original_label = format!("{proposed}{units}");
    let comparison = format!("{old} → {proposed}{units}");
    for dimension in &mut scene.dimensions {
        let label = match dimension {
            Dimension::Length { label, .. } | Dimension::Angle { label, .. } => label,
        };
        if *label == original_label {
            *label = comparison.clone();
        }
    }
    for annotation in &mut scene.annotations {
        if !units.is_empty() && annotation.text.contains(&original_label) {
            annotation.text = annotation.text.replace(&original_label, &comparison);
        }
    }
}
fn link_center(scene: &Scene, model: &Mechanism, id: LinkId, schematic: bool) -> Option<Pos2> {
    let link = model.link(id)?;
    let mut nodes = link
        .pairs
        .iter()
        .flatten()
        .filter_map(|p| at(scene, model, *p, schematic));
    let first = nodes.next()?;
    Some(
        nodes
            .next()
            .map(|last| first.lerp(last, 0.5))
            .unwrap_or(first),
    )
}
fn mass_center(
    scene: &Scene,
    model: &Mechanism,
    owner: LinkId,
    schematic: bool,
) -> Option<(Pos2, String)> {
    let id = model.link(owner)?.mass.as_ref()?.center?;
    let point = model.point(id)?;
    Some((
        at(scene, model, point.pair, schematic)?,
        name(model, point.pair),
    ))
}
fn attachment_reference(model: &Mechanism, id: PointId) -> Option<(PairId, u8)> {
    let point = model.point(id)?;
    let owner = model.link(point.owner)?;
    let typ = model
        .groups
        .iter()
        .find(|g| g.attached_point == Some(id))?
        .input_type;
    let first_p = owner.pairs[0]
        .and_then(|p| model.pair(p))
        .is_some_and(|p| p.kind == PairKind::Prismatic);
    let slot = if matches!(typ, b'2' | b'5') || matches!(typ, b'1' | b'4') && first_p {
        1
    } else {
        0
    };
    Some((owner.pairs[slot]?, typ))
}
/// A guide's stored XY is a point on its carrier, not necessarily the visible
/// slider centre. Project onto the full line, never measure to that XY point.
fn offset_support(model: &Mechanism, id: LinkId) -> Option<(PairId, PairId)> {
    let link = model.link(id)?;
    let pairs: Vec<_> = link.pairs.iter().copied().flatten().collect();
    let hinge = pairs
        .iter()
        .copied()
        .find(|p| model.pair(*p).is_some_and(|p| p.kind == PairKind::Revolute))?;
    let guide = pairs
        .iter()
        .copied()
        .find(|p| {
            model
                .pair(*p)
                .is_some_and(|p| p.kind == PairKind::Prismatic)
        })
        .or_else(|| {
            model
                .groups
                .iter()
                .find(|g| {
                    g.kind == GroupKind::Dyad
                        && g.input_type == b'3'
                        && (g.link_a == id || g.link_b == Some(id))
                })
                .and_then(|g| g.more)
        })?;
    Some((hinge, guide))
}
fn projected_foot(point: Pos2, guide: Pos2, angle: f32) -> Pos2 {
    let axis = direction(angle);
    guide + axis * (point - guide).dot(axis)
}
fn yes(text: &str) -> Option<bool> {
    match text.trim().to_uppercase().as_str() {
        "Y" | "Д" | "ДА" => Some(true),
        "N" | "Н" | "НЕТ" => Some(false),
        _ => None,
    }
}

fn on_guide_sketch(
    scene: &mut Scene,
    model: &Mechanism,
    reference: PairId,
    guide: PairId,
    text: &str,
    angle: f32,
    span: f32,
) {
    let cases: &[bool] = match yes(text) {
        Some(true) => &[true],
        Some(false) => &[false],
        None => &[true, false],
    };
    let base = scene.bounds().center() + Vec2::new(0.0, -span * 1.6);
    for (index, on_guide) in cases.iter().enumerate() {
        let h = base + Vec2::new(index as f32 * span * 1.8, 0.0);
        let b = if *on_guide {
            h
        } else {
            h + direction(angle + std::f32::consts::FRAC_PI_2) * span * 0.6
        };
        ray(
            scene,
            ObjectKey::Pair(guide.0),
            h - direction(angle) * span * 0.3,
            h + direction(angle) * span,
            format!("+{}", name(model, guide)),
        );
        if *on_guide {
            note(
                scene,
                h,
                format!(
                    "Условно: Да · {} = H · расстояние 0",
                    name(model, reference)
                ),
            );
        } else {
            length(scene, b, h, "? м · перпендикуляр");
            note(
                scene,
                b,
                format!("Условно: Нет · {} вне направляющей", name(model, reference)),
            );
            note(scene, h, "H · основание перпендикуляра");
        }
    }
}

fn offset_cue(
    scene: &mut Scene,
    model: &Mechanism,
    id: LinkId,
    label: &str,
    signed_answer: Option<&str>,
    schematic: bool,
    span: f32,
) {
    let signed = signed_answer.is_some();
    let text = signed_answer.unwrap_or("");
    let Some((hinge, guide)) = offset_support(model, id) else {
        return;
    };
    let Some(b) = at(scene, model, hinge, schematic) else {
        return;
    };
    let Some(origin) = at(scene, model, guide, schematic) else {
        return;
    };
    let angle = model
        .pair(guide)
        .map(|p| display(p.state.fi))
        .unwrap_or(0.0);
    let h = projected_foot(b, origin, angle);
    if schematic && signed {
        // A conditional example is a question construction, not a calculated
        // new position. In the original Y means clockwise B,H,Q.
        let cases: &[bool] = match yes(text) {
            Some(true) => &[true],
            Some(false) => &[false],
            None => &[true, false],
        };
        for (index, clockwise) in cases.iter().enumerate() {
            let h = origin + Vec2::new(index as f32 * span * 1.8, -span * 1.6);
            let side = if *clockwise { -1.0 } else { 1.0 };
            let b = h + direction(angle + std::f32::consts::FRAC_PI_2) * span * 0.42 * side;
            let q = h + direction(angle) * span;
            ray(
                scene,
                ObjectKey::Pair(guide.0),
                h,
                q,
                format!("+{} · Q", name(model, guide)),
            );
            length(scene, b, h, label);
            ray(scene, ObjectKey::Link(id.0), b, h, "H");
            ray(scene, ObjectKey::Link(id.0), q, b, name(model, hinge));
            note(
                scene,
                b,
                format!(
                    "Условно: {} → H → Q · {}",
                    name(model, hinge),
                    if *clockwise {
                        "Да — по часовой"
                    } else {
                        "Нет — против часовой"
                    }
                ),
            );
        }
        return;
    }
    let q = h + direction(angle) * span;
    ray(
        scene,
        ObjectKey::Pair(guide.0),
        h - direction(angle) * span * 0.3,
        q,
        format!("+{} · Q", name(model, guide)),
    );
    length(scene, b, h, label);
    note(scene, h, "H · основание перпендикуляра");
    if signed {
        ray(scene, ObjectKey::Link(id.0), b, h, name(model, hinge));
        ray(scene, ObjectKey::Link(id.0), q, b, "");
        note(
            scene,
            b,
            format!(
                "{}{} → H → Q: Да — по часовой; Нет — против",
                if schematic { "Условно: " } else { "" },
                name(model, hinge)
            ),
        );
    }
}

fn acute_cue(
    scene: &mut Scene,
    model: &Mechanism,
    group: &Group,
    text: &str,
    schematic: bool,
    span: f32,
) {
    let (base, tip, guide) = if group.input_type == b'2' {
        let (Some(x), Some(y), Some(inner)) = (group.pair_x, group.pair_y, group.more) else {
            return;
        };
        if model.pair(x).is_some_and(|p| p.kind == PairKind::Revolute) {
            (x, inner, y)
        } else {
            (y, inner, x)
        }
    } else {
        // U07: vector PairY -> PairX, not PairX -> PairY.
        let (Some(x), Some(y), Some(inner)) = (group.pair_x, group.pair_y, group.more) else {
            return;
        };
        (y, x, inner)
    };
    let (Some(start), Some(end), Some(guide_at)) = (
        at(scene, model, base, schematic),
        at(scene, model, tip, schematic),
        at(scene, model, guide, schematic),
    ) else {
        return;
    };
    let from = model
        .pair(guide)
        .map(|p| display(p.state.fi))
        .unwrap_or(0.0);
    let vector_label = format!("{}{}", name(model, base), name(model, tip));
    if schematic {
        let cases: &[bool] = match yes(text) {
            Some(true) => &[true],
            Some(false) => &[false],
            None => &[true, false],
        };
        for (index, positive) in cases.iter().enumerate() {
            let center = start + Vec2::new(index as f32 * span * 1.65, -span * 1.2);
            let sweep = if *positive {
                std::f32::consts::FRAC_PI_4
            } else {
                std::f32::consts::PI * 0.75
            };
            ray(
                scene,
                ObjectKey::Pair(guide.0),
                center,
                center + direction(from) * span,
                format!("+{}", name(model, guide)),
            );
            ray(
                scene,
                ObjectKey::Link(group.link_a.0),
                center,
                center + direction(from + sweep) * span,
                &vector_label,
            );
            arc(
                scene,
                center,
                from,
                sweep,
                if *positive {
                    "Да · острый"
                } else {
                    "Нет · тупой"
                },
            );
            note(
                scene,
                center - Vec2::new(0.0, span * 0.4),
                if *positive {
                    "Условно: да"
                } else {
                    "Условно: нет"
                },
            );
        }
    } else {
        ray(
            scene,
            ObjectKey::Link(group.link_a.0),
            start,
            end,
            vector_label,
        );
        ray(
            scene,
            ObjectKey::Pair(guide.0),
            guide_at,
            guide_at + direction(from) * span,
            format!("+{}", name(model, guide)),
        );
        let delta = end - start;
        if delta.length_sq() > 0.0 {
            let sweep = (delta.y.atan2(delta.x) - from + std::f32::consts::PI)
                .rem_euclid(std::f32::consts::TAU)
                - std::f32::consts::PI;
            ray(
                scene,
                ObjectKey::Pair(guide.0),
                start,
                start + direction(from) * span,
                "Перенос направления для сравнения",
            );
            arc(
                scene,
                start,
                from,
                sweep,
                "Да — острый/совпадает; Нет — тупой",
            );
        }
    }
}

pub fn annotate(
    scene: &mut Scene,
    model: &Mechanism,
    q: &ReplayQuestion,
    text: &str,
    schematic: bool,
) {
    let value = if text.trim().is_empty() {
        "?".to_owned()
    } else if q.kind == QuestionKind::Number {
        crate::ui_numbers::compact(text)
    } else {
        text.to_owned()
    };
    let label = format!(
        "{}{}",
        value,
        q.units.map(|u| format!(" {u}")).unwrap_or_default()
    );
    let number = diada_compat::parse_real(text.trim().as_bytes()).ok();
    let sweep = number
        .and_then(|v| TpNumbers.degrees_to_radians(v).ok())
        .map(display);
    let bounds = scene.bounds();
    let span = bounds.width().max(bounds.height()).max(0.12) * 0.35;
    let fallback = bounds.center();
    match q.path {
        FieldPath::LinkParameter(id) | FieldPath::LinkOnGuide(id) | FieldPath::SignedOffset(id) => {
            if let Some((hinge, guide)) = offset_support(model, id) {
                let signed = matches!(q.path, FieldPath::SignedOffset(_));
                let dim_label = if signed {
                    "Перпендикуляр · выбирается сторона"
                } else if matches!(q.path, FieldPath::LinkOnGuide(_)) {
                    match yes(text) {
                        Some(true) => "Да · расстояние 0 м",
                        Some(false) => "Нет · перпендикулярное расстояние",
                        None => "Да — расстояние 0; Нет — введите расстояние",
                    }
                } else {
                    &label
                };
                if schematic && matches!(q.path, FieldPath::LinkOnGuide(_)) {
                    let angle = model
                        .pair(guide)
                        .map(|p| display(p.state.fi))
                        .unwrap_or(0.0);
                    on_guide_sketch(scene, model, hinge, guide, text, angle, span);
                    return;
                }
                offset_cue(
                    scene,
                    model,
                    id,
                    dim_label,
                    signed.then_some(text),
                    schematic,
                    span,
                );
                if !signed {
                    note(
                        scene,
                        at(scene, model, hinge, schematic).unwrap_or(fallback),
                        format!(
                            "{}Перпендикуляр от {} к направляющей {}",
                            if schematic { "Условно: " } else { "" },
                            name(model, hinge),
                            name(model, guide)
                        ),
                    );
                }
            } else if let Some(link) = model.link(id) {
                let points: Vec<_> = link
                    .pairs
                    .iter()
                    .flatten()
                    .filter_map(|p| at(scene, model, *p, schematic))
                    .collect();
                if points.len() >= 2 {
                    length(scene, points[0], points[1], &label);
                    note(
                        scene,
                        points[0].lerp(points[1], 0.5),
                        format!(
                            "{}Длина звена {} между шарнирами",
                            if schematic { "Условно: " } else { "" },
                            link.name.display()
                        ),
                    );
                }
            }
        }
        FieldPath::GroundX(id) | FieldPath::GroundY(id) => {
            if let Some(position) = at(scene, model, id, schematic) {
                let x = matches!(q.path, FieldPath::GroundX(_));
                let start = if x {
                    Pos2::new(0.0, position.y)
                } else {
                    Pos2::new(position.x, 0.0)
                };
                length(scene, start, position, &label);
                ray(
                    scene,
                    ObjectKey::Pair(id.0),
                    Pos2::ZERO,
                    if x {
                        Pos2::new(span, 0.0)
                    } else {
                        Pos2::new(0.0, span)
                    },
                    if x { "+X" } else { "+Y" },
                );
                let is_p = model
                    .pair(id)
                    .is_some_and(|p| p.kind == PairKind::Prismatic);
                note(
                    scene,
                    position,
                    format!(
                        "{}{} · {}",
                        if schematic { "Условно: " } else { "" },
                        name(model, id),
                        if is_p {
                            "координата опорной точки направляющей"
                        } else {
                            "координата центра шарнира"
                        }
                    ),
                );
            }
        }
        FieldPath::GroundAngle(id) => {
            if let Some(center) = at(scene, model, id, schematic) {
                ray(
                    scene,
                    ObjectKey::Pair(id.0),
                    center,
                    center + Vec2::new(span, 0.0),
                    "+X",
                );
                if let Some(angle) = sweep {
                    ray(
                        scene,
                        ObjectKey::Pair(id.0),
                        center,
                        center + direction(angle) * span,
                        format!("+{}", name(model, id)),
                    );
                    arc(scene, center, 0.0, angle, &label);
                }
                note(
                    scene,
                    center,
                    format!(
                        "{}От +X к +{}; против часовой — положительно",
                        if schematic { "Условно: " } else { "" },
                        name(model, id)
                    ),
                );
            }
        }
        FieldPath::InputGuideAngle(id) => {
            if let Some(link) = model.link(id)
                && let (Some(first), Some(second)) = (link.pairs[0], link.pairs[1])
                && let Some(center) = at(scene, model, first, schematic)
            {
                let from = model
                    .pair(first)
                    .map(|p| display(p.state.fi))
                    .unwrap_or(0.0);
                ray(
                    scene,
                    ObjectKey::Pair(first.0),
                    center,
                    center + direction(from) * span,
                    format!("+{} · исходное", name(model, first)),
                );
                if let Some(angle) = sweep {
                    ray(
                        scene,
                        ObjectKey::Pair(second.0),
                        center,
                        center + direction(from + angle) * span,
                        format!("+{} · конечное", name(model, second)),
                    );
                    arc(scene, center, from, angle, &label);
                }
                note(
                    scene,
                    center,
                    format!(
                        "{}Угол от +{} до +{} · направления перенесены для сравнения",
                        if schematic { "Условно: " } else { "" },
                        name(model, first),
                        name(model, second)
                    ),
                );
            }
        }
        FieldPath::Assembly(id) => {
            if let Some(group) = model.group(id) {
                if group.input_type == b'1' {
                    let ids: Vec<_> = [group.pair_x, group.more, group.pair_y]
                        .into_iter()
                        .flatten()
                        .collect();
                    let points: Vec<_> = ids
                        .iter()
                        .filter_map(|p| at(scene, model, *p, schematic))
                        .collect();
                    if points.len() == 3 {
                        if schematic {
                            let cases: &[bool] = match yes(text) {
                                Some(true) => &[true],
                                Some(false) => &[false],
                                None => &[true, false],
                            };
                            for (index, clockwise) in cases.iter().enumerate() {
                                let base =
                                    fallback + Vec2::new(index as f32 * span * 2.0, -span * 1.8);
                                let p = [
                                    base,
                                    base + Vec2::new(
                                        span * 0.5,
                                        if *clockwise { span } else { -span },
                                    ),
                                    base + Vec2::new(span, 0.0),
                                ];
                                for j in 0..3 {
                                    ray(
                                        scene,
                                        ObjectKey::Link(group.link_a.0),
                                        p[j],
                                        p[(j + 1) % 3],
                                        name(model, ids[(j + 1) % 3]),
                                    );
                                }
                                note(
                                    scene,
                                    base,
                                    if *clockwise {
                                        "Условно: Да — по часовой"
                                    } else {
                                        "Условно: Нет — против часовой"
                                    },
                                );
                            }
                        } else {
                            for j in 0..3 {
                                ray(
                                    scene,
                                    ObjectKey::Link(group.link_a.0),
                                    points[j],
                                    points[(j + 1) % 3],
                                    name(model, ids[(j + 1) % 3]),
                                );
                            }
                        }
                        note(
                            scene,
                            points[0],
                            format!(
                                "Обход {} → {} → {} → {}. Да — по часовой",
                                name(model, ids[0]),
                                name(model, ids[1]),
                                name(model, ids[2]),
                                name(model, ids[0])
                            ),
                        );
                    }
                } else if matches!(group.input_type, b'2' | b'3') {
                    acute_cue(scene, model, group, text, schematic, span);
                }
            }
        }
        FieldPath::PointDistance(id) | FieldPath::PointAngle(id) | FieldPath::PointOnGuide(id) => {
            if let Some(point) = model.point(id)
                && let Some(owner) = model.link(point.owner)
                && let Some((reference, typ)) = attachment_reference(model, id)
                && let (Some(start), Some(end)) = (
                    at(scene, model, reference, schematic),
                    at(scene, model, point.pair, schematic),
                )
            {
                if matches!(q.path, FieldPath::PointAngle(_)) {
                    let from = display(owner.geometry.angle);
                    ray(
                        scene,
                        ObjectKey::Link(point.owner.0),
                        start,
                        start + direction(from) * span,
                        "Исходный вектор звена",
                    );
                    if let Some(angle) = sweep {
                        ray(
                            scene,
                            ObjectKey::Pair(point.pair.0),
                            start,
                            start + direction(from + angle) * span,
                            format!("{}{}", name(model, reference), name(model, point.pair)),
                        );
                        arc(scene, start, from, angle, &label);
                    }
                    note(
                        scene,
                        start,
                        format!(
                            "{}От вектора звена {} до {}{}; против часовой — положительно",
                            if schematic { "Условно: " } else { "" },
                            owner.name.display(),
                            name(model, reference),
                            name(model, point.pair)
                        ),
                    );
                } else {
                    let on_guide = matches!(q.path, FieldPath::PointOnGuide(_));
                    if schematic && on_guide {
                        // The additional P is the guide; its stored XY is the
                        // foot measured from the selected owner reference.
                        let angle = display(owner.geometry.angle) + std::f32::consts::FRAC_PI_2;
                        on_guide_sketch(scene, model, reference, point.pair, text, angle, span);
                        return;
                    }
                    length(
                        scene,
                        start,
                        end,
                        if on_guide {
                            "Да — расстояние 0; Нет — задайте перпендикуляр"
                        } else {
                            &label
                        },
                    );
                    if matches!(typ, b'3' | b'4') {
                        let angle = model
                            .pair(point.pair)
                            .map(|p| display(p.state.fi))
                            .unwrap_or(0.0);
                        ray(
                            scene,
                            ObjectKey::Pair(point.pair.0),
                            end - direction(angle) * span * 0.3,
                            end + direction(angle) * span,
                            format!("+{}", name(model, point.pair)),
                        );
                        note(
                            scene,
                            end,
                            format!(
                                "{}H · основание перпендикуляра из {} к {}",
                                if schematic { "Условно: " } else { "" },
                                name(model, reference),
                                name(model, point.pair)
                            ),
                        );
                    } else {
                        note(
                            scene,
                            end,
                            format!(
                                "{}{}{} · привязка к звену {}",
                                if schematic { "Условно: " } else { "" },
                                name(model, reference),
                                name(model, point.pair),
                                owner.name.display()
                            ),
                        );
                    }
                }
            }
        }
        FieldPath::Mass(id) | FieldPath::Inertia(id) | FieldPath::MassCenter(id) => {
            let (position, center_name) = mass_center(scene, model, id, schematic).unwrap_or((
                link_center(scene, model, id, schematic).unwrap_or(fallback),
                "центр масс".into(),
            ));
            let link = model.link(id).map(|l| l.name.display()).unwrap_or_default();
            let info = match q.path {
                FieldPath::Mass(_) => format!("Звено {link}, {center_name} · масса {label}"),
                FieldPath::Inertia(_) => {
                    format!("⊙ Ось через {center_name}, перпендикулярно плоскости · J = {label}")
                }
                _ => format!("Центр масс звена {link} · имя, положение задаётся далее"),
            };
            note(scene, position, info);
        }
        FieldPath::ForceMagnitude(id) | FieldPath::ForceAngle(id) => {
            if let Some(point) = model.point(id)
                && let Some(pair) = model.pair(point.pair)
                && let Some(position) = at(scene, model, point.pair, schematic)
            {
                let key = ObjectKey::Pair(point.pair.0);
                scene.loads.retain(|load| load.key != key);
                let magnitude = if matches!(q.path, FieldPath::ForceMagnitude(_)) {
                    number
                } else {
                    Some(pair.state.rx)
                };
                let angle = if matches!(q.path, FieldPath::ForceAngle(_)) {
                    sweep
                } else {
                    Some(display(pair.state.ry))
                };
                if let (Some(magnitude), Some(angle)) = (magnitude, angle)
                    && !magnitude.is_zero()
                {
                    scene.loads.push(LoadGlyph {
                        key,
                        position,
                        kind: LoadKind::Force { angle },
                    });
                }
                if matches!(q.path, FieldPath::ForceAngle(_)) {
                    ray(scene, key, position, position + Vec2::new(span, 0.0), "+X");
                    if let Some(angle) = sweep {
                        arc(scene, position, 0.0, angle, &label);
                    }
                }
                let owner = model
                    .link(point.owner)
                    .map(|l| l.name.display())
                    .unwrap_or_default();
                note(
                    scene,
                    position,
                    format!(
                        "{}{} · сила на звене {}: {}{}",
                        if schematic { "Условно: " } else { "" },
                        pair.name.display(),
                        owner,
                        label,
                        if magnitude.is_some_and(|n| n.is_zero()) {
                            " · нулевая сила, стрелки нет"
                        } else {
                            ""
                        }
                    ),
                );
            }
        }
        FieldPath::Moment(id) => {
            if let Some(position) = link_center(scene, model, id, schematic) {
                let key = ObjectKey::Link(id.0);
                scene.moments.retain(|moment| moment.key != key);
                if let Some(number) = number
                    && !number.is_zero()
                {
                    scene.moments.push(MomentGlyph {
                        key,
                        position,
                        negative: number.to_le_bytes()[5] & 0x80 != 0,
                        label: label.clone(),
                    });
                }
                note(
                    scene,
                    position,
                    format!(
                        "{}Внешний момент звена {} · + против часовой; − по часовой{}",
                        if schematic { "Условно: " } else { "" },
                        model.link(id).map(|l| l.name.display()).unwrap_or_default(),
                        if number.is_some_and(|n| n.is_zero()) {
                            " · M = 0"
                        } else {
                            ""
                        }
                    ),
                );
            }
        }
        _ => {
            let object = q
                .objects
                .iter()
                .find_map(|n| model.pairs.iter().position(|p| p.name.0 == n.0))
                .and_then(|i| at(scene, model, PairId(i), schematic))
                .or_else(|| {
                    q.objects
                        .iter()
                        .find_map(|n| model.links.iter().position(|l| l.name.0 == n.0))
                        .and_then(|i| link_center(scene, model, LinkId(i), schematic))
                })
                .unwrap_or(fallback);
            note(scene, object, crate::fields::question_text(q, Some(model)));
        }
    }
}

/// Original U01 motion traces include only T/M/F attachments after the first
/// input group. Pinned 3.20 OVR +0x5D4E: CS1EBD checks group T; CS1ECB/1ED9
/// exclude R/P. Ordinary hinges and sliders never acquire a trace implicitly.
pub fn trajectory_points(model: &Mechanism) -> Vec<(PairId, Pos2)> {
    let Some(ground) = model.ground else {
        return Vec::new();
    };
    let Some(input) = model
        .groups
        .iter()
        .position(|group| group.kind == GroupKind::Input)
    else {
        return Vec::new();
    };
    let mut seen = std::collections::HashSet::new();
    model
        .groups
        .iter()
        .skip(input + 1)
        .filter(|group| group.kind == GroupKind::Attached)
        .filter_map(|group| {
            let owner = group.link_b?;
            if owner == ground || model.link(owner).is_none() {
                return None;
            }
            let id = group.more?;
            let point = group.attached_point.and_then(|id| model.point(id))?;
            if point.pair != id || point.owner != owner {
                return None;
            }
            let pair = model.pair(id)?;
            if !matches!(
                pair.kind,
                PairKind::Point | PairKind::MassCenter | PairKind::ForcePoint
            ) {
                return None;
            }
            let [x, y] = pair.state.display_xy();
            let position = Pos2::new(x as f32, y as f32);
            (position.x.is_finite() && position.y.is_finite() && seen.insert(id))
                .then_some((id, position))
        })
        .collect()
}

pub fn plan(model: &Mechanism, acceleration: bool) -> Result<Scene, String> {
    Scene::plan(model, acceleration).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use diada_io::QuestionKind;

    fn r(text: &str) -> Real48 {
        diada_compat::parse_real(text.as_bytes()).unwrap()
    }
    fn question(path: FieldPath, units: Option<&'static str>) -> ReplayQuestion {
        ReplayQuestion {
            path,
            kind: QuestionKind::Number,
            text: "Параметр".into(),
            units,
            objects: vec![],
            current: None,
            answer_start_line: 1,
        }
    }
    fn rp_model() -> (Mechanism, LinkId, PairId, PairId) {
        let mut model = Mechanism::default();
        let ground = model.add_link(Name(*b"0 "));
        let body = model.add_link(Name(*b"1 "));
        let guide = model
            .add_pair(PairKind::Prismatic, Name(*b"P "), [ground, body])
            .unwrap();
        let hinge = model
            .add_pair(PairKind::Revolute, Name(*b"R "), [ground, body])
            .unwrap();
        model.pairs[guide.0].state.x = r("100");
        model.pairs[hinge.0].state.x = r("2");
        model.pairs[hinge.0].state.y = r("3");
        (model, body, guide, hinge)
    }

    #[test]
    fn compact_dimension_comparison_retains_original_geometry_and_numeric_input() {
        let (model, body, guide, _) = rp_model();
        let raw = " 1.25000000000000000E+0000";
        for (path, units) in [
            (FieldPath::LinkParameter(body), "м"),
            (FieldPath::GroundAngle(guide), "град"),
        ] {
            let question = question(path, Some(units));
            let mut scene = Scene::mechanism(&model);
            let mut short_scene = scene.clone();
            annotate(&mut scene, &model, &question, raw, false);
            annotate(&mut short_scene, &model, &question, "1.25", false);
            assert_eq!(format!("{scene:?}"), format!("{short_scene:?}"));
            let geometry = scene.dimensions.clone();
            compare_dimension(&mut scene, &question, " 0.00000000000000000E+0000", raw);
            for (before, after) in geometry.iter().zip(&scene.dimensions) {
                let label = match (before, after) {
                    (
                        Dimension::Length { start, end, .. },
                        Dimension::Length {
                            start: next_start,
                            end: next_end,
                            label,
                        },
                    ) => {
                        assert_eq!((start, end), (next_start, next_end));
                        label
                    }
                    (
                        Dimension::Angle {
                            center,
                            from,
                            sweep,
                            ..
                        },
                        Dimension::Angle {
                            center: next_center,
                            from: next_from,
                            sweep: next_sweep,
                            label,
                        },
                    ) => {
                        assert_eq!((center, from, sweep), (next_center, next_from, next_sweep));
                        label
                    }
                    _ => panic!("dimension kind changed"),
                };
                assert_eq!(*label, format!("0 → 1.25 {units}"));
            }
            assert!(!scene.dimensions.is_empty());
        }
    }

    #[test]
    fn perpendicular_uses_guide_line_not_its_arbitrary_reference_point() {
        let (model, body, _, _) = rp_model();
        let mut scene = Scene::mechanism(&model);
        annotate(
            &mut scene,
            &model,
            &question(FieldPath::SignedOffset(body), None),
            "Y",
            false,
        );
        let Some(Dimension::Length { start, end, label }) = scene.dimensions.first() else {
            panic!("missing perpendicular")
        };
        assert_eq!(*start, Pos2::new(2.0, 3.0));
        assert_eq!(*end, Pos2::new(2.0, 0.0));
        assert!(label.contains("сторона"));
        assert!(!label.contains("Y м"));
        assert!(
            scene
                .annotations
                .iter()
                .any(|a| a.text.contains("R → H → Q"))
        );
    }

    #[test]
    fn rp_attachment_measures_from_r_even_if_p_occupies_first_slot() {
        let (mut model, body, _, hinge) = rp_model();
        let id = model
            .add_point(body, PairKind::Point, Name(*b"T "))
            .unwrap();
        let point = model.points[id.0].pair;
        model.pairs[point.0].state.x = r("4");
        model.pairs[point.0].state.y = r("5");
        let mut group = Group::new(GroupKind::Attached, body);
        group.link_b = Some(body);
        group.attached_point = Some(id);
        group.more = Some(point);
        group.input_type = b'1';
        model.groups.push(group);
        let mut scene = Scene::mechanism(&model);
        annotate(
            &mut scene,
            &model,
            &question(FieldPath::PointDistance(id), Some("м")),
            "2.5",
            false,
        );
        let Some(Dimension::Length { start, end, .. }) = scene.dimensions.first() else {
            panic!("missing point attachment")
        };
        assert_eq!(*start, Pos2::new(2.0, 3.0));
        assert_eq!(*end, Pos2::new(4.0, 5.0));
        assert_eq!(attachment_reference(&model, id).unwrap().0, hinge);
    }

    #[test]
    fn pp_angle_is_relative_to_carrier_guide() {
        let (mut model, body, guide, hinge) = rp_model();
        model.pairs[hinge.0].kind = PairKind::Prismatic;
        model.pairs[guide.0].state.fi = r("1");
        let mut scene = Scene::mechanism(&model);
        annotate(
            &mut scene,
            &model,
            &question(FieldPath::InputGuideAngle(body), Some("град")),
            "30",
            false,
        );
        let Some(Dimension::Angle { from, sweep, .. }) = scene.dimensions.first() else {
            panic!("missing guide comparison")
        };
        assert_eq!(*from, 1.0);
        assert!((*sweep - std::f32::consts::FRAC_PI_6).abs() < 0.000001);
        let ray = scene
            .vectors
            .iter()
            .find(|v| v.label.contains("конечное"))
            .unwrap();
        let d = ray.end - ray.start;
        assert!((d.y.atan2(d.x) - (1.0 + std::f32::consts::FRAC_PI_6)).abs() < 0.000001);
    }

    #[test]
    fn rpr_acute_question_uses_y_to_x_not_reverse_direction() {
        let (mut model, body, guide, hinge) = rp_model();
        let mut group = Group::new(GroupKind::Dyad, body);
        group.pair_x = Some(hinge);
        group.pair_y = Some(guide);
        group.more = Some(guide);
        group.input_type = b'3';
        model.groups.push(group);
        let mut scene = Scene::mechanism(&model);
        annotate(
            &mut scene,
            &model,
            &question(FieldPath::Assembly(GroupId(0)), None),
            "Y",
            false,
        );
        let vector = scene.vectors.iter().find(|v| v.label == "PR").unwrap();
        assert_eq!(vector.start, Pos2::new(100.0, 0.0));
        assert_eq!(vector.end, Pos2::new(2.0, 3.0));
    }

    #[test]
    fn incomplete_sign_question_shows_both_labeled_conditional_sides() {
        let (model, body, _, _) = rp_model();
        let mut scene = Scene::topology(&model);
        annotate(
            &mut scene,
            &model,
            &question(FieldPath::SignedOffset(body), None),
            "",
            true,
        );
        assert!(
            scene
                .annotations
                .iter()
                .any(|a| a.text.contains("Условно:") && a.text.contains("Да — по часовой"))
        );
        assert!(
            scene
                .annotations
                .iter()
                .any(|a| a.text.contains("Условно:") && a.text.contains("Нет — против часовой"))
        );
    }

    #[test]
    fn zero_loads_and_incomplete_angles_do_not_create_nonzero_glyphs() {
        let (mut model, body, guide, _) = rp_model();
        let point = model
            .add_point(body, PairKind::ForcePoint, Name(*b"F "))
            .unwrap();
        let mut scene = Scene::mechanism(&model);
        annotate(
            &mut scene,
            &model,
            &question(FieldPath::ForceMagnitude(point), Some("Н")),
            "0",
            false,
        );
        assert!(
            scene
                .loads
                .iter()
                .all(|load| load.key != ObjectKey::Pair(model.points[point.0].pair.0))
        );
        annotate(
            &mut scene,
            &model,
            &question(FieldPath::Moment(body), Some("Н·м")),
            "0",
            false,
        );
        assert!(scene.moments.is_empty());
        annotate(
            &mut scene,
            &model,
            &question(FieldPath::GroundAngle(guide), Some("град")),
            "-",
            false,
        );
        assert!(
            !scene
                .dimensions
                .iter()
                .any(|d| matches!(d, Dimension::Angle { .. }))
        );
    }
}
