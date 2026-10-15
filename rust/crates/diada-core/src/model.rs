use diada_compat::{NumericError, Real48};
use std::fmt;

/// Exact two-byte name, including spaces and NUL bytes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Name(pub [u8; 2]);
impl Name {
    pub const BLANK: Self = Self([b' ', b' ']);
    pub const fn new(first: u8, second: u8) -> Self {
        Self([first, second])
    }
    pub fn display(self) -> String {
        self.0
            .into_iter()
            .filter(|b| *b != 0)
            .map(cp866_char)
            .collect::<String>()
            .trim_end()
            .to_owned()
    }
}
impl fmt::Display for Name {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.display())
    }
}
fn cp866_char(b: u8) -> char {
    match b {
        0..=127 => char::from(b),
        128..=159 => char::from_u32(0x410 + u32::from(b - 128)).unwrap(),
        160..=175 => char::from_u32(0x430 + u32::from(b - 160)).unwrap(),
        224..=239 => char::from_u32(0x440 + u32::from(b - 224)).unwrap(),
        176..=223 => "░▒▓│┤╡╢╖╕╣║╗╝╜╛┐└┴┬├─┼╞╟╚╔╩╦╠═╬╧╨╤╥╙╘╒╓╫╪┘┌█▄▌▐▀"
            .chars()
            .nth(usize::from(b - 176))
            .unwrap(),
        _ => "ЁёЄєЇїЎў°∙·√№¤■ "
            .chars()
            .nth(usize::from(b - 240))
            .unwrap(),
    }
}
macro_rules! id { ($($name:ident),+) => { $(#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)] pub struct $name(pub usize);)+ }; }
id!(LinkId, PairId, GroupId, PointId);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PairKind {
    Revolute,
    Prismatic,
    Point,
    MassCenter,
    ForcePoint,
}
impl PairKind {
    pub const fn code(self) -> u8 {
        match self {
            Self::Revolute => b'R',
            Self::Prismatic => b'P',
            Self::Point => b'T',
            Self::MassCenter => b'M',
            Self::ForcePoint => b'F',
        }
    }
    pub fn from_code(c: u8) -> Result<Self, CoreError> {
        match c {
            b'R' => Ok(Self::Revolute),
            b'P' => Ok(Self::Prismatic),
            b'T' => Ok(Self::Point),
            b'M' => Ok(Self::MassCenter),
            b'F' => Ok(Self::ForcePoint),
            _ => Err(CoreError::InvalidPairKind(c)),
        }
    }
}

/// Speed and acceleration use magnitude/direction, not Cartesian components.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PairState {
    pub x: Real48,
    pub y: Real48,
    pub speed: Real48,
    pub speed_angle: Real48,
    pub acceleration: Real48,
    pub acceleration_angle: Real48,
    pub fi: Real48,
    pub px: Real48,
    pub py: Real48,
    /// Reaction (or F/M load) magnitude, polar direction in radians, moment.
    pub rx: Real48,
    pub ry: Real48,
    pub rm: Real48,
}
impl Default for PairState {
    fn default() -> Self {
        Self {
            x: Real48::ZERO,
            y: Real48::ZERO,
            speed: Real48::ZERO,
            speed_angle: Real48::ZERO,
            acceleration: Real48::ZERO,
            acceleration_angle: Real48::ZERO,
            fi: Real48::ZERO,
            px: Real48::ZERO,
            py: Real48::ZERO,
            rx: Real48::ZERO,
            ry: Real48::ZERO,
            rm: Real48::ZERO,
        }
    }
}
impl PairState {
    pub fn display_xy(self) -> [f64; 2] {
        [
            self.x.load_extended().to_display_f64(),
            self.y.load_extended().to_display_f64(),
        ]
    }
    pub fn copy_kinematics_from(&mut self, other: Self) {
        let (rx, ry, rm) = (self.rx, self.ry, self.rm);
        *self = other;
        self.rx = rx;
        self.ry = ry;
        self.rm = rm;
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pair {
    pub kind: PairKind,
    pub name: Name,
    pub links: [LinkId; 2],
    pub state: PairState,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LinkGeometry {
    pub parameter: Real48,
    pub angle: Real48,
    pub rate: Real48,
    pub rate_derivative: Real48,
    pub origin_x: Real48,
    pub origin_y: Real48,
    pub last_value: Real48,
}
impl Default for LinkGeometry {
    fn default() -> Self {
        Self {
            parameter: Real48::ZERO,
            angle: Real48::ZERO,
            rate: Real48::ZERO,
            rate_derivative: Real48::ZERO,
            origin_x: Real48::ZERO,
            origin_y: Real48::ZERO,
            last_value: Real48::ZERO,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MassProperties {
    pub mass: Real48,
    pub inertia: Real48,
    pub center: Option<PointId>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Link {
    pub name: Name,
    pub pairs: [Option<PairId>; 2],
    pub extra_pairs: Vec<PairId>,
    pub geometry: LinkGeometry,
    pub mass: Option<MassProperties>,
    pub applied_moment: Option<Real48>,
}
impl Link {
    pub fn new(name: Name) -> Self {
        Self {
            name,
            pairs: [None, None],
            extra_pairs: Vec::new(),
            geometry: LinkGeometry::default(),
            mass: None,
            applied_moment: None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AttachmentSource {
    ExtraPair,
    AdditionalPoint,
}
/// Auxiliary 60-byte link geometry is separate from the actual link arena.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AttachedPoint {
    pub owner: LinkId,
    pub pair: PairId,
    pub source: AttachmentSource,
    pub geometry: LinkGeometry,
}
pub type AttachmentGeometry = LinkGeometry;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GroupKind {
    Ground,
    Input,
    Dyad,
    Attached,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Group {
    pub kind: GroupKind,
    pub assembly: u8,
    pub input_type: u8,
    pub link_a: LinkId,
    pub link_b: Option<LinkId>,
    pub pair_x: Option<PairId>,
    pub pair_y: Option<PairId>,
    pub more: Option<PairId>,
    /// For T groups, references auxiliary geometry rather than a real link.
    pub attached_point: Option<PointId>,
    /// Original offsets +23..+94: Other[4], radialA[2], otherDeriv[2],
    /// radialB[2], final[2]. Twelve six-byte values in the 103-byte record.
    pub values: [Real48; 12],
}
impl Group {
    pub fn new(kind: GroupKind, link_a: LinkId) -> Self {
        Self {
            kind,
            assembly: b'P',
            input_type: b' ',
            link_a,
            link_b: None,
            pair_x: None,
            pair_y: None,
            more: None,
            attached_point: None,
            values: [Real48::ZERO; 12],
        }
    }
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Mechanism {
    pub title: String,
    pub links: Vec<Link>,
    pub pairs: Vec<Pair>,
    pub groups: Vec<Group>,
    pub points: Vec<AttachedPoint>,
    pub ground: Option<LinkId>,
    pub inputs: Vec<LinkId>,
    pub force_analysis: bool,
}
impl Mechanism {
    pub fn link(&self, id: LinkId) -> Option<&Link> {
        self.links.get(id.0)
    }
    pub fn pair(&self, id: PairId) -> Option<&Pair> {
        self.pairs.get(id.0)
    }
    pub fn group(&self, id: GroupId) -> Option<&Group> {
        self.groups.get(id.0)
    }
    pub fn point(&self, id: PointId) -> Option<&AttachedPoint> {
        self.points.get(id.0)
    }
    pub fn link_by_name(&self, name: Name) -> Option<LinkId> {
        self.links.iter().position(|v| v.name == name).map(LinkId)
    }
    pub fn pair_by_name(&self, name: Name) -> Option<PairId> {
        self.pairs.iter().position(|v| v.name == name).map(PairId)
    }
    pub fn sync_named_pair(&mut self, id: PairId) -> Result<(), CoreError> {
        let p = self.pair(id).ok_or(CoreError::MissingPair(id))?.clone();
        for (i, other) in self.pairs.iter_mut().enumerate() {
            if i != id.0 && other.name == p.name {
                other.state.copy_kinematics_from(p.state);
            }
        }
        Ok(())
    }
    pub fn formal_mobility(&self) -> isize {
        let n = self.links.len() as isize - 1;
        let p = self
            .pairs
            .iter()
            .filter(|p| matches!(p.kind, PairKind::Revolute | PairKind::Prismatic))
            .count() as isize;
        3 * n - 2 * p
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnalysisMode {
    Kinematics,
    Kinetostatics,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InputSeries {
    pub link: LinkId,
    pub start: Real48,
    /// Preserved legacy increment field (+6). New configurations mirror step.
    pub other_start: Real48,
    pub rate: Real48,
    pub rate_derivative: Real48,
    pub step: Real48,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InputPosition {
    pub position: Real48,
    /// Historic TKinematicStep+6: stored series increment, not another
    /// independent position. The input calculation itself does not read it.
    pub other_position: Real48,
    pub rate: Real48,
    pub rate_derivative: Real48,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AnalysisConfig {
    pub positions: usize,
    pub inputs: Vec<InputSeries>,
    pub mode: AnalysisMode,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CalculationSnapshot {
    pub position_index: usize,
    pub inputs: Vec<InputPosition>,
    pub mechanism: Mechanism,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CoreError {
    Numeric(NumericError),
    MissingLink(LinkId),
    MissingPair(PairId),
    MissingPoint(PointId),
    UnknownLink(Name),
    InvalidPairKind(u8),
    InvalidStructure(&'static str),
    UndefinedPosition {
        group: Option<GroupId>,
        reason: &'static str,
    },
    Unassemblable {
        group: Option<GroupId>,
    },
    InvalidAnalysis(&'static str),
}
impl From<NumericError> for CoreError {
    fn from(e: NumericError) -> Self {
        Self::Numeric(e)
    }
}
impl fmt::Display for CoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for CoreError {}
