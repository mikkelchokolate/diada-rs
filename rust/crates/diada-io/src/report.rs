use crate::{EncodingError, LegacyName, encode_cp866};
use diada_compat::{Extended80, Real48};
use std::fmt;

pub const LINK_HEADING: &str = " Угловое положение подвижных звеньев (град.,рад/сек,рад/сек/сек)";
pub const POINT_HEADING: &str =
    " Подвижные кинематические пары и точки (метры,м/сек,м/сек/сек; углы - в град)";
pub const REACTION_HEADING: &str = " Реакции в кинематических парах. ( Н, Нм)";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputSource {
    Keyboard,
    File,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReportClock {
    pub year: u16,
    pub month: u8,
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReportHeader {
    pub mechanism_name: String,
    pub source: InputSource,
    pub clock: ReportClock,
}

/// Application branding for newly generated reports. Names and versions are
/// single ASCII tokens, so they cannot inject lines into the CP866 report.
/// Historical oracle reports continue to use `write_dif` without this value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReportIdentity<'a> {
    name: &'a str,
    version: &'a str,
}
impl<'a> ReportIdentity<'a> {
    pub fn new(name: &'a str, version: &'a str) -> Result<Self, ReportError> {
        let valid_token = |text: &str| {
            text.as_bytes()
                .first()
                .is_some_and(u8::is_ascii_alphanumeric)
                && text.bytes().all(|byte| {
                    byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'+')
                })
        };
        if !valid_token(name) || !valid_token(version) {
            return Err(ReportError::InvalidIdentity);
        }
        Ok(Self { name, version })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReportNumber {
    Real(Real48),
    Extended(Extended80),
}
impl From<Real48> for ReportNumber {
    fn from(value: Real48) -> Self {
        Self::Real(value)
    }
}
impl From<Extended80> for ReportNumber {
    fn from(value: Extended80) -> Self {
        Self::Extended(value)
    }
}

/// Formatting is supplied by the measured Turbo Pascal numeric runtime.
/// Rust native/f64 formatting must not be substituted for this contract.
pub trait NumberFormatter {
    fn format(&self, value: ReportNumber, width: i16, decimals: i16)
    -> Result<String, ReportError>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LinkReport {
    pub name: LegacyName,
    pub angle_degrees: ReportNumber,
    pub omega: ReportNumber,
    pub eps: ReportNumber,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PointReport {
    pub name: LegacyName,
    pub values: [ReportNumber; 10],
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReactionApplication {
    Rotation,
    PerpendicularFoot {
        rotating_pair: LegacyName,
        guide: LegacyName,
    },
    GuideIntersection {
        first: LegacyName,
        second: LegacyName,
    },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReactionReport {
    pub name: LegacyName,
    pub on_link: LegacyName,
    pub from_link: LegacyName,
    pub on_ground: bool,
    pub values: [ReportNumber; 4],
    pub application: ReactionApplication,
    pub moment: Option<ReportNumber>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BalanceMoment {
    pub on_link: LegacyName,
    pub value: ReportNumber,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PositionReport {
    pub links: Vec<LinkReport>,
    pub points: Vec<PointReport>,
    pub balance_moment: Option<BalanceMoment>,
    pub reactions: Vec<ReactionReport>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReportError {
    Encoding(EncodingError),
    Numeric(String),
    InconsistentSnapshot(String),
    UndefinedLegacyPath(String),
    InvalidClock,
    InvalidIdentity,
}
impl fmt::Display for ReportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Encoding(error) => error.fmt(f),
            Self::Numeric(error)
            | Self::InconsistentSnapshot(error)
            | Self::UndefinedLegacyPath(error) => f.write_str(error),
            Self::InvalidClock => f.write_str("invalid report clock"),
            Self::InvalidIdentity => f.write_str("invalid report program identity"),
        }
    }
}
impl std::error::Error for ReportError {}
impl From<EncodingError> for ReportError {
    fn from(error: EncodingError) -> Self {
        Self::Encoding(error)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ReportSelection {
    pub links: Option<Vec<LegacyName>>,
    pub points: Option<Vec<LegacyName>>,
    pub reactions: Option<Vec<LegacyName>>,
}
impl ReportSelection {
    fn contains(selection: &Option<Vec<LegacyName>>, name: LegacyName) -> bool {
        selection.as_ref().is_none_or(|names| names.contains(&name))
    }
}

/// Write complete six-position U03 blocks from calculated snapshots.
/// The independent position data must already contain original degree
/// conversions and vector projections; the writer performs no calculation.
pub fn write_dif<F: NumberFormatter>(
    header: &ReportHeader,
    positions: &[PositionReport],
    selection: &ReportSelection,
    formatter: &F,
) -> Result<Vec<u8>, ReportError> {
    write_dif_as(header, positions, selection, formatter, None)
}

/// Write the same report bytes with an explicit application name and version
/// in each block banner. No original serial number is added to this identity.
pub fn write_dif_with_identity<F: NumberFormatter>(
    header: &ReportHeader,
    positions: &[PositionReport],
    selection: &ReportSelection,
    formatter: &F,
    identity: ReportIdentity<'_>,
) -> Result<Vec<u8>, ReportError> {
    write_dif_as(header, positions, selection, formatter, Some(identity))
}

fn write_dif_as<F: NumberFormatter>(
    header: &ReportHeader,
    positions: &[PositionReport],
    selection: &ReportSelection,
    formatter: &F,
    identity: Option<ReportIdentity<'_>>,
) -> Result<Vec<u8>, ReportError> {
    validate_clock(header.clock)?;
    let mut output = Vec::new();
    for (block_index, block) in positions.chunks(6).enumerate() {
        if block_index != 0 {
            line(&mut output, "#####")?;
        }
        write_header(&mut output, header, identity)?;
        let first = &block[0];
        validate_snapshot_order(block)?;
        line(&mut output, LINK_HEADING)?;
        column_header(&mut output, "Имя   ", block_index * 6 + 1, block.len())?;
        for (index, link) in first.links.iter().enumerate() {
            // U03 always prints the first leading link before its name-filter
            // loop for following links. The snapshot excludes the ground link.
            if index != 0 && !ReportSelection::contains(&selection.links, link.name) {
                continue;
            }
            for (field, decimals, label) in [(0, 2, "  Fi"), (1, 3, " Omega"), (2, 3, "   Eps")] {
                let prefix = if field == 0 {
                    format!("{}{label}", link.name.text())
                } else {
                    label.to_owned()
                };
                let values: Vec<_> = block
                    .iter()
                    .map(|position| {
                        let item = &position.links[index];
                        match field {
                            0 => item.angle_degrees,
                            1 => item.omega,
                            _ => item.eps,
                        }
                    })
                    .collect();
                value_row(&mut output, &prefix, &values, decimals, formatter)?;
            }
        }
        line(&mut output, "")?;
        if let Some(moment) = &first.balance_moment {
            line(
                &mut output,
                &format!("Уравновешивающий момент на звене {}", moment.on_link.text()),
            )?;
            let values: Vec<_> = block
                .iter()
                .map(|position| position.balance_moment.as_ref().unwrap().value)
                .collect();
            value_row(&mut output, "     M", &values, 4, formatter)?;
            line(&mut output, "")?;
        }
        line(&mut output, POINT_HEADING)?;
        column_header(&mut output, "Имя   ", block_index * 6 + 1, block.len())?;
        let point_labels = [
            "   X", "     Y", "   |V|", " уг. V", " пр.Vx", " пр.Vy", "   |a|", " уг. a", " пр.ax",
            " пр.ay",
        ];
        for (index, point) in first.points.iter().enumerate() {
            if !ReportSelection::contains(&selection.points, point.name) {
                continue;
            }
            for (field, label) in point_labels.iter().enumerate() {
                let prefix = if field == 0 {
                    format!("{}{label}", point.name.text())
                } else {
                    (*label).to_owned()
                };
                let values: Vec<_> = block
                    .iter()
                    .map(|position| position.points[index].values[field])
                    .collect();
                value_row(
                    &mut output,
                    &prefix,
                    &values,
                    if field == 3 || field == 7 { 2 } else { 4 },
                    formatter,
                )?;
            }
        }
        line(&mut output, "")?;
        if !first.reactions.is_empty() {
            line(&mut output, REACTION_HEADING)?;
            column_header(&mut output, "      ", block_index * 6 + 1, block.len())?;
            for (index, reaction) in first.reactions.iter().enumerate() {
                if !ReportSelection::contains(&selection.reactions, reaction.name) {
                    continue;
                }
                let ground = if reaction.on_ground {
                    " стойки"
                } else {
                    ""
                };
                line(
                    &mut output,
                    &format!(
                        "Пара {}{ground}. Реакция на звено {} со стороны звена {}",
                        reaction.name.text(),
                        reaction.on_link.text(),
                        reaction.from_link.text()
                    ),
                )?;
                for (field, label) in ["   |R|", " уг. R", " пр.Rx", " пр.Ry"].iter().enumerate()
                {
                    let values: Vec<_> = block
                        .iter()
                        .map(|position| position.reactions[index].values[field])
                        .collect();
                    value_row(
                        &mut output,
                        label,
                        &values,
                        if field == 1 { 2 } else { 4 },
                        formatter,
                    )?;
                }
                match &reaction.application {
                    ReactionApplication::Rotation => {}
                    ReactionApplication::PerpendicularFoot {
                        rotating_pair,
                        guide,
                    } => {
                        line(&mut output, "Реакция приложена в основании перпендикуляра,")?;
                        line(
                            &mut output,
                            &format!(
                                "опущенного из пары {} на направляющую пары {}",
                                rotating_pair.text(),
                                guide.text()
                            ),
                        )?;
                    }
                    ReactionApplication::GuideIntersection { first, second } => {
                        line(
                            &mut output,
                            &format!(
                                "Реакция приложена в точке пересечения направляющих поступательных пар {} и {}",
                                first.text(),
                                second.text()
                            ),
                        )?;
                    }
                }
                if reaction.moment.is_some() {
                    line(
                        &mut output,
                        &format!(
                            "  Момент сил реакции на звено {} со стороны звена {}",
                            reaction.on_link.text(),
                            reaction.from_link.text()
                        ),
                    )?;
                    let values: Vec<_> = block
                        .iter()
                        .map(|position| position.reactions[index].moment.unwrap())
                        .collect();
                    value_row(&mut output, "     M", &values, 4, formatter)?;
                }
            }
            line(&mut output, "")?;
        }
    }
    Ok(output)
}

fn validate_snapshot_order(block: &[PositionReport]) -> Result<(), ReportError> {
    let first = &block[0];
    for item in &block[1..] {
        if item
            .links
            .iter()
            .map(|value| value.name)
            .ne(first.links.iter().map(|value| value.name))
            || item
                .points
                .iter()
                .map(|value| value.name)
                .ne(first.points.iter().map(|value| value.name))
            || item
                .reactions
                .iter()
                .map(|value| (&value.name, &value.application, value.moment.is_some()))
                .ne(first
                    .reactions
                    .iter()
                    .map(|value| (&value.name, &value.application, value.moment.is_some())))
            || item.balance_moment.as_ref().map(|value| value.on_link)
                != first.balance_moment.as_ref().map(|value| value.on_link)
        {
            return Err(ReportError::InconsistentSnapshot(
                "report object order or reaction roles changed within a six-position block"
                    .to_owned(),
            ));
        }
    }
    Ok(())
}
fn validate_clock(clock: ReportClock) -> Result<(), ReportError> {
    if clock.year == 0
        || !(1..=12).contains(&clock.month)
        || !(1..=31).contains(&clock.day)
        || clock.hour > 23
        || clock.minute > 59
        || clock.second > 59
    {
        Err(ReportError::InvalidClock)
    } else {
        Ok(())
    }
}
fn write_header(
    output: &mut Vec<u8>,
    header: &ReportHeader,
    identity: Option<ReportIdentity<'_>>,
) -> Result<(), ReportError> {
    let c = header.clock;
    let program = identity.map_or_else(
        || "Program DIADA v3.20 Serial Number 32D400 ".to_owned(),
        |identity| format!("Program {} v{} ", identity.name, identity.version),
    );
    line(
        output,
        &format!(
            "{program}{} D:{}.{}.{} T:{}.{:02}.{} ",
            if header.source == InputSource::Keyboard {
                "K"
            } else {
                "F"
            },
            c.day,
            c.month,
            c.year,
            c.hour,
            c.minute,
            c.second
        ),
    )?;
    line(
        output,
        &format!("{}{}", header.mechanism_name, "-".repeat(60)),
    )
}
fn column_header(
    output: &mut Vec<u8>,
    prefix: &str,
    start: usize,
    count: usize,
) -> Result<(), ReportError> {
    let mut text = prefix.to_owned();
    for index in 0..count {
        if index > 0 {
            text.push_str("    ");
        }
        text.push_str(&format!("    {:2}", start + index));
    }
    line(output, &text)
}
fn value_row<F: NumberFormatter>(
    output: &mut Vec<u8>,
    prefix: &str,
    values: &[ReportNumber],
    decimals: i16,
    formatter: &F,
) -> Result<(), ReportError> {
    let mut text = prefix.to_owned();
    for (index, value) in values.iter().enumerate() {
        if index > 0 {
            text.push_str("  ");
        }
        text.push(' ');
        text.push_str(&formatter.format(*value, 7, decimals)?);
    }
    line(output, &text)
}
fn line(output: &mut Vec<u8>, text: &str) -> Result<(), ReportError> {
    output.extend(encode_cp866(text)?);
    output.extend_from_slice(b"\r\n");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_identity_replaces_only_each_block_banner_and_keeps_legacy_output_available() {
        let positions = vec![PositionReport::default(); 13];
        let identity = ReportIdentity::new("diada-rs", "0.1.0").unwrap();
        for source in [InputSource::Keyboard, InputSource::File] {
            let header = ReportHeader {
                mechanism_name: "Механизм Я".into(),
                source,
                clock: ReportClock {
                    year: 2026,
                    month: 10,
                    day: 15,
                    hour: 23,
                    minute: 7,
                    second: 9,
                },
            };
            let legacy = write_dif(
                &header,
                &positions,
                &ReportSelection::default(),
                &crate::TpNumbers,
            )
            .unwrap();
            let actual = write_dif_with_identity(
                &header,
                &positions,
                &ReportSelection::default(),
                &crate::TpNumbers,
                identity,
            )
            .unwrap();
            let mut expected = Vec::new();
            let mut banners = 0;
            for line in legacy.split_inclusive(|byte| *byte == b'\n') {
                if let Some(rest) = line.strip_prefix(b"Program DIADA v3.20 Serial Number 32D400 ")
                {
                    expected.extend_from_slice(b"Program diada-rs v0.1.0 ");
                    expected.extend_from_slice(rest);
                    banners += 1;
                } else {
                    expected.extend_from_slice(line);
                }
            }
            assert_eq!(banners, 3);
            assert_eq!(actual, expected);
            assert!(
                !actual
                    .windows(b"32D400".len())
                    .any(|part| part == b"32D400")
            );
            let mode = if source == InputSource::Keyboard {
                "K"
            } else {
                "F"
            };
            assert!(actual.starts_with(
                format!("Program diada-rs v0.1.0 {mode} D:15.10.2026 T:23.07.9 \r\n").as_bytes()
            ));
        }
    }

    #[test]
    fn report_identity_rejects_line_injection_whitespace_and_non_ascii_tokens() {
        for invalid in [
            "",
            "\r\n",
            "new\nline",
            "a b",
            "a\t",
            "ДиАДА",
            ".hidden",
            "+1",
        ] {
            assert_eq!(
                ReportIdentity::new(invalid, "0.1.0"),
                Err(ReportError::InvalidIdentity)
            );
            assert_eq!(
                ReportIdentity::new("diada-rs", invalid),
                Err(ReportError::InvalidIdentity)
            );
        }
        assert!(ReportIdentity::new("diada-rs", "0.1.0-rc.1+build_2").is_ok());
    }
}
