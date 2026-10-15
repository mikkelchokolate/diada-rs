//! Deterministic screen-space layout. No values from this module enter the
//! mechanism or arithmetic; the constraints refer only to rendered pixels.
use eframe::egui::{Pos2, Rect, Vec2};

#[derive(Clone, Copy, Debug)]
pub enum Obstacle {
    Segment { a: Pos2, b: Pos2, clearance: f32 },
    Box(Rect),
}
impl Obstacle {
    pub fn intersects(self, rect: Rect) -> bool {
        match self {
            Self::Box(bounds) => bounds.intersects(rect),
            Self::Segment { a, b, clearance } => {
                let bounds = Rect::from_two_pos(a, b).expand(clearance);
                bounds.intersects(rect) && segment_intersects(a, b, rect.expand(clearance))
            }
        }
    }
}

#[derive(Clone, Debug)]
pub struct Request {
    pub anchor: Pos2,
    pub alternate_anchors: Vec<Pos2>,
    pub size: Vec2,
    pub preferred: Rect,
    pub priority: u16,
    pub force_leader: bool,
    /// Joint labels leave room around a glyph; shaft labels start on the shaft.
    pub leader_gap: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct Placement {
    pub rect: Rect,
    pub leader: Option<[Pos2; 3]>,
    pub collisions: usize,
}

/// Exact pixel inputs, rather than an approximate pose or a hash alone. The
/// bounded cache never carries a placement into different geometry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CacheKey {
    geometry: Vec<u32>,
    texts: Vec<String>,
}
impl CacheKey {
    pub fn new(
        clip: Rect,
        obstacles: &[Obstacle],
        requests: &[Request],
        texts: Vec<String>,
    ) -> Self {
        let mut geometry = Vec::new();
        let point = |p: Pos2| [p.x.to_bits(), p.y.to_bits()];
        geometry.extend(point(clip.min));
        geometry.extend(point(clip.max));
        geometry.push(obstacles.len() as u32);
        for obstacle in obstacles {
            match obstacle {
                Obstacle::Segment { a, b, clearance } => {
                    geometry.push(0);
                    geometry.extend(point(*a));
                    geometry.extend(point(*b));
                    geometry.push(clearance.to_bits());
                }
                Obstacle::Box(rect) => {
                    geometry.push(1);
                    geometry.extend(point(rect.min));
                    geometry.extend(point(rect.max));
                }
            }
        }
        geometry.push(requests.len() as u32);
        for request in requests {
            geometry.extend(point(request.anchor));
            geometry.extend([request.size.x.to_bits(), request.size.y.to_bits()]);
            geometry.extend(point(request.preferred.min));
            geometry.extend(point(request.preferred.max));
            geometry.extend([
                u32::from(request.priority),
                u32::from(request.force_leader),
                request.alternate_anchors.len() as u32,
                request.leader_gap.to_bits(),
            ]);
            for anchor in &request.alternate_anchors {
                geometry.extend(point(*anchor));
            }
        }
        Self { geometry, texts }
    }
}
#[derive(Clone, Default)]
pub struct Cache {
    entries: std::collections::VecDeque<(CacheKey, Vec<Placement>)>,
}
impl Cache {
    pub fn get(&mut self, key: &CacheKey) -> Option<Vec<Placement>> {
        let index = self.entries.iter().position(|(old, _)| old == key)?;
        let entry = self.entries.remove(index)?;
        let result = entry.1.clone();
        self.entries.push_back(entry);
        Some(result)
    }
    pub fn insert(&mut self, key: CacheKey, placements: Vec<Placement>) {
        if let Some(index) = self.entries.iter().position(|(old, _)| old == &key) {
            self.entries.remove(index);
        }
        if self.entries.len() >= 12 {
            self.entries.pop_front();
        }
        self.entries.push_back((key, placements));
    }
}

pub fn layout(clip: Rect, obstacles: &[Obstacle], requests: &[Request]) -> Vec<Placement> {
    let mut order = (0..requests.len()).collect::<Vec<_>>();
    order.sort_by(|a, b| {
        requests[*b]
            .priority
            .cmp(&requests[*a].priority)
            .then_with(|| {
                (requests[*b].size.x * requests[*b].size.y)
                    .total_cmp(&(requests[*a].size.x * requests[*a].size.y))
            })
            .then(a.cmp(b))
    });
    let mut placed = vec![None; requests.len()];
    for index in order {
        let request = &requests[index];
        let occupied = placed
            .iter()
            .flatten()
            .map(|p: &Placement| p.rect.expand(2.0))
            .collect::<Vec<_>>();
        let all_obstacles = obstacles
            .iter()
            .copied()
            .chain(
                placed
                    .iter()
                    .flatten()
                    .filter_map(|p: &Placement| p.leader)
                    .flat_map(|[a, b, c]| {
                        [
                            Obstacle::Segment {
                                a,
                                b,
                                clearance: 1.0,
                            },
                            Obstacle::Segment {
                                a: b,
                                b: c,
                                clearance: 1.0,
                            },
                        ]
                    }),
            )
            .collect::<Vec<_>>();
        placed[index] = Some(place(clip, &all_obstacles, &occupied, request));
    }
    placed.into_iter().flatten().collect()
}

fn place(clip: Rect, obstacles: &[Obstacle], occupied: &[Rect], request: &Request) -> Placement {
    let size = request.size.min(clip.size().max(Vec2::splat(1.0)));
    let bounded = |min: Pos2| {
        Rect::from_min_size(
            Pos2::new(
                min.x
                    .clamp(clip.left(), (clip.right() - size.x).max(clip.left())),
                min.y
                    .clamp(clip.top(), (clip.bottom() - size.y).max(clip.top())),
            ),
            size,
        )
    };
    let mut candidates = Vec::new();
    candidates.push((bounded(request.preferred.min), request.anchor));
    for anchor in std::iter::once(request.anchor).chain(request.alternate_anchors.iter().copied()) {
        for gap in [6.0, 10.0, 16.0, 24.0, 34.0, 48.0, 64.0, 84.0, 110.0, 145.0] {
            for min in [
                Pos2::new(anchor.x + gap, anchor.y - size.y * 0.5),
                Pos2::new(anchor.x + gap, anchor.y - gap - size.y),
                Pos2::new(anchor.x - size.x * 0.5, anchor.y - gap - size.y),
                Pos2::new(anchor.x - gap - size.x, anchor.y - gap - size.y),
                Pos2::new(anchor.x - gap - size.x, anchor.y - size.y * 0.5),
                Pos2::new(anchor.x - gap - size.x, anchor.y + gap),
                Pos2::new(anchor.x - size.x * 0.5, anchor.y + gap),
                Pos2::new(anchor.x + gap, anchor.y + gap),
            ] {
                candidates.push((bounded(min), anchor));
            }
            for direction in 0..24 {
                let angle = direction as f32 * std::f32::consts::TAU / 24.0;
                let d = Vec2::new(angle.cos(), angle.sin());
                let support = (size.x * d.x.abs() + size.y * d.y.abs()) * 0.5;
                candidates.push((bounded(anchor + d * (gap + support) - size * 0.5), anchor));
            }
        }
    }
    let mut best = None;
    let score_candidate = |rect: Rect, anchor: Pos2, limit: Option<(usize, f32)>| {
        let expanded = rect.expand(1.0);
        let text_hits = occupied
            .iter()
            .filter(|other| other.intersects(expanded))
            .count();
        if limit.is_some_and(|old| text_hits * 1000 > old.0) {
            return None;
        }
        let mut geometry_hits = 0;
        for obstacle in obstacles {
            if obstacle.intersects(expanded) {
                geometry_hits += 1;
                if limit.is_some_and(|old| text_hits * 1000 + geometry_hits * 100 > old.0) {
                    return None;
                }
            }
        }
        let primary_lower_bound = text_hits * 1000 + geometry_hits * 100;
        if limit.is_some_and(|old| primary_lower_bound > old.0) {
            return None;
        }
        let near = distance_to_rect(anchor, rect);
        let preference = rect.center().distance(request.preferred.center());
        let secondary_lower_bound = near * near + preference * 0.18;
        if limit.is_some_and(|old| primary_lower_bound == old.0 && secondary_lower_bound >= old.1) {
            return None;
        }
        let target = nearest(anchor, rect);
        let delta = target - anchor;
        let leader = (request.force_leader || near > 24.0).then(|| {
            let gap = if request.leader_gap.is_finite() {
                request.leader_gap.max(0.0)
            } else {
                7.0
            };
            let start = anchor + delta.normalized() * delta.length().min(gap);
            let mut route = [start, target, target];
            if route_text_hits(route, occupied) > 0 {
                for bend in [Pos2::new(start.x, target.y), Pos2::new(target.x, start.y)] {
                    let alternative = [start, bend, target];
                    if route_text_hits(alternative, occupied) < route_text_hits(route, occupied) {
                        route = alternative;
                    }
                }
            }
            route
        });
        let label_leader_hits = leader
            .map(|route| route_text_hits(route, occupied))
            .unwrap_or(0);
        let primary = primary_lower_bound + label_leader_hits * 1000;
        if limit.is_some_and(|old| {
            primary > old.0 || primary == old.0 && secondary_lower_bound >= old.1
        }) {
            return None;
        }
        let leader_hits = leader
            .map(|route| {
                obstacles
                    .iter()
                    .filter(|obstacle| {
                        route.windows(2).any(|p| match obstacle {
                            Obstacle::Segment { a: c, b: d, .. } => {
                                segments_cross(p[0], p[1], *c, *d)
                            }
                            Obstacle::Box(bounds) => segment_intersects(p[0], p[1], *bounds),
                        })
                    })
                    .count()
            })
            .unwrap_or(0);
        let score = (primary, secondary_lower_bound + leader_hits as f32 * 18.0);
        Some((
            score,
            Placement {
                rect,
                leader,
                collisions: text_hits + geometry_hits + label_leader_hits,
            },
        ))
    };
    for (rect, anchor) in candidates {
        if let Some(candidate) =
            score_candidate(rect, anchor, best.as_ref().map(|(score, _)| *score))
        {
            update_best(&mut best, candidate);
        }
    }
    let mut result = best.as_ref().unwrap().1;
    if result.collisions > 0 {
        // A crowded cluster can have a clear nearby wedge between the radial
        // candidates. Search the bounded viewport rather than silently using
        // the last colliding candidate.
        let step = if clip.width() * clip.height() < 50_000.0 {
            2.0
        } else {
            8.0
        };
        let mut y = clip.top();
        while y <= (clip.bottom() - size.y).max(clip.top()) {
            let mut x = clip.left();
            while x <= (clip.right() - size.x).max(clip.left()) {
                if let Some(candidate) = score_candidate(
                    Rect::from_min_size(Pos2::new(x, y), size),
                    request.anchor,
                    best.as_ref().map(|(score, _)| *score),
                ) {
                    update_best(&mut best, candidate);
                }
                x += step;
            }
            y += step;
        }
        result = best.unwrap().1;
    }
    result
}
fn update_best(best: &mut Option<((usize, f32), Placement)>, candidate: ((usize, f32), Placement)) {
    if best.as_ref().is_none_or(|(old, _)| {
        candidate.0.0 < old.0 || candidate.0.0 == old.0 && candidate.0.1 < old.1
    }) {
        *best = Some(candidate);
    }
}
fn route_text_hits(route: [Pos2; 3], occupied: &[Rect]) -> usize {
    let broad = [
        Rect::from_two_pos(route[0], route[1]),
        Rect::from_two_pos(route[1], route[2]),
    ];
    occupied
        .iter()
        .filter(|bounds| {
            route.windows(2).zip(broad).any(|(p, line_bounds)| {
                line_bounds.intersects(**bounds) && segment_intersects(p[0], p[1], **bounds)
            })
        })
        .count()
}

pub fn nearest(point: Pos2, rect: Rect) -> Pos2 {
    Pos2::new(
        point.x.clamp(rect.left(), rect.right()),
        point.y.clamp(rect.top(), rect.bottom()),
    )
}
fn distance_to_rect(point: Pos2, rect: Rect) -> f32 {
    point.distance(nearest(point, rect))
}

pub fn segment_intersects(a: Pos2, b: Pos2, rect: Rect) -> bool {
    let delta = b - a;
    let mut lo = 0.0_f32;
    let mut hi = 1.0_f32;
    for (p, q) in [
        (-delta.x, a.x - rect.left()),
        (delta.x, rect.right() - a.x),
        (-delta.y, a.y - rect.top()),
        (delta.y, rect.bottom() - a.y),
    ] {
        if p.abs() < 1e-6 {
            if q < 0.0 {
                return false;
            }
        } else {
            let t = q / p;
            if p < 0.0 {
                lo = lo.max(t);
            } else {
                hi = hi.min(t);
            }
            if lo > hi {
                return false;
            }
        }
    }
    true
}
fn segments_cross(a: Pos2, b: Pos2, c: Pos2, d: Pos2) -> bool {
    let cross = |x: Vec2, y: Vec2| x.x * y.y - x.y * y.x;
    let ab = b - a;
    let cd = d - c;
    let divisor = cross(ab, cd);
    if divisor.abs() < 1e-5 {
        return false;
    }
    let t = cross(c - a, cd) / divisor;
    let u = cross(c - a, ab) / divisor;
    (0.01..0.99).contains(&t) && (0.01..0.99).contains(&u)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn request(anchor: Pos2, size: Vec2) -> Request {
        Request {
            anchor,
            alternate_anchors: vec![],
            size,
            preferred: Rect::from_min_size(anchor + Vec2::new(6.0, -5.0), size),
            priority: 50,
            force_leader: false,
            leader_gap: 7.0,
        }
    }
    #[test]
    fn bounded_cache_reuses_only_identical_geometry_viewport_and_text() {
        let clip = Rect::from_min_max(Pos2::ZERO, Pos2::new(200.0, 150.0));
        let obstacles = [Obstacle::Segment {
            a: Pos2::new(0.0, 70.0),
            b: Pos2::new(200.0, 70.0),
            clearance: 3.0,
        }];
        let requests = [request(Pos2::new(80.0, 70.0), Vec2::new(20.0, 15.0))];
        let key = CacheKey::new(clip, &obstacles, &requests, vec!["A".into()]);
        let placements = layout(clip, &obstacles, &requests);
        let mut cache = Cache::default();
        cache.insert(key.clone(), placements.clone());
        assert_eq!(cache.get(&key).unwrap()[0].rect, placements[0].rect);
        assert!(
            cache
                .get(&CacheKey::new(
                    clip.translate(Vec2::new(1.0, 0.0)),
                    &obstacles,
                    &requests,
                    vec!["A".into()]
                ))
                .is_none()
        );
        let changed = [Obstacle::Segment {
            a: Pos2::new(0.0, 70.0),
            b: Pos2::new(200.0, 71.0),
            clearance: 3.0,
        }];
        assert!(
            cache
                .get(&CacheKey::new(clip, &changed, &requests, vec!["A".into()]))
                .is_none()
        );
        assert!(
            cache
                .get(&CacheKey::new(
                    clip,
                    &obstacles,
                    &requests,
                    vec!["B".into()]
                ))
                .is_none()
        );
        let mut selected = requests.clone();
        selected[0].priority += 1;
        assert!(
            cache
                .get(&CacheKey::new(
                    clip,
                    &obstacles,
                    &selected,
                    vec!["A".into()]
                ))
                .is_none()
        );
        let mut shaft_labels = requests.clone();
        shaft_labels[0].leader_gap = 0.0;
        assert!(
            cache
                .get(&CacheKey::new(
                    clip,
                    &obstacles,
                    &shaft_labels,
                    vec!["A".into()]
                ))
                .is_none()
        );
        for index in 0..12 {
            cache.insert(
                CacheKey::new(clip, &obstacles, &requests, vec![index.to_string()]),
                placements.clone(),
            );
        }
        assert_eq!(cache.entries.len(), 12);
        assert!(cache.get(&key).is_none());
        assert!(
            cache
                .get(&CacheKey::new(
                    clip,
                    &obstacles,
                    &requests,
                    vec!["11".into()]
                ))
                .is_some()
        );
    }
    #[test]
    fn nearby_labels_avoid_crossing_rods_and_one_another() {
        let clip = Rect::from_min_max(Pos2::ZERO, Pos2::new(280.0, 220.0));
        let anchor = Pos2::new(120.0, 110.0);
        let obstacles = [
            Obstacle::Segment {
                a: Pos2::new(0.0, 110.0),
                b: Pos2::new(280.0, 110.0),
                clearance: 3.0,
            },
            Obstacle::Segment {
                a: Pos2::new(120.0, 0.0),
                b: Pos2::new(120.0, 220.0),
                clearance: 3.0,
            },
            Obstacle::Box(Rect::from_center_size(anchor, Vec2::splat(11.0))),
        ];
        let requests = (0..6)
            .map(|_| request(anchor, Vec2::new(20.0, 14.0)))
            .collect::<Vec<_>>();
        let placements = layout(clip, &obstacles, &requests);
        for (index, p) in placements.iter().enumerate() {
            assert_eq!(p.collisions, 0);
            assert!(clip.contains_rect(p.rect));
            assert!(
                distance_to_rect(anchor, p.rect) < 65.0,
                "label too far: {p:?}"
            );
            assert!(obstacles.iter().all(|o| !o.intersects(p.rect.expand(1.0))));
            assert!(
                placements[..index]
                    .iter()
                    .all(|other| !other.rect.expand(2.0).intersects(p.rect))
            );
        }
        let again = layout(clip, &obstacles, &requests);
        assert!(placements.iter().zip(again).all(|(a, b)| a.rect == b.rect));
    }
    #[test]
    fn selected_label_has_priority_and_small_viewport_stays_bounded() {
        let clip = Rect::from_min_max(Pos2::ZERO, Pos2::new(95.0, 65.0));
        let anchor = Pos2::new(3.0, 4.0);
        let mut requests = vec![request(anchor, Vec2::new(23.0, 15.0)); 3];
        requests[2].priority = 200;
        let selected = layout(clip, &[], &requests)[2];
        let single = layout(clip, &[], &requests[2..])[0];
        assert_eq!(selected.rect, single.rect);
        assert!(
            layout(clip, &[], &requests)
                .iter()
                .all(|p| clip.contains_rect(p.rect))
        );
    }
    #[test]
    fn leader_routes_around_existing_text_only_when_needed() {
        let anchor = Pos2::new(20.0, 40.0);
        let blocker = Rect::from_min_max(Pos2::new(50.0, 30.0), Pos2::new(80.0, 50.0));
        let clip = Rect::from_min_max(Pos2::new(100.0, 5.0), Pos2::new(145.0, 75.0));
        let request = request(anchor, Vec2::new(20.0, 14.0));
        let p = place(clip, &[], &[blocker], &request);
        assert_eq!(p.collisions, 0);
        let route = p.leader.unwrap();
        assert!(
            route
                .windows(2)
                .all(|edge| !segment_intersects(edge[0], edge[1], blocker))
        );
        assert_eq!(route[2], nearest(anchor, p.rect));
        assert!((route[0].distance(anchor) - 7.0).abs() < 0.001);
        let nearby = place(
            Rect::from_min_max(Pos2::ZERO, Pos2::new(160.0, 100.0)),
            &[],
            &[],
            &request,
        );
        assert!(nearby.leader.is_none());
        assert!(distance_to_rect(anchor, nearby.rect) <= 10.0);
    }
    #[test]
    fn dense_component_cluster_and_moment_values_find_clear_space() {
        let clip = Rect::from_min_max(Pos2::ZERO, Pos2::new(315.0, 430.0));
        let anchor = Pos2::new(168.0, 222.0);
        let obstacles = (0..13)
            .map(|i| {
                let a = i as f32 * 0.17;
                Obstacle::Segment {
                    a: anchor - Vec2::new(a.cos(), a.sin()) * 120.0,
                    b: anchor + Vec2::new(a.cos(), a.sin()) * 100.0,
                    clearance: 2.8,
                }
            })
            .collect::<Vec<_>>();
        let mut requests = (0..13)
            .map(|i| {
                request(
                    anchor + Vec2::new((i % 3) as f32 * 3.0, (i % 4) as f32 * 2.0),
                    Vec2::new(18.0, 14.0),
                )
            })
            .collect::<Vec<_>>();
        let mut moment = request(anchor + Vec2::new(-60.0, -50.0), Vec2::new(77.0, 14.0));
        moment.priority = 85;
        requests.push(moment);
        let placements = layout(clip, &obstacles, &requests);
        assert!(
            placements.iter().all(|p| p.collisions == 0),
            "{placements:?}"
        );
    }
}
