//! Presentation geometry. Exact stored states are converted only at the final
//! drawing boundary; screen coordinates never feed calculations back.
use diada_core::{CoreError, GroupKind, LinkId, Mechanism, PairId, PairKind, Real48};
use eframe::egui::{self, Color32, FontId, Pos2, Rect, Shape, Stroke, Vec2};
#[path = "label_layout.rs"]
mod label_layout;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ObjectKey {
    Pair(usize),
    Link(usize),
    /// Auxiliary points currently share the stable pair arena selection key.
    #[allow(dead_code)]
    Point(usize),
}
pub struct SceneInteraction {
    pub clicked: bool,
    pub object: Option<ObjectKey>,
}
#[derive(Clone, Debug)]
pub struct Node {
    pub key: ObjectKey,
    pub name: String,
    pub position: Pos2,
    pub kind: NodeKind,
}
#[derive(Clone, Copy, Debug)]
pub enum NodeKind {
    Hinge { grounded: bool },
    Slider { angle: f32, grounded: bool },
    Point { mass: bool, force: bool },
}
#[derive(Clone, Debug)]
pub struct Member {
    pub key: ObjectKey,
    pub name: String,
    pub nodes: Vec<usize>,
}
#[derive(Clone, Debug)]
pub struct MemberPath {
    pub key: ObjectKey,
    pub points: Vec<Pos2>,
    pub dashed: bool,
}
#[derive(Clone, Debug, Default)]
pub struct Scene {
    pub nodes: Vec<Node>,
    pub members: Vec<Member>,
    pub paths: Vec<MemberPath>,
    pub vectors: Vec<Vector>,
    pub dimensions: Vec<Dimension>,
    pub trajectories: Vec<Vec<Pos2>>,
    /// Original U01 MotPutPixel trace: sampled points, without connecting lines.
    pub trace_dots: Vec<Pos2>,
    pub loads: Vec<LoadGlyph>,
    pub moments: Vec<MomentGlyph>,
    /// U02's six-pixel sector marks a rigid auxiliary attachment.
    pub rigid_angles: Vec<RigidAngle>,
    pub annotations: Vec<Annotation>,
    pub units: Option<&'static str>,
    /// PlaK masks body lines; PlaL and the motion MotH keep only a contour.
    pub mask_sliders: bool,
    pub outlined_sliders: Vec<ObjectKey>,
}

#[derive(Clone, Debug)]
pub struct Annotation {
    pub position: Pos2,
    pub text: String,
}

#[derive(Clone, Debug)]
pub struct RigidAngle {
    pub key: ObjectKey,
    pub position: Pos2,
    pub from: f32,
    pub sweep: f32,
}

/// User-approved modern annotation. This is not a claimed original U02 glyph.
#[derive(Clone, Debug)]
pub struct MomentGlyph {
    pub key: ObjectKey,
    pub position: Pos2,
    pub negative: bool,
    pub label: String,
}

#[derive(Clone, Debug)]
pub struct LoadGlyph {
    pub key: ObjectKey,
    pub position: Pos2,
    pub kind: LoadKind,
}
#[derive(Clone, Copy, Debug)]
pub enum LoadKind {
    Weight,
    Force { angle: f32 },
}
#[derive(Clone, Debug)]
pub struct Vector {
    pub key: ObjectKey,
    pub start: Pos2,
    pub end: Pos2,
    pub label: String,
    pub label_kind: VectorLabelKind,
    pub color: Color32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VectorLabelKind {
    /// Point names, including carried construction points, belong to vertices.
    Endpoint,
    /// Normal, tangential and Coriolis components belong to their own shaft.
    Component,
}
#[derive(Clone, Debug)]
pub enum Dimension {
    Length {
        start: Pos2,
        end: Pos2,
        label: String,
    },
    Angle {
        center: Pos2,
        from: f32,
        sweep: f32,
        label: String,
    },
}
#[derive(Clone, Debug)]
pub struct View {
    pub zoom: f32,
    pub pan: Vec2,
    pub grid: bool,
    pub selected: Option<ObjectKey>,
    pub highlighted: Vec<ObjectKey>,
    pub fixed_bounds: Option<Rect>,
    /// Suggested extent of the latest series, applied by the explicit Fit action.
    pub fit_bounds: Option<Rect>,
}
impl Default for View {
    fn default() -> Self {
        Self {
            zoom: 1.0,
            pan: Vec2::ZERO,
            grid: false,
            selected: None,
            highlighted: Vec::new(),
            fixed_bounds: None,
            fit_bounds: None,
        }
    }
}

impl Scene {
    /// Explicitly schematic while dimensions are incomplete.
    pub fn topology(model: &Mechanism) -> Self {
        let mut scene = Self::mechanism(model);
        scene.paths.clear();
        let positions = crate::schematic_layout::positions(model);
        for (node, position) in scene.nodes.iter_mut().zip(&positions) {
            node.position = *position;
            if let NodeKind::Slider { angle, .. } = &mut node.kind {
                *angle = 0.0;
            }
        }
        for member in &mut scene.members {
            if let ObjectKey::Link(link) = member.key {
                member.nodes = crate::schematic_layout::body_nodes(model, link, &positions);
            }
        }
        for load in &mut scene.loads {
            if let Some(node) = scene.nodes.iter().find(|node| node.key == load.key) {
                load.position = node.position;
            }
        }
        scene.moments.clear();
        scene.rigid_angles.clear();
        scene.units = None;
        scene
    }
    pub fn motion(model: &Mechanism) -> Self {
        let mut scene = Self::mechanism(model);
        scene.mask_sliders = false;
        scene
    }
    pub fn mechanism(model: &Mechanism) -> Self {
        let nodes = model
            .pairs
            .iter()
            .enumerate()
            .map(|(index, pair)| {
                let grounded = model
                    .ground
                    .is_some_and(|ground| pair.links.contains(&ground));
                let kind = match pair.kind {
                    PairKind::Revolute => NodeKind::Hinge { grounded },
                    PairKind::Prismatic => NodeKind::Slider {
                        angle: display(pair.state.fi),
                        grounded,
                    },
                    _ => NodeKind::Point {
                        mass: pair.kind == PairKind::MassCenter,
                        force: pair.kind == PairKind::ForcePoint,
                    },
                };
                Node {
                    key: ObjectKey::Pair(index),
                    name: pair.name.display(),
                    position: position([pair.state.x, pair.state.y]),
                    kind,
                }
            })
            .collect();
        let members = model
            .links
            .iter()
            .enumerate()
            .filter(|(index, _)| model.ground != Some(LinkId(*index)))
            .map(|(index, link)| Member {
                key: ObjectKey::Link(index),
                name: link.name.display(),
                nodes: link
                    .pairs
                    .iter()
                    .copied()
                    .flatten()
                    .chain(link.extra_pairs.iter().copied())
                    .map(|pair| pair.0)
                    .collect(),
            })
            .collect();
        let mut scene = Self {
            nodes,
            members,
            units: Some("м"),
            mask_sliders: true,
            ..Self::default()
        };
        scene.group_geometry(model);
        for (index, pair) in model.pairs.iter().enumerate() {
            let kind = match pair.kind {
                PairKind::MassCenter
                    if model
                        .points
                        .iter()
                        .find(|point| point.pair == PairId(index))
                        .and_then(|point| model.link(point.owner))
                        .and_then(|link| link.mass.as_ref())
                        .is_some_and(|mass| !mass.mass.is_zero()) =>
                {
                    Some(LoadKind::Weight)
                }
                PairKind::ForcePoint if !pair.state.rx.is_zero() => Some(LoadKind::Force {
                    angle: display(pair.state.ry),
                }),
                _ => None,
            };
            if let Some(kind) = kind {
                scene.loads.push(LoadGlyph {
                    key: ObjectKey::Pair(index),
                    position: position([pair.state.x, pair.state.y]),
                    kind,
                });
            }
        }
        for (index, link) in model.links.iter().enumerate() {
            let Some(value) = link.applied_moment.filter(|value| !value.is_zero()) else {
                continue;
            };
            let at = link
                .mass
                .as_ref()
                .and_then(|mass| mass.center)
                .and_then(|id| model.point(id))
                .and_then(|point| model.pair(point.pair))
                .map(|pair| position([pair.state.x, pair.state.y]))
                .or_else(|| {
                    scene
                        .paths
                        .iter()
                        .find(|path| path.key == ObjectKey::Link(index))
                        .and_then(|path| {
                            path.points.windows(2).max_by(|a, b| {
                                a[0].distance_sq(a[1]).total_cmp(&b[0].distance_sq(b[1]))
                            })
                        })
                        .map(|edge| edge[0].lerp(edge[1], 0.5))
                });
            if let Some(at) = at {
                let label = diada_compat::format_real(value, 0, Some(3))
                    .map(|text| crate::ui_numbers::compact(&text))
                    .unwrap_or_else(|error| error.to_string());
                scene.moments.push(MomentGlyph {
                    key: ObjectKey::Link(index),
                    position: at,
                    negative: value.to_le_bytes()[5] & 0x80 != 0,
                    label: format!("{label} Н·м"),
                });
            }
        }
        scene
    }
    fn group_geometry(&mut self, model: &Mechanism) {
        let foot = |id: LinkId| {
            model
                .link(id)
                .map(|link| position([link.geometry.origin_x, link.geometry.origin_y]))
        };
        let set = |scene: &mut Self, pair: Option<PairId>, at: Option<Pos2>| {
            if let (Some(pair), Some(at)) = (pair, at)
                && let Some(node) = scene.nodes.get_mut(pair.0)
            {
                node.position = at;
            }
        };
        // A P coordinate is a reference on the carrier. The visible relative
        // body can be elsewhere on that line. U02 ShowPlans places it at the
        // selected member foot; the exact numeric model is never changed.
        for group in &model.groups {
            if group.kind == GroupKind::Dyad
                && matches!(group.input_type, b'3' | b'5')
                && let Some(pair) = group.more
            {
                self.outlined_sliders.push(ObjectKey::Pair(pair.0));
            }
            match (group.kind, group.input_type) {
                (GroupKind::Input, b'1') => set(self, group.pair_x, foot(group.link_a)),
                (GroupKind::Input, b'3') => {
                    let at = group
                        .more
                        .and_then(|id| model.pair(id))
                        .map(|pair| position([pair.state.x, pair.state.y]));
                    set(self, group.pair_x, at);
                }
                (GroupKind::Dyad, b'2' | b'4') => {
                    for (pair, link) in [
                        (group.pair_x, Some(group.link_a)),
                        (group.pair_y, group.link_b),
                    ] {
                        if let (Some(pair), Some(link)) = (pair, link)
                            && model
                                .pair(pair)
                                .is_some_and(|p| p.kind == PairKind::Prismatic)
                        {
                            set(self, Some(pair), foot(link));
                        }
                    }
                }
                (GroupKind::Dyad, b'3') => set(self, group.more, foot(group.link_a)),
                (GroupKind::Dyad, b'5') => {
                    for link in [Some(group.link_a), group.link_b].into_iter().flatten() {
                        if let Some(pair) = model.link(link).and_then(|l| l.pairs[0]) {
                            if model
                                .pair(pair)
                                .is_some_and(|p| p.kind == PairKind::Revolute)
                            {
                                set(self, group.more, foot(link));
                            } else {
                                set(self, Some(pair), foot(link));
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        for member in &self.members {
            let nodes: Vec<_> = member
                .nodes
                .iter()
                .filter_map(|i| self.nodes.get(*i))
                .collect();
            if nodes.len() < 2 {
                continue;
            }
            self.paths.push(MemberPath {
                key: member.key,
                points: vec![nodes[0].position, nodes[1].position],
                dashed: false,
            });
        }
        for group in model
            .groups
            .iter()
            .filter(|g| g.kind == GroupKind::Dyad && matches!(g.input_type, b'3' | b'5'))
        {
            let Some(output) = group
                .more
                .and_then(|p| self.nodes.get(p.0))
                .map(|n| n.position)
            else {
                continue;
            };
            for link in [Some(group.link_a), group.link_b].into_iter().flatten() {
                let Some(base) = model
                    .link(link)
                    .and_then(|l| l.pairs[0])
                    .and_then(|p| self.nodes.get(p.0))
                    .map(|n| n.position)
                else {
                    continue;
                };
                let Some(at) = foot(link) else {
                    continue;
                };
                let key = ObjectKey::Link(link.0);
                self.paths.retain(|path| path.key != key);
                self.paths.push(MemberPath {
                    key,
                    points: vec![base, at, output],
                    dashed: false,
                });
            }
        }
        // Auxiliary T/M/F records live outside Link.extra_pairs. The same
        // reference selection as CalculateAttached gives their rigid owner ray.
        for (index, point) in model.points.iter().enumerate() {
            // Extra ground joints define fixed supports, not an additional
            // rod from the ground's primary reference to that support.
            if model.ground == Some(point.owner) {
                continue;
            }
            let Some(link) = model.link(point.owner) else {
                continue;
            };
            let typ = model
                .groups
                .iter()
                .find(|g| g.attached_point == Some(diada_core::PointId(index)))
                .map(|g| g.input_type)
                .unwrap_or(b'0');
            let slot = attachment_slot(model, point.owner, typ);
            let Some(reference) = link.pairs[slot].and_then(|id| model.pair(id)) else {
                continue;
            };
            let Some(pair) = model.pair(point.pair) else {
                continue;
            };
            self.paths.push(MemberPath {
                key: ObjectKey::Link(point.owner.0),
                points: vec![
                    position([reference.state.x, reference.state.y]),
                    position([pair.state.x, pair.state.y]),
                ],
                dashed: false,
            });
            if typ == b'0'
                && let Some(other) = link.pairs[1].and_then(|id| model.pair(id))
            {
                self.paths.push(MemberPath {
                    key: ObjectKey::Link(point.owner.0),
                    points: vec![
                        position([other.state.x, other.state.y]),
                        position([pair.state.x, pair.state.y]),
                    ],
                    dashed: false,
                });
            }
            if !point.geometry.parameter.is_zero() {
                self.rigid_angles.push(RigidAngle {
                    key: ObjectKey::Pair(point.pair.0),
                    position: position([reference.state.x, reference.state.y]),
                    from: display(link.geometry.angle),
                    sweep: display(point.geometry.angle),
                });
            }
        }
    }

    pub fn plan(model: &Mechanism, acceleration: bool) -> Result<Self, CoreError> {
        let mut scene = Self {
            units: Some(if acceleration { "м/с²" } else { "м/с" }),
            ..Self::default()
        };
        let color = if acceleration {
            Color32::from_rgb(182, 61, 49)
        } else {
            Color32::from_rgb(33, 122, 170)
        };
        scene.nodes.push(Node {
            key: ObjectKey::Pair(usize::MAX),
            name: if acceleration { "πa" } else { "πv" }.into(),
            position: Pos2::ZERO,
            kind: NodeKind::Point {
                mass: false,
                force: false,
            },
        });
        let mut endpoints = Vec::new();
        for (index, pair) in model.pairs.iter().enumerate() {
            let motion = if acceleration {
                [pair.state.acceleration, pair.state.acceleration_angle]
            } else {
                [pair.state.speed, pair.state.speed_angle]
            };
            let xy = polar(motion)?;
            endpoints.push(xy);
            // A shared P connects two bodies: its one stored pair state is not
            // the end of both construction polygons. U02 draws the distinct
            // group intermediates for types 3/5 instead (branches 4068/4674).
            let shared_p = model.groups.iter().any(|group| {
                (group.kind == GroupKind::Dyad && matches!(group.input_type, b'3' | b'5')
                    || group.kind == GroupKind::Attached
                        && matches!(group.input_type, b'3' | b'4' | b'5'))
                    && group.more == Some(PairId(index))
            });
            if shared_p || motion[0].is_zero() {
                continue;
            }
            let at = position(xy);
            let name = pair.name.display().to_lowercase();
            if scene
                .nodes
                .iter()
                .any(|node| node.name == name && node.position == at)
            {
                continue;
            }
            scene.nodes.push(Node {
                key: ObjectKey::Pair(index),
                name,
                position: at,
                kind: NodeKind::Point {
                    mass: false,
                    force: false,
                },
            });
            if !motion[0].is_zero() {
                scene.vectors.push(Vector {
                    key: ObjectKey::Pair(index),
                    start: Pos2::ZERO,
                    end: at,
                    label: String::new(),
                    label_kind: VectorLabelKind::Endpoint,
                    color,
                });
            }
        }
        // U02's T records include additional R/P pairs, ordinary points,
        // centres of mass and force application points. A rigid RR attachment
        // closes to both primary endpoints; RP/PP chooses CalculateAttached's
        // reference. The glyph itself is already represented above.
        for group in model
            .groups
            .iter()
            .filter(|g| g.kind == GroupKind::Attached && matches!(g.input_type, b'0' | b'1' | b'2'))
        {
            let Some(point) = group.more.and_then(|id| endpoints.get(id.0)).copied() else {
                continue;
            };
            let owner = group.link_b.unwrap_or(group.link_a);
            let Some(link) = model.link(owner) else {
                continue;
            };
            let reference = attachment_slot(model, owner, group.input_type);
            let slots: &[usize] = if group.input_type == b'0' {
                &[0, 1]
            } else if reference == 0 {
                &[0]
            } else {
                &[1]
            };
            for &slot in slots {
                if let Some(base) = link.pairs[slot].and_then(|id| endpoints.get(id.0)) {
                    scene.vectors.push(Vector {
                        key: ObjectKey::Link(owner.0),
                        start: position(point),
                        end: position(*base),
                        label: String::new(),
                        label_kind: VectorLabelKind::Endpoint,
                        color,
                    });
                }
            }
        }
        for (index, link) in model
            .links
            .iter()
            .enumerate()
            .filter(|(i, _)| model.ground != Some(LinkId(*i)))
        {
            if model.groups.iter().any(|group| {
                group.kind == GroupKind::Dyad
                    && matches!(group.input_type, b'3' | b'5')
                    && (group.link_a == LinkId(index) || group.link_b == Some(LinkId(index)))
            }) {
                continue;
            }
            if let (Some(a), Some(b)) = (link.pairs[0], link.pairs[1]) {
                scene.vectors.push(Vector {
                    key: ObjectKey::Link(index),
                    start: position(endpoints[a.0]),
                    end: position(endpoints[b.0]),
                    label: String::new(),
                    label_kind: VectorLabelKind::Endpoint,
                    color,
                });
            }
        }
        for group in &model.groups {
            let Some(more) = group.more.and_then(|id| endpoints.get(id.0)).copied() else {
                continue;
            };
            let links = [Some(group.link_a), group.link_b];
            let base = |index: usize| {
                links[index]
                    .and_then(|id| model.link(id))
                    .and_then(|l| l.pairs[0])
                    .and_then(|id| endpoints.get(id.0))
                    .copied()
            };
            let revolute_outer = || {
                [group.pair_x, group.pair_y]
                    .into_iter()
                    .flatten()
                    .find(|id| {
                        model
                            .pair(*id)
                            .is_some_and(|pair| pair.kind == PairKind::Revolute)
                    })
                    .and_then(|id| endpoints.get(id.0))
                    .copied()
            };
            let key = |index: usize| ObjectKey::Link(links[index].unwrap_or(group.link_a).0);
            let intermediate =
                |scene: &mut Self, index: usize, slot: usize| -> Result<[Real48; 2], CoreError> {
                    let at = polar([group.values[slot], group.values[slot + 1]])?;
                    if !group.values[slot].is_zero() {
                        scene.vectors.push(Vector {
                            key: key(index),
                            start: Pos2::ZERO,
                            end: position(at),
                            label: intermediate_name(model, group, index, acceleration),
                            label_kind: VectorLabelKind::Endpoint,
                            color,
                        });
                    }
                    Ok(at)
                };
            let component = |scene: &mut Self,
                             index: usize,
                             start: [Real48; 2],
                             slot: usize,
                             label: &str|
             -> Result<[Real48; 2], CoreError> {
                if group.values[slot].is_zero() {
                    return Ok(start);
                }
                let end = add(start, polar([group.values[slot], group.values[slot + 1]])?)?;
                scene.vectors.push(Vector {
                    key: key(index),
                    start: position(start),
                    end: position(end),
                    label: label.into(),
                    label_kind: VectorLabelKind::Component,
                    color,
                });
                Ok(end)
            };
            let finish = |scene: &mut Self, index: usize, start: [Real48; 2], label: &str| {
                scene.vectors.push(Vector {
                    key: key(index),
                    start: position(start),
                    end: position(more),
                    label: label.into(),
                    label_kind: VectorLabelKind::Component,
                    color,
                })
            };
            let connect = |scene: &mut Self, index: usize, a: [Real48; 2], b: [Real48; 2]| {
                scene.vectors.push(Vector {
                    key: key(index),
                    start: position(a),
                    end: position(b),
                    label: String::new(),
                    label_kind: VectorLabelKind::Endpoint,
                    color,
                });
            };
            if !acceleration {
                if group.kind == GroupKind::Dyad
                    && matches!(group.input_type, b'2' | b'3' | b'4' | b'5')
                {
                    let slots = if group.input_type == b'2' {
                        if group
                            .pair_x
                            .and_then(|id| model.pair(id))
                            .is_some_and(|p| p.kind == PairKind::Prismatic)
                        {
                            vec![(0, 0)]
                        } else {
                            vec![(1, 2)]
                        }
                    } else if group.input_type == b'3' {
                        vec![(0, 0)]
                    } else {
                        vec![(0, 0), (1, 2)]
                    };
                    let mut previous = None;
                    for (index, slot) in slots {
                        let at = intermediate(&mut scene, index, slot)?;
                        if matches!(group.input_type, b'3' | b'5') {
                            if group.input_type == b'3' {
                                for index in 0..2 {
                                    if let Some(base) = base(index) {
                                        connect(&mut scene, index, base, at);
                                    }
                                }
                            } else if let Some(previous) = previous {
                                connect(&mut scene, index, previous, at);
                            } else if let Some(base) = revolute_outer() {
                                // U02:4254..4674 updates this scratch endpoint
                                // only inside the R branch, even when R is
                                // the first outer pair in the member order.
                                connect(&mut scene, index, at, base);
                            }
                            previous = Some(at);
                        } else {
                            finish(&mut scene, index, at, "");
                        }
                    }
                }
                continue;
            }
            match (group.kind, group.input_type) {
                (GroupKind::Input, b'0' | b'2') => {
                    if let Some(start) = base(0) {
                        let end = component(&mut scene, 0, start, 4, "an")?;
                        finish(&mut scene, 0, end, "aτ");
                    }
                }
                (GroupKind::Dyad, b'1') => {
                    for (index, slot) in [(0, 4), (1, 8)] {
                        if let Some(start) = base(index) {
                            let end = component(&mut scene, index, start, slot, "an")?;
                            finish(&mut scene, index, end, "aτ");
                        }
                    }
                }
                (GroupKind::Dyad, b'2') => {
                    let p_first = group
                        .pair_x
                        .and_then(|id| model.pair(id))
                        .is_some_and(|p| p.kind == PairKind::Prismatic);
                    let (pi, ri, ps, rs, ks) = if p_first {
                        (0, 1, 4, 8, 6)
                    } else {
                        (1, 0, 8, 4, 10)
                    };
                    let start = intermediate(&mut scene, pi, ps)?;
                    let end = component(&mut scene, pi, start, ks, "ak")?;
                    finish(&mut scene, pi, end, "aτ");
                    if let Some(start) = base(ri) {
                        let end = component(&mut scene, ri, start, rs, "an")?;
                        finish(&mut scene, ri, end, "aτ");
                    }
                }
                (GroupKind::Dyad, b'4') => {
                    for (index, slot, ks) in [(0, 4, 6), (1, 8, 10)] {
                        let start = intermediate(&mut scene, index, slot)?;
                        let end = component(&mut scene, index, start, ks, "ak")?;
                        finish(&mut scene, index, end, "aτ");
                    }
                }
                (GroupKind::Dyad, b'3' | b'5') => {
                    let at = intermediate(&mut scene, 0, 4)?;
                    if group.input_type == b'3' {
                        // U02:4132: each outer pair connects to the shared
                        // intermediate and to its own translated correction.
                        for (index, slot, label) in [(0, 6, "ak"), (1, 10, "an")] {
                            if let Some(start) = base(index) {
                                let end = component(&mut scene, index, start, slot, label)?;
                                connect(&mut scene, index, at, start);
                                connect(&mut scene, index, at, end);
                            }
                        }
                    } else {
                        // U02:4728/4925: the first correction is translated
                        // from the R outer pair; the second from a(4)′.
                        if let Some(start) = revolute_outer() {
                            let end = component(&mut scene, 0, start, 6, "ak")?;
                            connect(&mut scene, 0, at, start);
                            connect(&mut scene, 0, at, end);
                        }
                        let other = intermediate(&mut scene, 1, 8)?;
                        let end = component(&mut scene, 1, other, 10, "ak")?;
                        connect(&mut scene, 1, at, other);
                        connect(&mut scene, 1, at, end);
                    }
                }
                _ => {}
            }
        }
        Ok(scene)
    }

    pub fn bounds(&self) -> Rect {
        let mut bounds = Rect::NOTHING;
        for point in self
            .nodes
            .iter()
            .map(|n| n.position)
            .chain(self.paths.iter().flat_map(|p| p.points.iter().copied()))
            .chain(self.vectors.iter().flat_map(|v| [v.start, v.end]))
            .chain(self.trajectories.iter().flat_map(|p| p.iter().copied()))
            .chain(self.trace_dots.iter().copied())
            .chain(self.dimensions.iter().flat_map(|d| match d {
                Dimension::Length { start, end, .. } => vec![*start, *end],
                Dimension::Angle { center, .. } => vec![*center],
            }))
            .chain(self.annotations.iter().map(|a| a.position))
        {
            if finite(point) {
                bounds.extend_with(point);
            }
        }
        if !bounds.is_finite() {
            return Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0));
        }
        Rect::from_center_size(bounds.center(), bounds.size().max(Vec2::splat(0.05)))
    }
    fn screen_guides(&self, transform: &impl Fn(Pos2) -> Pos2) -> Vec<GuideGeometry> {
        self.nodes
            .iter()
            .filter_map(|node| {
                let NodeKind::Slider {
                    angle,
                    grounded: true,
                } = node.kind
                else {
                    return None;
                };
                if !finite(node.position) || !angle.is_finite() {
                    return None;
                }
                Some(GuideGeometry::local(transform(node.position), angle))
            })
            .collect()
    }
    /// Register the complete drawing before laying out any text. In particular,
    /// early point labels must also avoid arrows painted later in the frame.
    fn label_obstacles(
        &self,
        transform: &impl Fn(Pos2) -> Pos2,
        rect: Rect,
        guides: &[GuideGeometry],
    ) -> Vec<label_layout::Obstacle> {
        use label_layout::Obstacle;
        let mut obstacles = Vec::new();
        for trail in &self.trajectories {
            for edge in trail.windows(2) {
                obstacle_line(&mut obstacles, transform(edge[0]), transform(edge[1]), 2.5);
            }
        }
        for point in &self.trace_dots {
            if finite(*point) {
                obstacles.push(Obstacle::Box(Rect::from_center_size(
                    transform(*point),
                    Vec2::splat(4.0),
                )));
            }
        }
        if self.paths.is_empty() {
            for member in &self.members {
                for edge in member.nodes.windows(2) {
                    if let (Some(a), Some(b)) = (self.nodes.get(edge[0]), self.nodes.get(edge[1])) {
                        obstacle_line(
                            &mut obstacles,
                            transform(a.position),
                            transform(b.position),
                            3.5,
                        );
                    }
                }
            }
        } else {
            for path in &self.paths {
                for edge in path.points.windows(2) {
                    obstacle_line(&mut obstacles, transform(edge[0]), transform(edge[1]), 3.5);
                }
            }
        }
        for guide in guides {
            obstacle_line(&mut obstacles, guide.line[0], guide.line[1], 3.0);
            for [a, b] in &guide.hatches {
                obstacle_line(&mut obstacles, *a, *b, 2.5);
            }
        }
        for node in &self.nodes {
            if !finite(node.position) {
                continue;
            }
            let at = transform(node.position);
            match node.kind {
                NodeKind::Hinge { grounded } => {
                    obstacles.push(Obstacle::Box(Rect::from_center_size(at, Vec2::splat(12.0))));
                    if grounded {
                        obstacles.push(Obstacle::Box(Rect::from_min_max(
                            at + Vec2::new(-11.0, 3.0),
                            at + Vec2::new(11.0, 17.0),
                        )));
                    }
                }
                NodeKind::Point { .. } => {
                    obstacles.push(Obstacle::Box(Rect::from_center_size(at, Vec2::splat(8.0))))
                }
                NodeKind::Slider { angle, .. } => {
                    if !angle.is_finite() {
                        continue;
                    }
                    let vertices = slider_corners(at, angle, 1.5);
                    obstacles.push(Obstacle::Box(points_bounds(&vertices).expand(2.0)));
                }
            }
        }
        for angle in &self.rigid_angles {
            if !finite(angle.position)
                || !angle.from.is_finite()
                || !angle.sweep.is_finite()
                || !self
                    .nodes
                    .iter()
                    .any(|node| node.key == angle.key && finite(node.position))
            {
                continue;
            }
            let sweep = angle.sweep.rem_euclid(std::f32::consts::TAU);
            if !(0.001..=std::f32::consts::TAU - 0.001).contains(&sweep) {
                continue;
            }
            let at = transform(angle.position);
            let points = (0..=24)
                .map(|i| {
                    let a = angle.from + sweep * i as f32 / 24.0;
                    at + Vec2::new(a.cos(), -a.sin()) * 9.0
                })
                .collect::<Vec<_>>();
            obstacle_line(&mut obstacles, at, points[0], 2.5);
            obstacle_line(&mut obstacles, at, points[24], 2.5);
            for edge in points.windows(2) {
                obstacle_line(&mut obstacles, edge[0], edge[1], 2.5);
            }
        }
        for vector in &self.vectors {
            obstacle_arrow(
                &mut obstacles,
                transform(vector.start),
                transform(vector.end),
            );
        }
        for load in &self.loads {
            if !finite(load.position)
                || !self
                    .nodes
                    .iter()
                    .any(|node| node.key == load.key && finite(node.position))
            {
                continue;
            }
            let (direction, length) = match load.kind {
                LoadKind::Weight => (Vec2::new(0.0, 1.0), 25.0),
                LoadKind::Force { angle } if angle.is_finite() => {
                    (Vec2::new(angle.cos(), -angle.sin()), 20.0)
                }
                _ => continue,
            };
            let at = transform(load.position);
            obstacle_line(&mut obstacles, at, at + direction * length, 4.0);
        }
        for moment in &self.moments {
            if !finite(moment.position)
                || self.paths.is_empty()
                || !self.members.iter().any(|member| {
                    member.key == moment.key
                        && member.nodes.iter().all(|index| {
                            self.nodes
                                .get(*index)
                                .is_some_and(|node| finite(node.position))
                        })
                })
            {
                continue;
            }
            let glyph = moment_geometry(
                transform(moment.position) + Vec2::new(0.0, -20.0),
                moment.negative,
            );
            for edge in glyph.arc.windows(2) {
                obstacle_line(&mut obstacles, edge[0], edge[1], 3.5);
            }
            obstacles.push(Obstacle::Box(points_bounds(&glyph.head).expand(2.0)));
        }
        for dimension in &self.dimensions {
            match dimension {
                Dimension::Length { start, end, .. }
                    if finite(*start) && finite(*end) && start != end =>
                {
                    let a = transform(*start);
                    let b = transform(*end);
                    let normal = Vec2::new(-(b - a).y, (b - a).x).normalized() * 26.0;
                    obstacle_line(&mut obstacles, a, a + normal * 1.2, 3.0);
                    obstacle_line(&mut obstacles, b, b + normal * 1.2, 3.0);
                    obstacle_arrow(&mut obstacles, a + normal, b + normal);
                    obstacle_arrow(&mut obstacles, b + normal, a + normal);
                }
                Dimension::Angle {
                    center,
                    from,
                    sweep,
                    ..
                } if finite(*center) && from.is_finite() && sweep.is_finite() => {
                    let at = transform(*center);
                    let points = (0..=48)
                        .map(|i| {
                            let a = from + sweep * i as f32 / 48.0;
                            at + Vec2::new(a.cos(), -a.sin()) * 42.0
                        })
                        .collect::<Vec<_>>();
                    for edge in points.windows(2) {
                        obstacle_line(&mut obstacles, edge[0], edge[1], 3.0);
                    }
                }
                _ => {}
            }
        }
        if let Some(units) = self.units {
            let scale = transform(Pos2::new(1.0, 0.0)).distance(transform(Pos2::ZERO));
            if rect.width() >= 125.0
                && rect.height() >= 80.0
                && let Some((pixels, text)) = scale_bar_value(scale, units)
            {
                obstacles.push(Obstacle::Box(Rect::from_min_size(
                    rect.left_bottom() + Vec2::new(11.0, -38.0),
                    Vec2::new(pixels.max(text.chars().count() as f32 * 7.4) + 6.0, 27.0),
                )));
            }
        }
        obstacles
    }
    pub fn paint(&self, ui: &mut egui::Ui, view: &mut View) -> SceneInteraction {
        let (response, painter) = ui.allocate_painter(
            ui.available_size().max(Vec2::splat(1.0)),
            egui::Sense::click_and_drag(),
        );
        let rect = response.rect;
        let background = ui.visuals().extreme_bg_color;
        let foreground = ui.visuals().text_color();
        let accent = Color32::from_rgb(0, 125, 130);
        painter.rect_filled(rect, 4.0, background);
        if response.dragged() {
            view.pan += response.drag_delta();
        }
        if response.hovered() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll != 0.0 {
                view.zoom = (view.zoom * (scroll * 0.003).exp()).clamp(0.05, 40.0);
            }
        }
        let bounds = view.fixed_bounds.unwrap_or_else(|| self.bounds());
        let room = (rect.size() - Vec2::splat(90.0)).max(Vec2::splat(1.0));
        let scale = (room.x / bounds.width()).min(room.y / bounds.height()) * view.zoom;
        let origin = rect.center() + view.pan;
        let transform = |p: Pos2| {
            origin
                + Vec2::new(
                    (p.x - bounds.center().x) * scale,
                    -(p.y - bounds.center().y) * scale,
                )
        };
        let clip = rect.shrink(5.0_f32.min(rect.width().min(rect.height()) * 0.2));
        let guides = self.screen_guides(&transform);
        let mut labels = LabelPlacement::new(clip, self.label_obstacles(&transform, rect, &guides));
        let stroke = Stroke::new(1.6, foreground);
        let selected = |key| view.selected == Some(key) || view.highlighted.contains(&key);
        if view.grid {
            let faint = Stroke::new(0.5, ui.visuals().widgets.noninteractive.bg_stroke.color);
            let anchor = self
                .nodes
                .iter()
                .find(|node| {
                    finite(node.position)
                        && matches!(node.kind, NodeKind::Hinge { .. } | NodeKind::Slider { .. })
                })
                .map(|node| node.position)
                .unwrap_or(Pos2::ZERO);
            let anchor = transform(anchor);
            let mut x = rect.left() + (anchor.x - rect.left()).rem_euclid(40.0);
            let mut y = rect.top() + (anchor.y - rect.top()).rem_euclid(40.0);
            while x < rect.right() {
                painter.line_segment(
                    [Pos2::new(x, rect.top()), Pos2::new(x, rect.bottom())],
                    faint,
                );
                x += 40.0;
            }
            while y < rect.bottom() {
                painter.line_segment(
                    [Pos2::new(rect.left(), y), Pos2::new(rect.right(), y)],
                    faint,
                );
                y += 40.0;
            }
        }
        for trail in &self.trajectories {
            for edge in trail.windows(2) {
                if finite(edge[0]) && finite(edge[1]) {
                    painter.line_segment(
                        [transform(edge[0]), transform(edge[1])],
                        Stroke::new(1.0, Color32::from_rgb(174, 145, 52)),
                    );
                }
            }
        }
        for guide in &guides {
            painter.line_segment(guide.line, stroke);
            for hatch in &guide.hatches {
                painter.line_segment(*hatch, stroke);
            }
        }
        for path in &self.paths {
            let width = if selected(path.key) { 2.8 } else { 1.6 };
            for edge in path.points.windows(2) {
                if !finite(edge[0]) || !finite(edge[1]) {
                    continue;
                }
                let a = transform(edge[0]);
                let b = transform(edge[1]);
                let stroke = Stroke::new(
                    width,
                    if selected(path.key) {
                        accent
                    } else {
                        foreground
                    },
                );
                if path.dashed {
                    dashed(&painter, a, b, stroke);
                } else {
                    painter.line_segment([a, b], stroke);
                }
            }
        }
        for angle in &self.rigid_angles {
            if !finite(angle.position)
                || !angle.from.is_finite()
                || !angle.sweep.is_finite()
                || !self
                    .nodes
                    .iter()
                    .any(|node| node.key == angle.key && finite(node.position))
            {
                continue;
            }
            let at = transform(angle.position);
            let sweep = angle.sweep.rem_euclid(std::f32::consts::TAU);
            if !(0.001..=std::f32::consts::TAU - 0.001).contains(&sweep) {
                continue;
            }
            let st = Stroke::new(
                1.0,
                if selected(angle.key) {
                    accent
                } else {
                    foreground
                },
            );
            let points: Vec<_> = (0..=24)
                .map(|i| {
                    let a = angle.from + sweep * i as f32 / 24.0;
                    at + Vec2::new(a.cos(), -a.sin()) * 9.0
                })
                .collect();
            painter.line_segment([at, points[0]], st);
            painter.line_segment([at, points[24]], st);
            painter.add(Shape::line(points, st));
        }
        if self.paths.is_empty() {
            // Coincident schematic members still have distinct selection
            // keys. Put selected strokes on top while keeping label and hit
            // traversal in their original order.
            for emphasized in [false, true] {
                for member in self
                    .members
                    .iter()
                    .filter(|member| selected(member.key) == emphasized)
                {
                    let st = if emphasized {
                        Stroke::new(2.8, accent)
                    } else {
                        stroke
                    };
                    for edge in member.nodes.windows(2) {
                        if let (Some(a), Some(b)) =
                            (self.nodes.get(edge[0]), self.nodes.get(edge[1]))
                            && finite(a.position)
                            && finite(b.position)
                        {
                            painter
                                .line_segment([transform(a.position), transform(b.position)], st);
                        }
                    }
                }
            }
        }
        let mut clicked = None;
        for member in &self.members {
            let edges: Vec<_> = if self.paths.is_empty() {
                member
                    .nodes
                    .windows(2)
                    .filter_map(|edge| {
                        Some([
                            self.nodes.get(edge[0])?.position,
                            self.nodes.get(edge[1])?.position,
                        ])
                    })
                    .collect()
            } else {
                self.paths
                    .iter()
                    .filter(|p| p.key == member.key)
                    .flat_map(|p| p.points.windows(2).map(|p| [p[0], p[1]]))
                    .collect()
            };
            let mut label = false;
            for edge in edges {
                if !finite(edge[0]) || !finite(edge[1]) {
                    continue;
                }
                let a = transform(edge[0]);
                let b = transform(edge[1]);
                if !label && a.distance(b) > 5.0 {
                    labels.paint(
                        &painter,
                        &member.name,
                        a.lerp(b, 0.5) + Vec2::new(3.0, -12.0),
                        egui::Align2::CENTER_CENTER,
                        FontId::monospace(13.0),
                        if selected(member.key) {
                            accent
                        } else {
                            foreground
                        },
                        background,
                        Some(a.lerp(b, 0.5)),
                        if selected(member.key) { 250 } else { 60 },
                    );
                    label = true;
                }
                if response.clicked()
                    && response
                        .interact_pointer_pos()
                        .is_some_and(|p| segment_distance(p, a, b) < 6.0)
                {
                    clicked = Some(member.key);
                }
            }
            // A structure row already identifies both members before either
            // member has a second joint. Show their selected names without
            // fabricating an edge in the schematic preview.
            if !label
                && self.paths.is_empty()
                && selected(member.key)
                && let Some(node) = member
                    .nodes
                    .iter()
                    .filter_map(|index| self.nodes.get(*index))
                    .find(|node| finite(node.position))
            {
                let at = transform(node.position);
                labels.paint(
                    &painter,
                    &member.name,
                    at + Vec2::new(3.0, -12.0),
                    egui::Align2::CENTER_CENTER,
                    FontId::monospace(13.0),
                    accent,
                    background,
                    Some(at),
                    250,
                );
            }
        }
        let mut physical = Vec::<(String, Pos2)>::new();
        for node in &self.nodes {
            if !finite(node.position)
                || physical
                    .iter()
                    .any(|(name, at)| name == &node.name && at.distance(node.position) < 0.000001)
            {
                continue;
            }
            physical.push((node.name.clone(), node.position));
            let at = transform(node.position);
            let emphasized = selected(node.key)
                || self.nodes.iter().any(|other| {
                    other.name == node.name
                        && finite(other.position)
                        && other.position.distance(node.position) < 0.000001
                        && selected(other.key)
                });
            let stroke = if emphasized {
                Stroke::new(2.3, accent)
            } else {
                stroke
            };
            match node.kind {
                NodeKind::Hinge { grounded } => {
                    if grounded {
                        ground(&painter, at, 1.5, stroke);
                    }
                    painter.circle(at, 4.5, background, stroke);
                }
                NodeKind::Slider { angle, .. } => {
                    let vertices = slider_corners(at, angle, 1.5);
                    if self.mask_sliders && !self.outlined_sliders.contains(&node.key) {
                        painter.add(Shape::convex_polygon(vertices.to_vec(), background, stroke));
                    } else {
                        painter.add(Shape::closed_line(vertices.to_vec(), stroke));
                    }
                }
                NodeKind::Point { mass, force } => {
                    let color = if mass || force {
                        Color32::from_rgb(56, 150, 69)
                    } else {
                        Color32::from_rgb(190, 151, 22)
                    };
                    painter.rect_filled(Rect::from_center_size(at, Vec2::splat(4.5)), 0.0, color);
                }
            }
            labels.paint(
                &painter,
                &node.name,
                at + match node.kind {
                    NodeKind::Hinge { .. } => Vec2::new(12.0, 5.0),
                    NodeKind::Slider { grounded: true, .. } => Vec2::new(15.0, -14.0),
                    NodeKind::Slider { .. } => Vec2::new(-30.0, -14.0),
                    NodeKind::Point { .. } => Vec2::new(-10.0, 5.0),
                },
                egui::Align2::LEFT_TOP,
                FontId::monospace(14.0),
                if emphasized { accent } else { foreground },
                background,
                Some(at),
                if emphasized { 250 } else { 80 },
            );
            if response.clicked()
                && response
                    .interact_pointer_pos()
                    .is_some_and(|p| at.distance(p) < 13.0)
            {
                clicked = Some(node.key);
            }
        }
        for vector in &self.vectors {
            if !finite(vector.start) || !finite(vector.end) {
                continue;
            }
            let a = transform(vector.start);
            let b = transform(vector.end);
            arrow(
                &painter,
                a,
                b,
                Stroke::new(if selected(vector.key) { 2.8 } else { 1.6 }, vector.color),
            );
            labels.vector(
                &painter,
                vector,
                a,
                b,
                background,
                if selected(vector.key) { 250 } else { 55 },
            );
        }
        // U02 ShowPlans: symbol lengths are pixels and independent of force
        // magnitude and mechanism scale. A thick terminal shaft is the legacy
        // arrowhead convention (the original does not draw a filled triangle).
        for load in &self.loads {
            if !finite(load.position)
                || !self
                    .nodes
                    .iter()
                    .any(|node| node.key == load.key && finite(node.position))
            {
                continue;
            }
            let at = transform(load.position);
            let color = Color32::from_rgb(56, 150, 69);
            let (direction, length, head) = match load.kind {
                LoadKind::Weight => (Vec2::new(0.0, 1.0), 25.0, 20.0),
                LoadKind::Force { angle } => {
                    if !angle.is_finite() {
                        continue;
                    }
                    (Vec2::new(angle.cos(), -angle.sin()), 20.0, 15.0)
                }
            };
            let emphasized = selected(load.key);
            let width = if emphasized { 2.5 } else { 1.6 };
            painter.line_segment([at, at + direction * length], Stroke::new(width, color));
            painter.line_segment(
                [at + direction * head, at + direction * (head + 3.0)],
                Stroke::new(3.0, color),
            );
        }
        for moment in &self.moments {
            if !finite(moment.position)
                || self.paths.is_empty()
                || !self.members.iter().any(|member| {
                    member.key == moment.key
                        && member.nodes.iter().all(|index| {
                            self.nodes
                                .get(*index)
                                .is_some_and(|node| finite(node.position))
                        })
                })
            {
                continue;
            }
            let at = transform(moment.position) + Vec2::new(0.0, -20.0);
            let color = Color32::from_rgb(56, 150, 69);
            // U01 RunMotion has a fixed ellipse 12×9, arc10°..210°.
            // Its positive terminal is near180°..200°, negative near20°..40°.
            // The approved modern overlay adds a clear arrowhead and a value.
            let glyph = moment_geometry(at, moment.negative);
            let st = Stroke::new(if selected(moment.key) { 2.8 } else { 1.6 }, color);
            painter.add(Shape::line(glyph.arc, st));
            // Only the head touches the existing curve endpoint. A separate
            // tangent shaft would extend outside the ellipse and make a kink.
            painter.add(Shape::convex_polygon(glyph.head.to_vec(), color, st));
            labels.paint(
                &painter,
                &moment.label,
                at + Vec2::new(20.0, -2.0),
                egui::Align2::LEFT_CENTER,
                FontId::monospace(12.0),
                color,
                background,
                Some(at),
                if selected(moment.key) { 250 } else { 90 },
            );
        }
        for dimension in &self.dimensions {
            match dimension {
                Dimension::Length { start, end, label } => {
                    if !finite(*start) || !finite(*end) {
                        continue;
                    }
                    let a = transform(*start);
                    let b = transform(*end);
                    if a == b {
                        continue;
                    }
                    let normal = Vec2::new(-(b - a).y, (b - a).x).normalized() * 26.0;
                    let st = Stroke::new(1.2, accent);
                    painter.line_segment([a, a + normal * 1.2], st);
                    painter.line_segment([b, b + normal * 1.2], st);
                    arrow(&painter, a + normal, b + normal, st);
                    arrow(&painter, b + normal, a + normal, st);
                    labels.paint(
                        &painter,
                        label,
                        a.lerp(b, 0.5) + normal * 1.35,
                        egui::Align2::CENTER_CENTER,
                        FontId::proportional(15.0),
                        accent,
                        background,
                        Some(a.lerp(b, 0.5) + normal),
                        95,
                    );
                }
                Dimension::Angle {
                    center,
                    from,
                    sweep,
                    label,
                } => {
                    if !finite(*center) || !from.is_finite() || !sweep.is_finite() {
                        continue;
                    }
                    let at = transform(*center);
                    let points = (0..=48)
                        .map(|i| {
                            let a = from + sweep * i as f32 / 48.0;
                            at + Vec2::new(a.cos(), -a.sin()) * 42.0
                        })
                        .collect();
                    painter.add(Shape::line(points, Stroke::new(1.5, accent)));
                    let a = from + sweep * 0.5;
                    labels.paint(
                        &painter,
                        label,
                        at + Vec2::new(a.cos(), -a.sin()) * 67.0,
                        egui::Align2::CENTER_CENTER,
                        FontId::proportional(15.0),
                        accent,
                        background,
                        Some(at + Vec2::new(a.cos(), -a.sin()) * 42.0),
                        95,
                    );
                }
            }
        }
        for annotation in &self.annotations {
            if !finite(annotation.position) || annotation.text.is_empty() {
                continue;
            }
            let at = transform(annotation.position);
            labels.annotation(&painter, &annotation.text, at, accent);
        }
        if let Some(units) = self.units {
            scale_bar(&painter, rect, scale, units, foreground);
        }
        // MotPutPixel is drawn after the mechanism. In particular, its first
        // sample must remain visible at the centre of an auxiliary point.
        for point in &self.trace_dots {
            if finite(*point) {
                painter.circle_filled(transform(*point), 1.0, Color32::from_rgb(174, 145, 52));
            }
        }
        labels.render(&painter, background);
        if response.clicked() {
            ui.ctx().request_repaint();
        }
        SceneInteraction {
            clicked: response.clicked(),
            object: clicked,
        }
    }
}

struct MomentGeometry {
    arc: Vec<Pos2>,
    head: [Pos2; 3],
}
fn moment_geometry(at: Pos2, negative: bool) -> MomentGeometry {
    let (from, sweep) = if negative {
        (210.0_f32, -200.0_f32)
    } else {
        (10.0_f32, 200.0_f32)
    };
    let arc = (0..=40)
        .map(|i| {
            let a = (from + sweep * i as f32 / 40.0).to_radians();
            at + Vec2::new(12.0 * a.cos(), -9.0 * a.sin())
        })
        .collect::<Vec<_>>();
    let angle = (from + sweep).to_radians();
    let tangent = Vec2::new(-12.0 * angle.sin(), -9.0 * angle.cos()).normalized() * sweep.signum();
    let normal = Vec2::new(-tangent.y, tangent.x);
    let tip = arc[40];
    let base = tip - tangent * 6.0;
    MomentGeometry {
        arc,
        head: [tip, base + normal * 2.6, base - normal * 2.6],
    }
}
fn points_bounds(points: &[Pos2]) -> Rect {
    let mut bounds = Rect::NOTHING;
    for point in points {
        if finite(*point) {
            bounds.extend_with(*point);
        }
    }
    bounds
}
fn obstacle_line(obstacles: &mut Vec<label_layout::Obstacle>, a: Pos2, b: Pos2, clearance: f32) {
    if finite(a) && finite(b) {
        obstacles.push(label_layout::Obstacle::Segment { a, b, clearance });
    }
}
fn obstacle_arrow(obstacles: &mut Vec<label_layout::Obstacle>, a: Pos2, b: Pos2) {
    if !finite(a) || !finite(b) {
        return;
    }
    let delta = b - a;
    let length = delta.length();
    if length < 0.05 {
        return;
    }
    let direction = delta / length;
    let normal = Vec2::new(-direction.y, direction.x);
    let head = (length * 0.28).min(7.0);
    obstacle_line(obstacles, a, b, 3.5);
    obstacle_line(
        obstacles,
        b,
        b - direction * head + normal * head * 0.45,
        3.5,
    );
    obstacle_line(
        obstacles,
        b,
        b - direction * head - normal * head * 0.45,
        3.5,
    );
}
fn display(value: Real48) -> f32 {
    value.load_extended().to_display_f64() as f32
}
fn attachment_slot(model: &Mechanism, owner: LinkId, typ: u8) -> usize {
    if matches!(typ, b'2' | b'5') {
        return 1;
    }
    usize::from(
        matches!(typ, b'1' | b'4')
            && model
                .link(owner)
                .and_then(|link| link.pairs[0])
                .and_then(|id| model.pair(id))
                .is_some_and(|pair| pair.kind == PairKind::Prismatic),
    )
}
/// These are points carried by another body, rather than the speed/acceleration
/// of the member whose construction happens to contain the vector.
fn intermediate_name(
    model: &Mechanism,
    group: &diada_core::Group,
    index: usize,
    acceleration: bool,
) -> String {
    let links = [Some(group.link_a), group.link_b];
    let own = links[index].unwrap_or(group.link_a);
    let other_owner = |pair: PairId, own: LinkId| {
        model
            .pair(pair)
            .and_then(|pair| pair.links.iter().copied().find(|link| *link != own))
    };
    let (point, carrier) = match group.input_type {
        b'3' => (group.pair_x, group.link_b),
        b'5' => {
            let outer = [group.pair_x, group.pair_y];
            let r = outer.into_iter().flatten().find(|id| {
                model
                    .pair(*id)
                    .is_some_and(|p| p.kind == PairKind::Revolute)
            });
            let p = outer.into_iter().flatten().find(|id| {
                model
                    .pair(*id)
                    .is_some_and(|p| p.kind == PairKind::Prismatic)
            });
            let p_owner = links.into_iter().flatten().find(|link| {
                model
                    .link(*link)
                    .and_then(|l| l.pairs[0])
                    .is_some_and(|id| Some(id) == p)
            });
            (
                r,
                if index == 0 {
                    p_owner
                } else {
                    p.zip(p_owner).and_then(|(p, owner)| other_owner(p, owner))
                },
            )
        }
        _ => {
            let outer = model.link(own).and_then(|link| link.pairs[0]);
            (group.more, outer.and_then(|p| other_owner(p, own)))
        }
    };
    let name = point
        .and_then(|id| model.pair(id))
        .map(|p| p.name.display().to_lowercase())
        .unwrap_or_default();
    let carrier = carrier
        .and_then(|id| model.link(id))
        .map(|l| l.name.display())
        .unwrap_or_default();
    if name.is_empty() {
        return String::new();
    }
    format!("{name}({carrier}){}", if acceleration { "′" } else { "" })
}
fn arrow(painter: &egui::Painter, a: Pos2, b: Pos2, stroke: Stroke) {
    let delta = b - a;
    let length = delta.length();
    if length < 0.05 {
        return;
    }
    let direction = delta / length;
    let normal = Vec2::new(-direction.y, direction.x);
    let head = (length * 0.28).min(7.0);
    painter.line_segment([a, b], stroke);
    painter.line_segment([b, b - direction * head + normal * head * 0.45], stroke);
    painter.line_segment([b, b - direction * head - normal * head * 0.45], stroke);
}
struct LabelPlacement {
    clip: Rect,
    obstacles: Vec<label_layout::Obstacle>,
    requests: Vec<label_layout::Request>,
    labels: Vec<QueuedLabel>,
}
struct QueuedLabel {
    galley: std::sync::Arc<egui::Galley>,
    color: Color32,
    padding: Vec2,
    framed: bool,
}
impl LabelPlacement {
    fn new(clip: Rect, obstacles: Vec<label_layout::Obstacle>) -> Self {
        Self {
            clip,
            obstacles,
            requests: Vec::new(),
            labels: Vec::new(),
        }
    }
    #[allow(clippy::too_many_arguments)]
    fn paint(
        &mut self,
        painter: &egui::Painter,
        text: &str,
        at: Pos2,
        align: egui::Align2,
        font: FontId,
        color: Color32,
        _background: Color32,
        object: Option<Pos2>,
        priority: u16,
    ) {
        if text.is_empty() {
            return;
        }
        let galley = painter.layout(text.into(), font, color, self.clip.width().max(1.0));
        let size = galley.size();
        let desired = align.anchor_size(at, size);
        self.requests.push(label_layout::Request {
            anchor: object.unwrap_or(desired.center()),
            alternate_anchors: Vec::new(),
            size,
            preferred: desired,
            priority,
            force_leader: false,
            leader_gap: 7.0,
        });
        self.labels.push(QueuedLabel {
            galley,
            color,
            padding: Vec2::ZERO,
            framed: false,
        });
    }
    fn vector(
        &mut self,
        painter: &egui::Painter,
        vector: &Vector,
        a: Pos2,
        b: Pos2,
        background: Color32,
        priority: u16,
    ) {
        if vector.label_kind == VectorLabelKind::Endpoint {
            self.paint(
                painter,
                &vector.label,
                b + Vec2::new(7.0, -5.0),
                egui::Align2::LEFT_BOTTOM,
                FontId::monospace(13.0),
                vector.color,
                background,
                Some(b),
                priority,
            );
            return;
        }
        if vector.label.is_empty() {
            return;
        }
        let galley = painter.layout(
            vector.label.clone(),
            FontId::monospace(13.0),
            vector.color,
            self.clip.width().max(1.0),
        );
        let size = galley.size();
        let anchor = a + (b - a) * 0.5;
        let delta = b - a;
        let normal = if delta.length() >= 0.5 {
            let direction = delta.normalized();
            let mut normal = Vec2::new(-direction.y, direction.x);
            // Keep horizontal text on the upper (or right) side of its shaft.
            if normal.y > 0.0 || normal.y == 0.0 && normal.x < 0.0 {
                normal = -normal;
            }
            normal
        } else {
            // Zero/tiny components still have a finite, local label anchor.
            Vec2::new(0.0, -1.0)
        };
        let clearance = (size.x * normal.x.abs() + size.y * normal.y.abs()) * 0.5 + 6.0;
        self.requests.push(label_layout::Request {
            anchor,
            alternate_anchors: Vec::new(),
            size,
            preferred: Rect::from_center_size(anchor + normal * clearance, size),
            priority,
            force_leader: false,
            leader_gap: 0.0,
        });
        self.labels.push(QueuedLabel {
            galley,
            color: vector.color,
            padding: Vec2::ZERO,
            framed: false,
        });
    }
    fn annotation(&mut self, painter: &egui::Painter, text: &str, at: Pos2, color: Color32) {
        let padding = Vec2::new(6.0, 4.0);
        let galley = painter.layout(
            text.into(),
            FontId::proportional(13.0),
            color,
            (self.clip.width() - padding.x * 2.0).clamp(1.0, 280.0),
        );
        let size = galley.size() + padding * 2.0;
        self.requests.push(label_layout::Request {
            anchor: at,
            alternate_anchors: Vec::new(),
            size,
            preferred: Rect::from_min_size(at + Vec2::new(17.0, -size.y - 17.0), size),
            priority: 180,
            force_leader: false,
            leader_gap: 7.0,
        });
        self.labels.push(QueuedLabel {
            galley,
            color,
            padding,
            framed: true,
        });
    }
    fn render(&self, painter: &egui::Painter, background: Color32) {
        let key = label_layout::CacheKey::new(
            self.clip,
            &self.obstacles,
            &self.requests,
            self.labels
                .iter()
                .map(|label| label.galley.text().to_owned())
                .collect(),
        );
        let id = egui::Id::new("diada-screen-label-layout");
        let cached = painter.ctx().data_mut(|data| {
            data.get_temp_mut_or_default::<label_layout::Cache>(id)
                .get(&key)
        });
        let placements = if let Some(placements) = cached {
            placements
        } else {
            let placements = label_layout::layout(self.clip, &self.obstacles, &self.requests);
            painter.ctx().data_mut(|data| {
                data.get_temp_mut_or_default::<label_layout::Cache>(id)
                    .insert(key, placements.clone())
            });
            placements
        };
        for (label, placement) in self.labels.iter().zip(&placements) {
            if let Some(line) = placement.leader {
                painter.add(Shape::line(
                    line.to_vec(),
                    Stroke::new(0.8, label.color.gamma_multiply(0.65)),
                ));
            }
        }
        for (label, placement) in self.labels.iter().zip(placements) {
            if label.framed {
                painter.rect_filled(placement.rect, 3.0, background);
                painter.rect_stroke(
                    placement.rect,
                    3.0,
                    Stroke::new(1.0, label.color.gamma_multiply(0.5)),
                    egui::StrokeKind::Inside,
                );
            }
            painter.galley(
                placement.rect.min + label.padding,
                label.galley.clone(),
                label.color,
            );
        }
    }
}
fn scale_bar(painter: &egui::Painter, rect: Rect, scale: f32, units: &str, color: Color32) {
    if !scale.is_finite() || scale <= 0.0 || rect.width() < 125.0 || rect.height() < 80.0 {
        return;
    }
    let Some((pixels, text)) = scale_bar_value(scale, units) else {
        return;
    };
    let a = rect.left_bottom() + Vec2::new(14.0, -16.0);
    let b = a + Vec2::new(pixels, 0.0);
    let st = Stroke::new(1.0, color.gamma_multiply(0.7));
    painter.line_segment([a, b], st);
    for at in [a, b] {
        painter.line_segment([at + Vec2::new(0.0, -3.0), at + Vec2::new(0.0, 3.0)], st);
    }
    painter.text(
        a + Vec2::new(0.0, -6.0),
        egui::Align2::LEFT_BOTTOM,
        text,
        FontId::proportional(12.0),
        color.gamma_multiply(0.75),
    );
}
fn scale_bar_value(scale: f32, units: &str) -> Option<(f32, String)> {
    if !scale.is_finite() || scale <= 0.0 {
        return None;
    }
    let desired = 75.0 / scale;
    if !desired.is_finite() || desired <= 0.0 {
        return None;
    }
    let exponent = desired.log10().floor();
    let base = 10.0_f32.powf(exponent);
    let leading = desired / base;
    let value = base
        * if leading >= 5.0 {
            5.0
        } else if leading >= 2.0 {
            2.0
        } else {
            1.0
        };
    let pixels = value * scale;
    let number = if (0.001..10000.0).contains(&value) {
        let decimals = (-exponent).max(0.0) as usize;
        format!("{value:.decimals$}")
    } else {
        format!("{value:.0e}")
    };
    Some((pixels, format!("{number} {units}")))
}
fn position(xy: [Real48; 2]) -> Pos2 {
    Pos2::new(display(xy[0]), display(xy[1]))
}
fn finite(p: Pos2) -> bool {
    p.x.is_finite() && p.y.is_finite()
}
fn polar([magnitude, angle]: [Real48; 2]) -> Result<[Real48; 2], CoreError> {
    let m = magnitude.load_extended();
    let a = angle.load_extended();
    Ok([
        m.checked_mul(diada_compat::cos(a)?)?.store_real()?,
        m.checked_mul(diada_compat::sin(a)?)?.store_real()?,
    ])
}
fn add(a: [Real48; 2], b: [Real48; 2]) -> Result<[Real48; 2], CoreError> {
    Ok([
        a[0].load_extended()
            .checked_add(b[0].load_extended())?
            .store_real()?,
        a[1].load_extended()
            .checked_add(b[1].load_extended())?
            .store_real()?,
    ])
}
fn slider_corners(at: Pos2, angle: f32, scale: f32) -> [Pos2; 4] {
    let t = Vec2::new(angle.cos(), -angle.sin()) * 11.6 * scale;
    let n = Vec2::new(angle.sin(), angle.cos()) * 6.6 * scale;
    [at - t - n, at + t - n, at + t + n, at - t + n]
}
fn ground(painter: &egui::Painter, at: Pos2, scale: f32, stroke: Stroke) {
    let p = |x, y| at + Vec2::new(x, y) * scale;
    painter.add(Shape::line(
        vec![p(1.0, 2.0), p(4.0, 5.0), p(-4.0, 5.0), p(-1.0, 2.0)],
        stroke,
    ));
    painter.line_segment([p(-6.0, 6.0), p(6.0, 6.0)], stroke);
    for i in 0..4 {
        let x = -6.0 + i as f32 * 3.0;
        painter.line_segment([p(x, 10.0), p(x + 3.0, 7.0)], stroke);
    }
}
#[derive(Clone, Debug, PartialEq)]
struct GuideGeometry {
    line: [Pos2; 2],
    hatches: Vec<[Pos2; 2]>,
}
impl GuideGeometry {
    fn local(at: Pos2, angle: f32) -> Self {
        let t = Vec2::new(angle.cos(), -angle.sin());
        let n = Vec2::new(angle.sin(), angle.cos());
        let c = at + n * 12.0;
        Self {
            line: [c - t * 48.0, c + t * 48.0],
            hatches: (0..16)
                .map(|i| {
                    let start = c + t * (-48.0 + i as f32 * 6.0);
                    [start, start + n * 4.5 - t * 4.5]
                })
                .collect(),
        }
    }
}
fn segment_distance(p: Pos2, a: Pos2, b: Pos2) -> f32 {
    let d = b - a;
    let n = d.length_sq();
    if n <= f32::EPSILON {
        return p.distance(a);
    }
    p.distance(a + d * ((p - a).dot(d) / n).clamp(0.0, 1.0))
}
fn dashed(painter: &egui::Painter, a: Pos2, b: Pos2, stroke: Stroke) {
    let delta = b - a;
    let length = delta.length();
    if length <= f32::EPSILON {
        return;
    }
    let direction = delta / length;
    let mut start = 0.0;
    while start < length {
        painter.line_segment(
            [
                a + direction * start,
                a + direction * (start + 5.0).min(length),
            ],
            stroke,
        );
        start += 9.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn painted_grid(
        scene: &Scene,
        mut view: View,
        size: Vec2,
    ) -> (Rect, Vec<[Pos2; 2]>, egui::FullOutput) {
        let context = egui::Context::default();
        let mut rect = Rect::NOTHING;
        let mut output = context.run_ui(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
                ..Default::default()
            },
            |ui| {
                rect = ui.available_rect_before_wrap();
                scene.paint(ui, &mut view);
            },
        );
        output.textures_delta.clear();
        let lines = output
            .shapes
            .iter()
            .filter_map(|shape| match shape.shape {
                Shape::LineSegment { points, stroke } if stroke.width == 0.5 => Some(points),
                _ => None,
            })
            .collect();
        (rect, lines, output)
    }

    fn assert_grid_crosses(lines: &[[Pos2; 2]], anchor: Pos2) {
        assert!(
            lines
                .iter()
                .any(|[a, b]| a.x == b.x && (a.x - anchor.x).abs() < 0.001)
        );
        assert!(
            lines
                .iter()
                .any(|[a, b]| a.y == b.y && (a.y - anchor.y).abs() < 0.001)
        );
        for [a, b] in lines {
            let distance = if a.x == b.x {
                a.x - anchor.x
            } else {
                a.y - anchor.y
            };
            assert!((distance - (distance / 40.0).round() * 40.0).abs() < 0.001);
        }
    }

    #[test]
    fn grid_crosses_the_first_physical_joint_after_pan_zoom_and_resize() {
        let physical = Pos2::new(2.35, -1.15);
        for kind in [
            NodeKind::Hinge { grounded: true },
            NodeKind::Slider {
                angle: 0.45,
                grounded: true,
            },
        ] {
            let mut nodes: Vec<_> = [(false, false), (true, false), (false, true)]
                .into_iter()
                .enumerate()
                .map(|(index, (mass, force))| Node {
                    key: ObjectKey::Pair(index),
                    name: String::new(),
                    position: Pos2::new(0.31 + index as f32 * 0.1, 0.67),
                    kind: NodeKind::Point { mass, force },
                })
                .collect();
            nodes.extend([
                Node {
                    key: ObjectKey::Pair(3),
                    name: String::new(),
                    position: Pos2::new(f32::NAN, f32::NAN),
                    kind: NodeKind::Hinge { grounded: false },
                },
                Node {
                    key: ObjectKey::Pair(4),
                    name: String::new(),
                    position: physical,
                    kind,
                },
                Node {
                    key: ObjectKey::Pair(5),
                    name: String::new(),
                    position: Pos2::new(-0.5, 2.0),
                    kind: NodeKind::Hinge { grounded: false },
                },
            ]);
            let scene = Scene {
                nodes,
                ..Default::default()
            };
            let bounds = Rect::from_center_size(Pos2::ZERO, Vec2::splat(10.0));
            for (size, pan, zoom) in [
                (Vec2::new(620.0, 460.0), Vec2::ZERO, 1.0),
                (Vec2::new(620.0, 460.0), Vec2::new(17.0, -29.0), 1.65),
                (Vec2::new(1000.0, 680.0), Vec2::new(-23.0, 11.0), 0.75),
            ] {
                let (rect, lines, output) = painted_grid(
                    &scene,
                    View {
                        grid: true,
                        fixed_bounds: Some(bounds),
                        pan,
                        zoom,
                        ..Default::default()
                    },
                    size,
                );
                let room = rect.size() - Vec2::splat(90.0);
                let scale = (room.x / 10.0).min(room.y / 10.0) * zoom;
                let expected = rect.center() + pan + Vec2::new(physical.x, -physical.y) * scale;
                let drawn = output
                    .shapes
                    .iter()
                    .find_map(|shape| match (&shape.shape, kind) {
                        (Shape::Circle(circle), NodeKind::Hinge { .. }) if circle.radius == 4.5 => {
                            Some(circle.center)
                        }
                        (Shape::Path(path), NodeKind::Slider { .. })
                            if path.closed && path.points.len() == 4 =>
                        {
                            Some(points_bounds(&path.points).center())
                        }
                        _ => None,
                    })
                    .expect("the physical pair must be painted");
                assert!(drawn.distance(expected) < 0.001);
                assert_grid_crosses(&lines, drawn);
                assert_eq!(scene.nodes[4].position, physical);
            }
        }
    }

    #[test]
    fn grid_without_a_physical_joint_uses_world_zero_instead_of_an_auxiliary_point() {
        let scene = Scene {
            nodes: vec![Node {
                key: ObjectKey::Pair(0),
                name: String::new(),
                position: Pos2::new(1.37, 2.19),
                kind: NodeKind::Point {
                    mass: true,
                    force: false,
                },
            }],
            ..Default::default()
        };
        let bounds = Rect::from_center_size(Pos2::new(0.5, -0.25), Vec2::splat(10.0));
        let pan = Vec2::new(13.0, -7.0);
        let (rect, lines, _) = painted_grid(
            &scene,
            View {
                grid: true,
                fixed_bounds: Some(bounds),
                pan,
                ..Default::default()
            },
            Vec2::new(620.0, 460.0),
        );
        let room = rect.size() - Vec2::splat(90.0);
        let scale = (room.x / 10.0).min(room.y / 10.0);
        let zero = rect.center() + pan + Vec2::new(-bounds.center().x, bounds.center().y) * scale;
        assert_grid_crosses(&lines, zero);
        let (_, no_grid, _) = painted_grid(&scene, View::default(), Vec2::new(620.0, 460.0));
        assert!(no_grid.is_empty());
    }

    #[test]
    fn all_five_motion_series_keep_hinges_fixed_and_short_guides_follow_sliders() {
        for typ in 1..=5 {
            let model = original_pose(typ);
            let config = model
                .original_motion_configs(
                    diada_compat::parse_real(b"1").unwrap(),
                    None,
                    diada_core::AnalysisMode::Kinematics,
                )
                .unwrap()
                .remove(0);
            let scenes: Vec<_> = model
                .sample_frames(&config)
                .unwrap()
                .into_iter()
                .flatten()
                .map(|frame| Scene::motion(&frame.mechanism))
                .collect();
            let bounds = scenes
                .iter()
                .fold(Rect::NOTHING, |bounds, scene| bounds.union(scene.bounds()));
            let scale = (900.0 / bounds.width()).min(500.0 / bounds.height());
            let transform = |p: Pos2| {
                let d = p - bounds.center();
                Pos2::new(500.0 + d.x * scale, 300.0 - d.y * scale)
            };
            let first = &scenes[0];
            let first_guides = first.screen_guides(&transform);
            let mut slider_moved = false;
            for scene in &scenes {
                let guides = scene.screen_guides(&transform);
                assert_eq!(guides.len(), first_guides.len());
                let mut guide_index = 0;
                for (node, original) in scene.nodes.iter().zip(&first.nodes) {
                    match node.kind {
                        NodeKind::Hinge { grounded: true } => {
                            assert_eq!(node.position, original.position);
                            assert_eq!(transform(node.position), transform(original.position));
                        }
                        NodeKind::Slider {
                            angle,
                            grounded: true,
                        } => {
                            let guide = &guides[guide_index];
                            assert_eq!(
                                *guide,
                                GuideGeometry::local(transform(node.position), angle)
                            );
                            assert!((guide.line[0].distance(guide.line[1]) - 96.0).abs() < 0.0001);
                            assert_eq!(guide.hatches.len(), 16);
                            if node.position.distance(original.position) > 0.01 {
                                slider_moved = true;
                                assert_ne!(*guide, first_guides[guide_index]);
                            }
                            guide_index += 1;
                        }
                        _ => {}
                    }
                }
                assert_eq!(guide_index, guides.len());
            }
            if !first_guides.is_empty() {
                assert!(slider_moved, "ground slider should move in type {typ}");
            }
        }
    }

    #[test]
    fn local_guides_follow_an_oblique_slider_and_ignore_invalid_or_moving_carriers() {
        let angle = std::f32::consts::FRAC_PI_4;
        let node = |position, grounded| Node {
            key: ObjectKey::Pair(2),
            name: "C".into(),
            position,
            kind: NodeKind::Slider { angle, grounded },
        };
        let transform = |p: Pos2| Pos2::new(p.x * 10.0 + 150.0, 200.0 - p.y * 10.0);
        let t = Vec2::new(angle.cos(), -angle.sin());
        let n = Vec2::new(angle.sin(), angle.cos());
        let mut previous: Option<(Pos2, GuideGeometry)> = None;
        for at in [
            Pos2::new(1.0, 2.0),
            Pos2::new(-1.0, 0.0),
            Pos2::new(3.0, 4.0),
        ] {
            let scene = Scene {
                nodes: vec![
                    node(at, true),
                    node(Pos2::new(f32::NAN, 5.0), true),
                    node(Pos2::new(100.0, 200.0), false),
                    Node {
                        kind: NodeKind::Slider {
                            angle: f32::NAN,
                            grounded: true,
                        },
                        ..node(at, true)
                    },
                ],
                ..Default::default()
            };
            let guides = scene.screen_guides(&transform);
            assert_eq!(guides.len(), 1);
            let guide = &guides[0];
            let center = guide.line[0].lerp(guide.line[1], 0.5);
            assert!(center.distance(transform(at) + n * 12.0) < 0.0001);
            assert!((guide.line[1] - guide.line[0] - t * 96.0).length() < 0.0001);
            assert_eq!(guide.hatches.len(), 16);
            for hatch in &guide.hatches {
                assert!((hatch[1] - hatch[0] - n * 4.5 + t * 4.5).length() < 0.0001);
            }
            if let Some((before_at, before)) = &previous {
                let delta = transform(at) - transform(*before_at);
                for (now, old) in guide.line.iter().zip(&before.line) {
                    assert!((*now - *old - delta).length() < 0.0001);
                }
            }
            previous = Some((at, guide.clone()));
        }
    }

    #[test]
    fn static_schematics_keep_short_guides_and_hidden_mechanisms_have_no_rails() {
        let scene = Scene::topology(&original_pose(2));
        let local = scene.screen_guides(&|p| p);
        assert_eq!(local.len(), 1);
        let guide = &local[0];
        assert!((guide.line[0].distance(guide.line[1]) - 96.0).abs() < 0.0001);
        assert_eq!(guide.hatches.len(), 16);
        assert!(Scene::default().screen_guides(&|p| p).is_empty());
    }

    #[test]
    fn motion_labels_avoid_the_actual_local_support_geometry() {
        let model = original_pose(2);
        let config = model
            .original_motion_configs(
                diada_compat::parse_real(b"1").unwrap(),
                None,
                diada_core::AnalysisMode::Kinematics,
            )
            .unwrap()
            .remove(0);
        let scenes: Vec<_> = model
            .sample_frames(&config)
            .unwrap()
            .into_iter()
            .flatten()
            .map(|frame| Scene::motion(&frame.mechanism))
            .collect();
        let bounds = scenes
            .iter()
            .fold(Rect::NOTHING, |bounds, scene| bounds.union(scene.bounds()));
        let view = View {
            fixed_bounds: Some(bounds),
            ..Default::default()
        };
        for index in [0, 18, 36, 54] {
            for size in [Vec2::new(620.0, 460.0), Vec2::new(1000.0, 680.0)] {
                assert_scene_labels_clear_in_view(
                    &scenes[index],
                    &view,
                    size,
                    "local slider support",
                );
            }
        }
    }

    #[test]
    fn seven_connection_ground_extra_slider_has_guide_without_a_phantom_ground_rod() {
        let mut model = Mechanism::default();
        let links = (b'0'..=b'5')
            .map(|name| model.add_link(diada_core::Name([name, b' '])))
            .collect::<Vec<_>>();
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
                .add_pair(kind, diada_core::Name([name, b' ']), [links[a], links[b]])
                .unwrap();
        }
        model
            .set_ground_and_inputs(links[0], vec![links[1]])
            .unwrap();
        model.prepare_initial_groups().unwrap();
        model.append_dyad(links[2], links[3]).unwrap();
        model.append_dyad(links[4], links[5]).unwrap();
        let number = |value: &str| diada_compat::parse_real(value.as_bytes()).unwrap();
        for (pair, (x, y)) in model.pairs.iter_mut().zip([
            ("0", "0"),
            ("1", "1"),
            ("2", "1"),
            ("3", "0"),
            ("2", "2"),
            ("3", "3"),
            ("4", "3"),
        ]) {
            pair.state.x = number(x);
            pair.state.y = number(y);
        }
        model.links[3].geometry.origin_x = number("3");
        model.links[5].geometry.origin_x = number("4");
        model.links[5].geometry.origin_y = number("3");
        let f = PairId(6);
        assert!(model.points.iter().any(|point| point.owner == links[0]
            && point.pair == f
            && point.source == diada_core::AttachmentSource::ExtraPair));
        assert!(
            model
                .groups
                .iter()
                .any(|group| group.kind == GroupKind::Attached
                    && group.more == Some(f)
                    && group.input_type == b'4')
        );
        for point in &mut model.points {
            point.geometry.parameter = number("1");
        }
        for scene in [Scene::mechanism(&model), Scene::motion(&model)] {
            assert!(
                scene
                    .paths
                    .iter()
                    .all(|path| path.key != ObjectKey::Link(links[0].0)),
                "fixed O/C/F supports do not form moving rods"
            );
            assert!(
                scene
                    .rigid_angles
                    .iter()
                    .all(|angle| angle.key != ObjectKey::Pair(f.0))
            );
            for link in &links[1..] {
                assert!(
                    scene
                        .paths
                        .iter()
                        .any(|path| path.key == ObjectKey::Link(link.0)),
                    "moving member {} remains visible",
                    link.0
                );
            }
            let node = &scene.nodes[f.0];
            assert_eq!(node.name, "F");
            assert!(matches!(node.kind, NodeKind::Slider { grounded: true, .. }));
            let d = scene.nodes[4].position;
            assert_eq!(
                scene
                    .paths
                    .iter()
                    .filter(|path| path.key == ObjectKey::Link(links[2].0)
                        && path.points.last() == Some(&d))
                    .count(),
                2,
                "both moving-body rays to auxiliary D are retained"
            );
            assert_scene_labels_clear(
                &scene,
                Vec2::new(800.0, 600.0),
                "seven-connection mechanism",
            );
            let ctx = egui::Context::default();
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0))),
                    ..Default::default()
                },
                |ui| {
                    scene.paint(ui, &mut View::default());
                },
            );
            output.textures_delta.clear();
            let slider = output
                .shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    Shape::Path(path)
                        if path.closed
                            && path.points.len() == 4
                            && path.points[0].distance(path.points[1]) > 30.0
                            && points_bounds(&path.points).center().x > 500.0
                            && points_bounds(&path.points).center().y < 200.0 =>
                    {
                        Some(points_bounds(&path.points).center())
                    }
                    _ => None,
                })
                .unwrap();
            assert!(output.shapes.iter().any(|shape|matches!(&shape.shape,Shape::LineSegment {points,..} if (points[0].distance(points[1])-96.0).abs()<0.01 && (points[0].lerp(points[1],0.5).distance(slider)-12.0).abs()<0.01)),"F's fixed guide is still painted");
        }
    }

    #[test]
    fn original_trace_dot_overlays_current_point_and_keeps_earlier_samples_in_bounds() {
        let early = Pos2::new(-3.0, 2.0);
        let current = Pos2::new(1.0, -1.0);
        let scene = Scene {
            nodes: vec![Node {
                key: ObjectKey::Point(0),
                name: "T".into(),
                position: current,
                kind: NodeKind::Point {
                    mass: false,
                    force: false,
                },
            }],
            trace_dots: vec![early, current, Pos2::new(f32::NAN, 0.0)],
            ..Default::default()
        };
        assert!(scene.bounds().contains(early) && scene.bounds().contains(current));
        let only_trace = Scene {
            trace_dots: vec![early, current],
            ..Default::default()
        };
        assert_eq!(only_trace.bounds(), scene.bounds());
        let ctx = egui::Context::default();
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(480.0, 320.0))),
                ..Default::default()
            },
            |ui| {
                scene.paint(ui, &mut View::default());
            },
        );
        output.textures_delta.clear();
        let point_index=output.shapes.iter().position(|shape|matches!(&shape.shape,Shape::Rect(rect) if rect.rect.width()==4.5 && rect.rect.height()==4.5)).unwrap();
        let point_center = match &output.shapes[point_index].shape {
            Shape::Rect(rect) => rect.rect.center(),
            _ => unreachable!(),
        };
        let dots = output
            .shapes
            .iter()
            .enumerate()
            .filter_map(|(index, shape)| match &shape.shape {
                Shape::Circle(circle) if circle.fill == Color32::from_rgb(174, 145, 52) => {
                    Some((index, circle))
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            dots.len(),
            2,
            "invalid sample is skipped and no connecting segment is fabricated"
        );
        let (dot_index, dot) = dots
            .iter()
            .find(|(_, dot)| dot.center == point_center)
            .unwrap();
        assert!(
            *dot_index > point_index,
            "the current gold pixel must cover the point glyph"
        );
        assert_eq!(dot.radius, 1.0);
        assert!(
            !output
                .shapes
                .iter()
                .any(|shape| matches!(&shape.shape, Shape::LineSegment { .. })),
            "sampled dots are not converted into a polyline"
        );
        let obstacles = scene.label_obstacles(
            &|p| p,
            Rect::from_min_size(Pos2::ZERO, Vec2::new(480.0, 320.0)),
            &scene.screen_guides(&|p| p),
        );
        assert!(
            obstacles.iter().any(
                |obstacle| obstacle.intersects(Rect::from_center_size(early, Vec2::splat(0.5)))
            )
        );
    }

    #[test]
    fn selected_schematic_members_cover_overlapping_unselected_strokes() {
        let scene = Scene {
            nodes: vec![
                Node {
                    key: ObjectKey::Pair(0),
                    name: "O".into(),
                    position: Pos2::ZERO,
                    kind: NodeKind::Hinge { grounded: false },
                },
                Node {
                    key: ObjectKey::Pair(1),
                    name: "C".into(),
                    position: Pos2::new(1.0, 0.0),
                    kind: NodeKind::Hinge { grounded: false },
                },
                Node {
                    key: ObjectKey::Pair(2),
                    name: "B".into(),
                    position: Pos2::new(1.0, 1.0),
                    kind: NodeKind::Hinge { grounded: false },
                },
            ],
            members: vec![
                Member {
                    key: ObjectKey::Link(0),
                    name: "0".into(),
                    nodes: vec![0, 1],
                },
                Member {
                    key: ObjectKey::Link(1),
                    name: "1".into(),
                    nodes: vec![0, 1],
                },
                Member {
                    key: ObjectKey::Link(2),
                    name: "2".into(),
                    nodes: vec![0, 1],
                },
                Member {
                    key: ObjectKey::Link(3),
                    name: "3".into(),
                    nodes: vec![1, 2],
                },
            ],
            ..Default::default()
        };
        let ctx = egui::Context::default();
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(340.0, 280.0))),
                ..Default::default()
            },
            |ui| {
                scene.paint(
                    ui,
                    &mut View {
                        highlighted: vec![
                            ObjectKey::Pair(1),
                            ObjectKey::Link(0),
                            ObjectKey::Link(3),
                        ],
                        ..Default::default()
                    },
                );
            },
        );
        output.textures_delta.clear();
        let bodies = output
            .shapes
            .iter()
            .filter_map(|shape| match shape.shape {
                Shape::LineSegment { points, stroke } if points[0].distance(points[1]) > 80.0 => {
                    Some((points, stroke))
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        let midpoint = bodies
            .iter()
            .find(|(points, _)| points[0].y == points[1].y)
            .unwrap()
            .0[0]
            .lerp(
                bodies
                    .iter()
                    .find(|(points, _)| points[0].y == points[1].y)
                    .unwrap()
                    .0[1],
                0.5,
            );
        let overlap = bodies
            .iter()
            .filter(|(points, _)| segment_distance(midpoint, points[0], points[1]) < 1.0)
            .collect::<Vec<_>>();
        assert_eq!(overlap.len(), 3);
        let accent = Color32::from_rgb(0, 125, 130);
        assert!(
            overlap[..2]
                .iter()
                .all(|(_, stroke)| stroke.color != accent)
        );
        assert_eq!(
            overlap[2].1.color, accent,
            "the final stroke at the coincident edge must show the selected member"
        );
        assert_eq!(overlap[2].1.width, 2.8);
        assert!(
            bodies
                .iter()
                .any(|(points, stroke)| points[0].x == points[1].x && stroke.color == accent)
        );
        for name in ["C", "0", "3"] {
            assert!(output.shapes.iter().any(|shape|matches!(&shape.shape,Shape::Text(text) if text.galley.text()==name && text.fallback_color==accent)),"selected pair and both link names remain visible");
        }
    }

    #[test]
    fn selected_structure_pair_and_both_single_joint_member_names_are_visible() {
        let mut model = Mechanism::default();
        let a = model.add_link(diada_core::Name(*b"0 "));
        let b = model.add_link(diada_core::Name(*b"1 "));
        let pair = model
            .add_pair(PairKind::Revolute, diada_core::Name(*b"O "), [a, b])
            .unwrap();
        let scene = Scene::topology(&model);
        assert!(scene.paths.is_empty());
        let ctx = egui::Context::default();
        let raw = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(340.0, 280.0))),
            ..Default::default()
        };
        let mut output = ctx.run_ui(raw, |ui| {
            let mut view = View {
                highlighted: vec![
                    ObjectKey::Pair(pair.0),
                    ObjectKey::Link(a.0),
                    ObjectKey::Link(b.0),
                ],
                ..Default::default()
            };
            scene.paint(ui, &mut view);
        });
        output.textures_delta.clear();
        for name in ["O", "0", "1"] {
            let text = output
                .shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    Shape::Text(text) if text.galley.text() == name => Some(text),
                    _ => None,
                })
                .unwrap_or_else(|| panic!("missing selected name {name}"));
            assert_eq!(text.fallback_color, Color32::from_rgb(0, 125, 130));
        }
    }

    #[test]
    #[ignore = "presentation benchmark; prints genuine dense force frame timings"]
    fn label_layout_benchmark() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../tests/fixtures/diada-v320-derived/force-two-dyads-01/SCENE.DI");
        let model = diada_io::load_dialogue(
            &diada_io::Journal::from_bytes(&std::fs::read(path).unwrap()),
            "SCENE",
            &diada_io::TpNumbers,
        )
        .unwrap()
        .mechanism;
        let inputs = crate::analysis::InputFields::for_model(&model);
        let config = crate::analysis::config(&inputs, 12, false).unwrap();
        let mut sampler = diada_core::KinematicSampler::new(&model, &config).unwrap();
        sampler.next_frame().unwrap().unwrap();
        let frame = sampler.next_frame().unwrap().unwrap().mechanism;
        for (name, scene) in [
            ("M", Scene::mechanism(&frame)),
            ("V", Scene::plan(&frame, false).unwrap()),
            ("A", Scene::plan(&frame, true).unwrap()),
        ] {
            for size in [Vec2::new(161.0, 154.0), Vec2::new(315.0, 430.0)] {
                let ctx = egui::Context::default();
                let raw = egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
                    ..Default::default()
                };
                let mut times = Vec::new();
                for _ in 0..6 {
                    let timer = std::time::Instant::now();
                    let mut output = ctx.run_ui(raw.clone(), |ui| {
                        scene.paint(ui, &mut View::default());
                    });
                    output.textures_delta.clear();
                    times.push(timer.elapsed().as_secs_f64() * 1000.0);
                }
                eprintln!("dense force {name} {size:?} cold/warm ms: {times:?}");
            }
        }
        let mut sampler = diada_core::KinematicSampler::new(&model, &config).unwrap();
        let mut frames = Vec::new();
        while let Some(frame) = sampler.next_frame() {
            frames.push(frame.unwrap().mechanism);
        }
        for size in [Vec2::new(161.0, 154.0), Vec2::new(315.0, 430.0)] {
            let ctx = egui::Context::default();
            let raw = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
                ..Default::default()
            };
            let mut times = Vec::new();
            for frame in &frames {
                let scenes = [
                    Scene::motion(frame),
                    Scene::plan(frame, false).unwrap(),
                    Scene::plan(frame, true).unwrap(),
                ];
                let timer = std::time::Instant::now();
                for scene in scenes {
                    let mut output = ctx.run_ui(raw.clone(), |ui| {
                        scene.paint(ui, &mut View::default());
                    });
                    output.textures_delta.clear();
                }
                times.push(timer.elapsed().as_secs_f64() * 1000.0);
            }
            eprintln!(
                "dense changed cycle MVA {size:?}, {} frames, ms: {times:?}",
                frames.len()
            );
        }
    }

    #[test]
    fn moment_heads_attach_to_arc_and_follow_both_physical_directions() {
        // Independently specified ellipse endpoints and screen-y-down sense.
        for at in [Pos2::ZERO, Pos2::new(17.0, 19.0), Pos2::new(800.0, 400.0)] {
            for negative in [false, true] {
                let glyph = moment_geometry(at, negative);
                let tip = glyph.head[0];
                assert_eq!(Some(&tip), glyph.arc.last());
                let expected = if negative {
                    Vec2::new(11.81769, -1.56283)
                } else {
                    Vec2::new(-10.392_3, 4.5)
                };
                assert!((tip - at - expected).length() < 0.001);
                let direction = tip - glyph.head[1].lerp(glyph.head[2], 0.5);
                let radius = tip - at;
                let cross = radius.x * direction.y - radius.y * direction.x;
                assert!(if negative { cross > 0.0 } else { cross < 0.0 });
                assert!(direction.x > 0.0 && direction.y > 0.0);
                // The tangent of x²/144+y²/81=1 is perpendicular to its
                // gradient. This verifies geometry without repeating angles.
                let gradient = Vec2::new(radius.x / 144.0, radius.y / 81.0);
                assert!(gradient.dot(direction).abs() < 0.0001);
                assert!(glyph.head.iter().all(|p| finite(*p)));
                assert!((glyph.head[1] - glyph.head[2]).length() > 5.0);
                assert!((direction.length() - 6.0).abs() < 0.001);
                let incoming = tip - glyph.arc[glyph.arc.len() - 2];
                assert!(incoming.normalized().dot(direction.normalized()) > 0.99);
            }
        }
    }

    #[test]
    fn force_fixture_preserves_signed_values_and_omits_zero_moment() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../tests/fixtures/diada-v320-derived/force-fixed-01/SCENE.DI");
        let mut model = diada_io::load_dialogue(
            &diada_io::Journal::from_bytes(&std::fs::read(path).unwrap()),
            "SCENE",
            &diada_io::TpNumbers,
        )
        .unwrap()
        .mechanism;
        let scene = Scene::mechanism(&model);
        assert!(
            scene
                .moments
                .iter()
                .any(|m| !m.negative && m.label == "0.1 Н·м")
        );
        assert!(
            scene
                .moments
                .iter()
                .any(|m| m.negative && m.label == "-0.2 Н·м")
        );
        let before = model
            .links
            .iter()
            .map(|l| l.applied_moment)
            .collect::<Vec<_>>();
        for moment in &scene.moments {
            let _ = moment_geometry(moment.position, moment.negative);
        }
        assert_eq!(
            before,
            model
                .links
                .iter()
                .map(|l| l.applied_moment)
                .collect::<Vec<_>>()
        );
        let inputs = crate::analysis::InputFields::for_model(&model);
        let config = crate::analysis::config(&inputs, 12, false).unwrap();
        let mut sampler = diada_core::KinematicSampler::new(&model, &config).unwrap();
        sampler.next_frame().unwrap().unwrap();
        let frame = sampler.next_frame().unwrap().unwrap().mechanism;
        for (mode, scene) in [
            ("force M", Scene::mechanism(&frame)),
            ("force V", Scene::plan(&frame, false).unwrap()),
            ("force A", Scene::plan(&frame, true).unwrap()),
        ] {
            for size in [
                Vec2::new(161.0, 154.0),
                Vec2::new(315.0, 430.0),
                Vec2::new(1000.0, 680.0),
            ] {
                assert_scene_labels_clear(&scene, size, mode);
            }
        }
        for link in &mut model.links {
            link.applied_moment = Some(Real48::ZERO);
        }
        assert!(Scene::mechanism(&model).moments.is_empty());
    }

    #[test]
    fn actual_all_five_mechanism_and_plan_labels_avoid_drawn_geometry() {
        for typ in 1..=5 {
            let model = original_pose(typ);
            for (mode, scene) in [
                ("M", Scene::mechanism(&model)),
                ("V", Scene::plan(&model, false).unwrap()),
                ("A", Scene::plan(&model, true).unwrap()),
            ] {
                for size in [
                    Vec2::new(161.0, 154.0),
                    Vec2::new(315.0, 280.0),
                    Vec2::new(315.0, 430.0),
                    Vec2::new(1000.0, 680.0),
                ] {
                    assert_scene_labels_clear(&scene, size, &format!("{typ}{mode} {size:?}"));
                }
            }
        }
    }
    #[test]
    fn all_five_plan_components_attach_to_midshafts_and_point_letters_to_vertices() {
        let mut component_names = std::collections::BTreeSet::new();
        let mut carried_point_names = std::collections::BTreeSet::new();
        for typ in 1..=5 {
            let model = original_pose(typ);
            for acceleration in [false, true] {
                let scene = Scene::plan(&model, acceleration).unwrap();
                let ctx = egui::Context::default();
                let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
                    let mut labels = LabelPlacement::new(
                        Rect::from_min_size(Pos2::ZERO, Vec2::new(1000.0, 700.0)),
                        Vec::new(),
                    );
                    for vector in scene.vectors.iter().filter(|v| !v.label.is_empty()) {
                        let a = Pos2::new(300.0, 350.0)
                            + Vec2::new(vector.start.x, -vector.start.y) * 40.0;
                        let b =
                            Pos2::new(300.0, 350.0) + Vec2::new(vector.end.x, -vector.end.y) * 40.0;
                        labels.vector(ui.painter(), vector, a, b, Color32::WHITE, 55);
                        let request = labels.requests.last().unwrap();
                        if matches!(vector.label.as_str(), "an" | "aτ" | "ak") {
                            component_names.insert(vector.label.clone());
                            assert!(acceleration);
                            assert_eq!(vector.label_kind, VectorLabelKind::Component);
                            assert_eq!(request.anchor, a + (b - a) * 0.5);
                            assert_eq!(request.leader_gap, 0.0);
                        } else {
                            carried_point_names.insert((acceleration, vector.label.clone()));
                            assert_eq!(vector.label_kind, VectorLabelKind::Endpoint);
                            assert_eq!(request.anchor, b);
                            assert_eq!(request.leader_gap, 7.0);
                        }
                    }
                    labels.render(ui.painter(), Color32::WHITE);
                });
                output.textures_delta.clear();
                let painted = output.shapes.iter().filter_map(|s| match &s.shape {
                    Shape::Text(text) => Some(text),
                    _ => None,
                });
                for text in painted {
                    assert_eq!(text.angle, 0.0, "{typ} V/A text must stay horizontal");
                }
            }
        }
        assert_eq!(component_names, ["ak", "an", "aτ"].map(String::from).into());
        assert!(carried_point_names.contains(&(false, "a(3)".into())));
        assert!(carried_point_names.contains(&(true, "a(3)′".into())));
    }
    #[test]
    fn shared_endpoint_components_keep_distinct_midshaft_labels_and_exact_leaders() {
        let clip = Rect::from_min_size(Pos2::ZERO, Vec2::new(600.0, 420.0));
        let end = Pos2::new(440.0, 220.0);
        let vectors = [
            (Pos2::new(70.0, 90.0), "an"),
            (Pos2::new(70.0, 220.0), "aτ"),
            (Pos2::new(70.0, 350.0), "ak"),
        ]
        .map(|(start, label)| Vector {
            key: ObjectKey::Link(1),
            start,
            end,
            label: label.into(),
            label_kind: VectorLabelKind::Component,
            color: Color32::BLUE,
        });
        let mut obstacles = Vec::new();
        for vector in &vectors {
            let middle = vector.start + (vector.end - vector.start) * 0.5;
            obstacles.push(label_layout::Obstacle::Segment {
                a: vector.start,
                b: vector.end,
                clearance: 3.0,
            });
            // Force displacement at each shaft, rather than at their common tip.
            obstacles.push(label_layout::Obstacle::Box(Rect::from_center_size(
                middle,
                Vec2::new(72.0, 64.0),
            )));
        }
        let ctx = egui::Context::default();
        let mut placements = Vec::new();
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            let mut labels = LabelPlacement::new(clip, obstacles.clone());
            for vector in &vectors {
                labels.vector(
                    ui.painter(),
                    vector,
                    vector.start,
                    vector.end,
                    Color32::WHITE,
                    55,
                );
            }
            placements = label_layout::layout(clip, &obstacles, &labels.requests);
            labels.render(ui.painter(), Color32::WHITE);
        });
        output.textures_delta.clear();
        for (index, (vector, placement)) in vectors.iter().zip(&placements).enumerate() {
            assert_eq!(placement.collisions, 0);
            assert!(clip.contains_rect(placement.rect));
            assert!(obstacles.iter().all(|o| !o.intersects(placement.rect)));
            assert!(
                placements[..index]
                    .iter()
                    .all(|p| !p.rect.intersects(placement.rect))
            );
            let middle = vector.start + (vector.end - vector.start) * 0.5;
            let leader = placement
                .leader
                .expect("blocked midpoint needs a thin leader");
            assert_eq!(leader[0], middle);
            assert_ne!(leader[0], end);
            assert!(placement.rect.distance_to_pos(leader[2]) < 0.001);
            assert!(output.shapes.iter().any(|s| match &s.shape {
                Shape::Path(path) => path.points == leader && path.stroke.width == 0.8,
                _ => false,
            }));
        }
        let texts = output
            .shapes
            .iter()
            .filter_map(|s| match &s.shape {
                Shape::Text(text) => Some(text),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(texts.len(), 3);
        for text in texts {
            assert_eq!(text.angle, 0.0);
        }
    }
    #[test]
    fn tiny_and_zero_component_labels_remain_finite_and_near_their_segments() {
        let ctx = egui::Context::default();
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            let mut labels = LabelPlacement::new(
                Rect::from_min_size(Pos2::ZERO, Vec2::new(400.0, 300.0)),
                Vec::new(),
            );
            for delta in [Vec2::ZERO, Vec2::new(0.001, -0.001)] {
                let a = Pos2::new(200.0, 150.0);
                let b = a + delta;
                let vector = Vector {
                    key: ObjectKey::Link(1),
                    start: a,
                    end: b,
                    label: "aτ".into(),
                    label_kind: VectorLabelKind::Component,
                    color: Color32::BLUE,
                };
                labels.vector(ui.painter(), &vector, a, b, Color32::WHITE, 55);
                let request = labels.requests.last().unwrap();
                assert!(finite(request.anchor));
                assert!(request.preferred.is_finite());
                assert!(request.preferred.distance_to_pos(a) < 7.0);
                assert!(request.preferred.bottom() < a.y);
            }
            let placements = label_layout::layout(labels.clip, &[], &labels.requests);
            assert!(
                placements
                    .iter()
                    .all(|p| p.rect.is_finite() && p.leader.is_none())
            );
            labels.render(ui.painter(), Color32::WHITE);
        });
        output.textures_delta.clear();
    }
    #[test]
    fn dimensions_question_callouts_and_force_values_share_clear_layout() {
        let mut model = original_pose(1);
        model.links[1].applied_moment = Some(diada_compat::parse_real(b"0.1").unwrap());
        model.links[2].applied_moment = Some(diada_compat::parse_real(b"-0.2").unwrap());
        let mut scene = Scene::mechanism(&model);
        let a = scene.nodes.iter().find(|n| n.name == "A").unwrap().position;
        let o = scene.nodes.iter().find(|n| n.name == "O").unwrap().position;
        scene.dimensions.push(Dimension::Length {
            start: o,
            end: a,
            label: "OA = ? м".into(),
        });
        scene.dimensions.push(Dimension::Angle {
            center: o,
            from: 0.0,
            sweep: std::f32::consts::FRAC_PI_6,
            label: "φ = ?°".into(),
        });
        scene.annotations.push(Annotation {
            position: a,
            text: "Укажите длину звена OA".into(),
        });
        assert_scene_labels_clear(&scene, Vec2::new(620.0, 460.0), "dimensions and question");
    }
    fn assert_scene_labels_clear(scene: &Scene, size: Vec2, description: &str) {
        assert_scene_labels_clear_in_view(scene, &View::default(), size, description);
    }
    fn assert_scene_labels_clear_in_view(
        scene: &Scene,
        view: &View,
        size: Vec2,
        description: &str,
    ) {
        let ctx = egui::Context::default();
        let mut rect = Rect::NOTHING;
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
                ..Default::default()
            },
            |ui| {
                rect = ui.available_rect_before_wrap();
                scene.paint(ui, &mut view.clone());
            },
        );
        output.textures_delta.clear();
        let bounds = view.fixed_bounds.unwrap_or_else(|| scene.bounds());
        let room = (rect.size() - Vec2::splat(90.0)).max(Vec2::splat(1.0));
        let scale = (room.x / bounds.width()).min(room.y / bounds.height());
        let transform = |p: Pos2| {
            rect.center()
                + Vec2::new(
                    (p.x - bounds.center().x) * scale,
                    -(p.y - bounds.center().y) * scale,
                )
        };
        let guides = scene.screen_guides(&transform);
        let obstacles = scene.label_obstacles(&transform, rect, &guides);
        let texts = output
            .shapes
            .iter()
            .filter_map(|s| match &s.shape {
                Shape::Text(text) => Some((
                    text.galley.text(),
                    Rect::from_min_size(text.pos, text.galley.size()),
                )),
                _ => None,
            })
            // The fixed ruler is painted before all queued object labels;
            // its reserved box is itself one of the geometry obstacles.
            .skip(usize::from(
                scene.units.is_some() && rect.width() >= 125.0 && rect.height() >= 80.0,
            ))
            .collect::<Vec<_>>();
        for (index, (text, bounds)) in texts.iter().enumerate() {
            assert!(
                rect.contains_rect(*bounds),
                "{description}: {text} outside viewport {bounds:?}"
            );
            assert!(
                obstacles.iter().all(|o| !o.intersects(*bounds)),
                "{description}: {text} covers geometry {bounds:?}"
            );
            assert!(
                texts[..index]
                    .iter()
                    .all(|(_, other)| !other.intersects(*bounds)),
                "{description}: {text} overlaps another label {bounds:?}"
            );
        }
        assert!(!texts.is_empty());
        for vector in &scene.vectors {
            if !vector.label.is_empty() && finite(vector.start) && finite(vector.end) {
                assert!(
                    texts.iter().any(|(text, _)| *text == vector.label),
                    "{description}: missing vector label {}",
                    vector.label
                );
            }
        }
        for moment in &scene.moments {
            assert!(
                texts.iter().any(|(text, _)| *text == moment.label),
                "{description}: missing moment value"
            );
        }
        for annotation in &scene.annotations {
            assert!(
                texts.iter().any(|(text, _)| *text == annotation.text),
                "{description}: missing question annotation"
            );
        }
    }

    fn original_pose(typ: usize) -> Mechanism {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(
            "../../../tests/fixtures/diada-v320-derived/dyad-{typ:02}/SCENE.DI"
        ));
        let loaded = diada_io::load_dialogue(
            &diada_io::Journal::from_bytes(&std::fs::read(path).unwrap()),
            "SCENE",
            &diada_io::TpNumbers,
        )
        .unwrap();
        let inputs = crate::analysis::InputFields::for_model(&loaded.mechanism);
        let config = crate::analysis::config(&inputs, 12, false).unwrap();
        let mut sampler = diada_core::KinematicSampler::new(&loaded.mechanism, &config).unwrap();
        sampler.next_frame().unwrap().unwrap();
        sampler.next_frame().unwrap().unwrap().mechanism
    }

    // The Python checker fits only a uniform screen scale and translation to
    // original VGA pixels. It cannot change any member's geometry or angles.
    #[test]
    #[ignore = "write geometry for independent original raster QA"]
    fn export_whole_reference_geometry() {
        let path = std::env::var("DIADA_GRAPHICS_GEOMETRY").expect("output JSON path");
        let mut entries = Vec::new();
        for typ in 1..=5 {
            let model = original_pose(typ);
            for (mode, scene) in [
                ('M', Scene::mechanism(&model)),
                ('V', Scene::plan(&model, false).unwrap()),
                ('A', Scene::plan(&model, true).unwrap()),
            ] {
                let edges = scene
                    .paths
                    .iter()
                    .flat_map(|path| path.points.windows(2).map(|p| [p[0], p[1]]))
                    .chain(scene.vectors.iter().map(|v| [v.start, v.end]))
                    .filter(|p| finite(p[0]) && finite(p[1]) && p[0].distance_sq(p[1]) > 1e-12)
                    .map(|p| {
                        format!(
                            "[[{:.9},{:.9}],[{:.9},{:.9}]]",
                            p[0].x, p[0].y, p[1].x, p[1].y
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(",");
                let nodes = scene
                    .nodes
                    .iter()
                    .filter(|node| finite(node.position))
                    .map(|node| {
                        format!(
                            "{{\"xy\":[{:.9},{:.9}],\"kind\":\"{}\"}}",
                            node.position.x,
                            node.position.y,
                            match node.kind {
                                NodeKind::Slider { .. } => "P",
                                NodeKind::Hinge { .. } => "R",
                                _ => "T",
                            }
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(",");
                entries.push(format!(
                    "{{\"type\":{typ},\"mode\":\"{mode}\",\"edges\":[{edges}],\"nodes\":[{nodes}]}}"
                ));
            }
        }
        std::fs::write(path, format!("[{}]", entries.join(",\n"))).unwrap();
    }

    #[test]
    fn visible_slider_feet_and_type3_construction_follow_original_30degree_frame() {
        let model = original_pose(3);
        let group = model
            .groups
            .iter()
            .find(|g| g.kind == GroupKind::Dyad)
            .unwrap();
        let scene = Scene::mechanism(&model);
        let node = &scene.nodes[group.more.unwrap().0];
        // Independently measured centre of B's rectangle in type-3-M.png:
        // O=(75,225), C=(575,225), B=(140,233). These coordinates check the
        // visible foot rather than the other body's stored shared-P state.
        let pixel = Pos2::new(
            75.0 + node.position.x * (500.0 / 3.0),
            225.0 - node.position.y * (500.0 / 3.0),
        );
        assert!(pixel.distance(Pos2::new(140.0, 233.0)) < 1.0, "{pixel:?}");
        let stored = model.pair(group.more.unwrap()).unwrap().state;
        assert!(node.position.distance(position([stored.x, stored.y])) > 2.0);
        for acceleration in [false, true] {
            let scene = Scene::plan(&model, acceleration).unwrap();
            let intermediate = scene
                .vectors
                .iter()
                .find(|v| v.label == if acceleration { "a(3)′" } else { "a(3)" })
                .unwrap();
            let at = intermediate.end;
            assert!(
                scene
                    .vectors
                    .iter()
                    .filter(|v| v.end.distance(at) < 1e-6 || v.start.distance(at) < 1e-6)
                    .count()
                    >= 3
            );
        }
    }

    #[test]
    fn no_nan_bounds_or_zero_load_arrow_and_blank_mass_point_keeps_geometry() {
        let mut model = original_pose(1);
        let owner = model.inputs[0];
        let point = model
            .add_point(owner, PairKind::MassCenter, diada_core::Name::BLANK)
            .unwrap();
        model.links[owner.0].mass = Some(diada_core::MassProperties {
            mass: Real48::ZERO,
            inertia: Real48::ZERO,
            center: Some(point),
        });
        let scene = Scene::mechanism(&model);
        assert!(
            scene
                .nodes
                .iter()
                .any(|n| n.name.is_empty() && matches!(n.kind, NodeKind::Point { mass: true, .. }))
        );
        assert!(scene.loads.is_empty());
        let mut scene = Scene::default();
        scene.annotations.push(Annotation {
            position: Pos2::new(5.0, 6.0),
            text: "?".into(),
        });
        scene.dimensions.push(Dimension::Length {
            start: Pos2::new(-2.0, -3.0),
            end: Pos2::new(1.0, 1.0),
            label: "?".into(),
        });
        let bounds = scene.bounds();
        assert!(bounds.contains(Pos2::new(5.0, 6.0)) && bounds.contains(Pos2::new(-2.0, -3.0)));
    }

    #[test]
    fn original_type5_velocity_triangle_contains_its_long_slanted_closing_edge() {
        let model = original_pose(5);
        let scene = Scene::plan(&model, false).unwrap();
        let a = scene
            .nodes
            .iter()
            .find(|node| node.name == "a")
            .unwrap()
            .position;
        let carried = scene
            .vectors
            .iter()
            .find(|vector| vector.label == "a(3)")
            .unwrap()
            .end;
        let edge = scene
            .vectors
            .iter()
            .find(|vector| {
                vector.start.distance(carried) < 1e-6 && vector.end.distance(a) < 1e-6
                    || vector.end.distance(carried) < 1e-6 && vector.start.distance(a) < 1e-6
            })
            .expect("the a(3)→a diagonal is present in original type-5-V.png");
        let midpoint = edge.start.lerp(edge.end, 0.5);
        // Original raster: pole≈(575,345), carried a(3)≈(75,345),
        // a≈(450,128); middle of that visible diagonal≈(263,237).
        let pixel = Pos2::new(575.0 + midpoint.x * 500.0, 345.0 - midpoint.y * 500.0);
        assert!(pixel.distance(Pos2::new(263.0, 237.0)) < 1.0, "{pixel:?}");
    }
}
