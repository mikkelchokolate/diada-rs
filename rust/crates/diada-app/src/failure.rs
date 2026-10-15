//! Defined geometry after a failed position. Arena indices remain stable so
//! selection keys cannot accidentally identify a different pair/member.
use crate::drawing::{ObjectKey, Scene};
use diada_core::{CoreError, GroupKind, LinkId, Mechanism, PairId};
use std::collections::HashSet;

pub fn preview(
    model: &Mechanism,
    error: &CoreError,
    kinematics_valid: bool,
    motion: bool,
) -> Scene {
    if kinematics_valid {
        return if motion {
            Scene::motion(model)
        } else {
            Scene::mechanism(model)
        };
    }
    let stop = match error {
        CoreError::Unassemblable { group: Some(group) }
        | CoreError::UndefinedPosition {
            group: Some(group), ..
        } => Some(group.0),
        _ => None,
    };
    let mut visible_links = HashSet::<LinkId>::new();
    let mut visible_pairs = HashSet::<PairId>::new();
    if let Some(ground) = model.ground {
        visible_links.insert(ground);
        visible_pairs.extend(
            model
                .pairs
                .iter()
                .enumerate()
                .filter(|(_, pair)| pair.links.contains(&ground))
                .map(|(index, _)| PairId(index)),
        );
    }
    let mut defined = model.clone();
    defined.groups = model
        .groups
        .iter()
        .enumerate()
        .filter(|(index, group)| {
            group.kind == GroupKind::Ground || stop.is_some_and(|stop| *index < stop)
        })
        .map(|(_, group)| group.clone())
        .collect();
    for group in &defined.groups {
        visible_links.insert(group.link_a);
        visible_links.extend(group.link_b);
        visible_pairs.extend(
            [group.pair_x, group.pair_y, group.more]
                .into_iter()
                .flatten(),
        );
    }
    // Remove undefined attachments before constructing owner rays. Keeping
    // only visible node positions afterward would be insufficient: an old
    // point coordinate can coincide with a valid ground coordinate.
    let mut point_map = vec![None; model.points.len()];
    let mut next_point = 0;
    defined.points = model
        .points
        .iter()
        .enumerate()
        .filter_map(|(index, point)| {
            if defined
                .groups
                .iter()
                .any(|group| group.attached_point == Some(diada_core::PointId(index)))
            {
                point_map[index] = Some(diada_core::PointId(next_point));
                next_point += 1;
                Some(point.clone())
            } else {
                None
            }
        })
        .collect();
    for group in &mut defined.groups {
        group.attached_point = group
            .attached_point
            .and_then(|id| point_map.get(id.0).copied().flatten());
    }
    for link in &mut defined.links {
        if let Some(mass) = &mut link.mass {
            mass.center = mass
                .center
                .and_then(|id| point_map.get(id.0).copied().flatten());
        }
    }
    // Primary endpoints of a completed body are known even if one record is
    // an alias created later in the structure traversal.
    for link in &visible_links {
        if let Some(link) = defined.link(*link) {
            visible_pairs.extend(link.pairs.iter().copied().flatten());
        }
    }
    let mut scene = if motion {
        Scene::motion(&defined)
    } else {
        Scene::mechanism(&defined)
    };
    scene.members.retain(
        |member| matches!(member.key,ObjectKey::Link(id) if visible_links.contains(&LinkId(id))),
    );
    scene.paths.retain(
        |path| matches!(path.key,ObjectKey::Link(id) if visible_links.contains(&LinkId(id))),
    );
    for (index, node) in scene.nodes.iter_mut().enumerate() {
        if !visible_pairs.contains(&PairId(index)) {
            node.position = eframe::egui::Pos2::new(f32::NAN, f32::NAN);
            node.name.clear();
        }
    }
    scene.vectors.clear();
    scene.loads.clear();
    scene.moments.clear();
    scene.annotations.clear();
    scene.dimensions.clear();
    scene.rigid_angles.retain(
        |angle| matches!(angle.key,ObjectKey::Pair(id) if visible_pairs.contains(&PairId(id))),
    );
    scene.trajectories.clear();
    scene
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> Mechanism {
        let loaded = diada_io::load_dialogue(
            &diada_io::Journal::from_bytes(include_bytes!(
                "../../../../tests/fixtures/diada-v320-derived/SCENE.DI"
            )),
            "SCENE",
            &diada_io::TpNumbers,
        )
        .unwrap();
        let fields = crate::analysis::InputFields::for_model(&loaded.mechanism);
        let config = crate::analysis::config(&fields, 1, false).unwrap();
        diada_core::KinematicSampler::new(&loaded.mechanism, &config)
            .unwrap()
            .next_frame()
            .unwrap()
            .unwrap()
            .mechanism
    }
    #[test]
    fn later_failure_keeps_completed_dyad_and_hides_next_body_and_points() {
        let model = fixture();
        let dyads = model
            .groups
            .iter()
            .enumerate()
            .filter(|(_, g)| g.kind == GroupKind::Dyad)
            .collect::<Vec<_>>();
        assert_eq!(dyads.len(), 2);
        let first = dyads[0].1;
        let (index, last) = dyads[1];
        let scene = preview(
            &model,
            &CoreError::Unassemblable {
                group: Some(diada_core::GroupId(index)),
            },
            false,
            false,
        );
        assert!(
            scene
                .members
                .iter()
                .any(|m| m.key == ObjectKey::Link(first.link_a.0))
        );
        assert!(
            !scene
                .members
                .iter()
                .any(|m| m.key == ObjectKey::Link(last.link_a.0))
        );
        assert!(!scene.nodes[last.more.unwrap().0].position.x.is_finite());
        assert!(scene.nodes[first.more.unwrap().0].position.x.is_finite());
        assert!(scene.loads.is_empty() && scene.moments.is_empty() && scene.vectors.is_empty());
    }
    #[test]
    fn unknown_numeric_failure_only_exposes_ground_and_force_failure_keeps_defined_m() {
        let model = fixture();
        let error = CoreError::InvalidAnalysis("test failure without known group");
        let scene = preview(&model, &error, false, false);
        assert!(scene.members.is_empty() && scene.paths.is_empty());
        let full = preview(&model, &error, true, true);
        assert_eq!(full.paths.len(), Scene::motion(&model).paths.len());
        assert!(!full.mask_sliders);
    }
}
