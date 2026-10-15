//! A dimensionless incidence drawing. Its coordinates are never measurements.
//!
//! Link primary slots change when structural groups are accepted. Layout uses
//! the original pair incidence instead, so accepting a group cannot reshuffle
//! the graph or turn a multiple-joint rigid body into several unrelated rods.
use diada_core::{LinkId, Mechanism, Name, PairKind};
use eframe::egui::{Pos2, Vec2};
use std::cell::RefCell;

#[derive(PartialEq, Eq)]
struct Incidence {
    links: usize,
    pairs: Vec<(Name, PairKind, [LinkId; 2])>,
}

thread_local! {
    // Repaints, field hovers and accepted groups do not change incidence.
    // Retain only one drawing; all numerical model state is deliberately absent.
    static CACHE: RefCell<Option<(Incidence, Vec<Pos2>)>> = const { RefCell::new(None) };
}

/// One position per pair arena entry; named aliases share a physical joint.
pub fn positions(model: &Mechanism) -> Vec<Pos2> {
    let incidence = Incidence {
        links: model.links.len(),
        pairs: model
            .pairs
            .iter()
            .map(|pair| (pair.name, pair.kind, pair.links))
            .collect(),
    };
    CACHE.with(|cache| {
        if let Some((previous, positions)) = cache.borrow().as_ref()
            && *previous == incidence
        {
            return positions.clone();
        }
        let positions = layout(model);
        *cache.borrow_mut() = Some((incidence, positions.clone()));
        positions
    })
}

fn layout(model: &Mechanism) -> Vec<Pos2> {
    let mut representatives = Vec::<usize>::new();
    let mut indices = Vec::with_capacity(model.pairs.len());
    for (index, pair) in model.pairs.iter().enumerate() {
        let alias = !pair.name.display().is_empty()
            && matches!(pair.kind, PairKind::Revolute | PairKind::Prismatic);
        let joint = alias.then(|| {
            representatives.iter().position(|other| {
                let other = &model.pairs[*other];
                other.name == pair.name
                    && matches!(other.kind, PairKind::Revolute | PairKind::Prismatic)
            })
        });
        let joint = joint.flatten().unwrap_or_else(|| {
            representatives.push(index);
            representatives.len() - 1
        });
        indices.push(joint);
    }
    let count = representatives.len();
    if count == 0 {
        return Vec::new();
    }
    let mut bodies = vec![Vec::<usize>::new(); model.links.len()];
    for (index, pair) in model.pairs.iter().enumerate() {
        for id in pair.links {
            if let Some(body) = bodies.get_mut(id.0)
                && !body.contains(&indices[index])
            {
                body.push(indices[index]);
            }
        }
    }
    // A body's complete distance graph keeps three or more attachment points
    // noncollinear. Chord lengths also favour a polygon for larger bodies.
    let mut constraints = Vec::new();
    for body in &bodies {
        for (a, first) in body.iter().enumerate() {
            for (b, second) in body.iter().enumerate().skip(a + 1) {
                let chord = if body.len() > 2 {
                    let angle = std::f32::consts::PI / body.len() as f32;
                    (angle * (b - a) as f32).sin() / angle.sin()
                } else {
                    1.0
                };
                constraints.push((*first, *second, 0.26 * chord));
            }
        }
    }
    let radius = 0.2 * (count as f32).sqrt();
    let mut points: Vec<_> = (0..count)
        .map(|index| {
            let angle = std::f32::consts::TAU * index as f32 / count as f32;
            Vec2::angled(angle) * radius
        })
        .collect();
    if count > 1 {
        // Fixed iterations and arena order make hover/repaint deterministic.
        // This is intentionally a small drawing layout, not a mechanism solve.
        for iteration in 0..240 {
            let mut forces = vec![Vec2::ZERO; count];
            for a in 0..count {
                for b in a + 1..count {
                    let delta = points[b] - points[a];
                    let length = delta.length().max(0.001);
                    let force = delta / length * (0.0014 / (length * length));
                    forces[a] -= force;
                    forces[b] += force;
                }
            }
            for &(a, b, length) in &constraints {
                let delta = points[b] - points[a];
                let current = delta.length().max(0.001);
                let force = delta / current * ((current - length) * 1.8);
                forces[a] += force;
                forces[b] -= force;
            }
            let step = 0.08 * (1.0 - iteration as f32 / 300.0);
            for (point, force) in points.iter_mut().zip(forces) {
                let delta = force * step;
                *point += delta * (0.035 / delta.length().max(0.035));
            }
        }
        // Keep the first connection horizontal and the first joint at zero.
        // Neither the ground choice nor group ordering enters this convention.
        let origin = points[0];
        let next = constraints
            .iter()
            .filter_map(|&(a, b, _)| (a == 0).then_some(b))
            .min()
            .unwrap_or(1);
        let direction = points[next] - origin;
        let angle = direction.y.atan2(direction.x);
        let (sin, cos) = angle.sin_cos();
        for point in &mut points {
            let delta = *point - origin;
            *point = Vec2::new(
                cos * delta.x + sin * delta.y,
                -sin * delta.x + cos * delta.y,
            );
        }
        if points.iter().map(|point| point.y).sum::<f32>() < 0.0 {
            for point in &mut points {
                point.y = -point.y;
            }
        }
    } else {
        points[0] = Vec2::ZERO;
    }
    indices
        .into_iter()
        .map(|index| Pos2::ZERO + points[index])
        .collect()
}

/// One physical joint per body, with multiple-joint bodies closed as contours.
/// The returned indices still address the original pair arena for selection.
pub fn body_nodes(model: &Mechanism, link: usize, positions: &[Pos2]) -> Vec<usize> {
    let mut nodes = Vec::<usize>::new();
    for (index, pair) in model.pairs.iter().enumerate() {
        if pair.links.iter().any(|id| id.0 == link)
            && !nodes.iter().any(|other| {
                let other_pair = &model.pairs[*other];
                !pair.name.display().is_empty()
                    && matches!(pair.kind, PairKind::Revolute | PairKind::Prismatic)
                    && matches!(other_pair.kind, PairKind::Revolute | PairKind::Prismatic)
                    && pair.name == other_pair.name
            })
        {
            nodes.push(index);
        }
    }
    if nodes.len() >= 3 {
        let center = nodes
            .iter()
            .map(|index| positions[*index].to_vec2())
            .fold(Vec2::ZERO, |sum, point| sum + point)
            / nodes.len() as f32;
        nodes.sort_by(|a, b| {
            let a = positions[*a].to_vec2() - center;
            let b = positions[*b].to_vec2() - center;
            a.y.atan2(a.x).total_cmp(&b.y.atan2(b.x))
        });
        nodes.push(nodes[0]);
    }
    nodes
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drawing::{ObjectKey, Scene};

    fn branched() -> Mechanism {
        let mut model = Mechanism::default();
        for name in b'0'..=b'5' {
            model.add_link(Name([name, b' ']));
        }
        for (name, kind, a, b) in [
            (b'O', PairKind::Revolute, 0, 1),
            (b'A', PairKind::Revolute, 1, 2),
            (b'B', PairKind::Revolute, 2, 3),
            (b'C', PairKind::Prismatic, 3, 0),
            (b'D', PairKind::Revolute, 2, 4),
            (b'E', PairKind::Revolute, 4, 5),
            (b'F', PairKind::Prismatic, 5, 0),
        ] {
            model
                .add_pair(kind, Name([name, b' ']), [LinkId(a), LinkId(b)])
                .unwrap();
        }
        model
    }

    #[test]
    fn reused_body_is_a_single_closed_triangle_stable_across_group_entry() {
        let mut model = branched();
        let original = model.clone();
        let before = Scene::topology(&model);
        let p = positions(&model);
        assert_eq!(p[0], Pos2::ZERO);
        assert!((p[1].y).abs() < 1.0e-5);
        let ab = p[2] - p[1];
        let ad = p[4] - p[1];
        assert!((ab.x * ad.y - ab.y * ad.x).abs() > 0.02);
        for a in 0..p.len() {
            for b in a + 1..p.len() {
                assert!(p[a].distance(p[b]) > 0.12, "{a} and {b} overlap: {p:?}");
            }
        }
        let body = before
            .members
            .iter()
            .find(|member| member.key == ObjectKey::Link(2))
            .unwrap();
        assert_eq!(body.nodes.len(), 4);
        assert_eq!(body.nodes.first(), body.nodes.last());
        for node in [1, 2, 4] {
            assert!(body.nodes.contains(&node));
        }
        assert_eq!(
            model, original,
            "conditional drawing cannot edit numerical data"
        );
        model
            .set_ground_and_inputs(LinkId(0), vec![LinkId(1)])
            .unwrap();
        model.prepare_initial_groups().unwrap();
        assert_eq!(positions(&model), p);
        model.append_dyad(LinkId(2), LinkId(3)).unwrap();
        assert_eq!(positions(&model), p);
        model.append_dyad(LinkId(4), LinkId(5)).unwrap();
        assert_eq!(positions(&model), p);
        let after = Scene::topology(&model);
        assert_eq!(
            after
                .nodes
                .iter()
                .map(|node| node.position)
                .collect::<Vec<_>>(),
            p
        );
        for old in before
            .members
            .iter()
            .filter(|member| member.key != ObjectKey::Link(0))
        {
            let new = after
                .members
                .iter()
                .find(|member| member.key == old.key)
                .unwrap();
            assert_eq!(new.nodes, old.nodes);
        }
    }

    #[test]
    fn aliases_coincide_but_blank_auxiliary_points_are_independent() {
        let mut model = branched();
        model.pairs.push(model.pairs[1].clone());
        model.pairs[7].links = [LinkId(3), LinkId(4)];
        for kind in [PairKind::MassCenter, PairKind::ForcePoint, PairKind::Point] {
            let mut point = model.pairs[0].clone();
            point.kind = kind;
            point.name = Name::BLANK;
            point.links = [LinkId(2), LinkId(2)];
            model.pairs.push(point);
        }
        let p = positions(&model);
        assert_eq!(p[1], p[7]);
        for a in 8..11 {
            assert!(p[a].is_finite());
            for b in a + 1..11 {
                assert!(p[a].distance(p[b]) > 0.08);
            }
        }
        let body = body_nodes(&model, 3, &p);
        assert!(
            body.contains(&7),
            "alias incidence still belongs to its additional body"
        );
    }

    #[test]
    fn empty_single_disconnected_and_dense_incomplete_graphs_remain_finite() {
        assert!(positions(&Mechanism::default()).is_empty());
        let mut model = branched();
        model.pairs.truncate(1);
        assert_eq!(positions(&model), vec![Pos2::ZERO]);
        for index in 1..24 {
            let mut pair = model.pairs[0].clone();
            pair.name = Name([b'Q', b'A' + index]);
            pair.links = [LinkId(index as usize % 6), LinkId((index as usize + 2) % 6)];
            model.pairs.push(pair);
        }
        let p = positions(&model);
        assert!(
            p.iter()
                .all(|point| point.is_finite() && point.to_vec2().length() < 10.0)
        );
        for a in 0..p.len() {
            for b in a + 1..p.len() {
                assert!(p[a].distance(p[b]) > 0.025);
            }
        }
        assert_eq!(positions(&model), p);
    }

    #[test]
    fn a_large_multiple_joint_body_keeps_distinct_finite_polygon_vertices() {
        let mut model = branched();
        model.pairs.clear();
        for index in 0..64 {
            let mut pair = branched().pairs[0].clone();
            pair.name = Name([b'A' + index / 26, b'A' + index % 26]);
            pair.links = [LinkId(0), LinkId(1 + index as usize % 5)];
            model.pairs.push(pair);
        }
        let start = std::time::Instant::now();
        let p = positions(&model);
        eprintln!("64-joint schematic layout: {:?}", start.elapsed());
        let repeated = std::time::Instant::now();
        assert_eq!(positions(&model), p);
        eprintln!("64-joint unchanged incidence: {:?}", repeated.elapsed());
        assert!(
            p.iter()
                .all(|point| point.is_finite() && point.to_vec2().length() < 10.0)
        );
        for a in 0..p.len() {
            for b in a + 1..p.len() {
                assert!(p[a].distance(p[b]) > 0.08);
            }
        }
        let body = body_nodes(&model, 0, &p);
        assert_eq!(body.len(), 65);
        assert_eq!(body.first(), body.last());
    }

    #[test]
    fn cached_layout_tracks_changed_incidence_but_never_depends_on_geometry() {
        let mut model = branched();
        let original = positions(&model);
        let one = diada_compat::parse_real(b"1").unwrap();
        for pair in &mut model.pairs {
            pair.state.x = one;
            pair.state.y = one;
        }
        assert_eq!(positions(&model), original);
        model.pairs[4].links = [LinkId(1), LinkId(4)];
        let moved = positions(&model);
        assert_ne!(moved, original);
        assert_eq!(moved, layout(&model));
        model.pairs[4].links = [LinkId(2), LinkId(4)];
        assert_eq!(positions(&model), original);
    }
}
