//! Input values and series belong to the calculation session, never the canvas.
use chrono::{Datelike, Timelike};
use diada_compat::Real48;
use diada_core::*;
use diada_io::{
    InputSource, ReportClock, ReportHeader, ReportIdentity, ReportSelection, TpNumbers,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InputFields {
    pub link: LinkId,
    pub name: String,
    pub rotational: bool,
    pub start: String,
    pub rate: String,
    pub acceleration: String,
    pub step: String,
}

impl InputFields {
    pub fn for_model(model: &Mechanism) -> Vec<Self> {
        model
            .inputs
            .iter()
            .map(|id| {
                let typ = model
                    .groups
                    .iter()
                    .find(|g| g.kind == GroupKind::Input && g.link_a == *id)
                    .map(|g| g.input_type)
                    .unwrap_or(b'0');
                Self {
                    link: *id,
                    name: model.links[id.0].name.display(),
                    rotational: matches!(typ, b'0' | b'2'),
                    start: "0".into(),
                    rate: "1".into(),
                    acceleration: "0".into(),
                    step: if matches!(typ, b'0' | b'2') {
                        "30"
                    } else {
                        "0"
                    }
                    .into(),
                }
            })
            .collect()
    }
    pub fn series(&self) -> Result<InputSeries, String> {
        let start = parse(&self.start)?;
        let step = parse(&self.step)?;
        // The historical input routine receives degrees and converts them
        // itself. Link/point geometry, in contrast, is already in radians.
        Ok(InputSeries {
            link: self.link,
            start,
            other_start: step,
            rate: parse(&self.rate)?,
            rate_derivative: parse(&self.acceleration)?,
            step,
        })
    }
}

pub fn parse(text: &str) -> Result<Real48, String> {
    diada_compat::parse_real(text.trim().as_bytes()).map_err(|_| {
        format!("Не удалось прочитать число «{text}». Используйте точку для дробной части.")
    })
}
pub fn config(
    fields: &[InputFields],
    positions: usize,
    forces: bool,
) -> Result<AnalysisConfig, String> {
    if positions == 0 || positions > u16::MAX as usize {
        return Err("Число положений должно быть от 1 до 65535.".into());
    }
    Ok(AnalysisConfig {
        positions,
        inputs: fields
            .iter()
            .map(InputFields::series)
            .collect::<Result<_, _>>()?,
        mode: if forces {
            AnalysisMode::Kinetostatics
        } else {
            AnalysisMode::Kinematics
        },
    })
}

pub fn clock() -> ReportClock {
    let time = chrono::Local::now();
    ReportClock {
        year: time.year() as u16,
        month: time.month() as u8,
        day: time.day() as u8,
        hour: time.hour() as u8,
        minute: time.minute() as u8,
        second: time.second() as u8,
    }
}
pub fn report(
    model_name: String,
    source: InputSource,
    time: ReportClock,
    frames: &[CalculationSnapshot],
    selection: &ReportSelection,
    forces: bool,
) -> Result<Vec<u8>, String> {
    let positions = diada_io::position_reports(frames, forces).map_err(|e| e.to_string())?;
    let identity = ReportIdentity::new(env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"))
        .map_err(|e| e.to_string())?;
    diada_io::write_dif_with_identity(
        &ReportHeader {
            mechanism_name: model_name,
            source,
            clock: time,
        },
        &positions,
        selection,
        &TpNumbers,
        identity,
    )
    .map_err(|e| e.to_string())
}

pub fn error(error: &CoreError) -> String {
    match error {
        CoreError::Unassemblable { group } => format!(
            "Механизм не собирается{} в этом положении.",
            group
                .map(|id| format!(": группа {}", id.0 + 1))
                .unwrap_or_default()
        ),
        CoreError::UndefinedPosition { group, reason } => format!(
            "Положение{} не определено. {}",
            group
                .map(|id| format!(" группы {}", id.0 + 1))
                .unwrap_or_default(),
            reason_text(reason)
        ),
        CoreError::Numeric(e) => {
            use diada_compat::NumericError::*;
            format!(
                "{} (ошибка {}).",
                match e {
                    Overflow => "Переполнение числа",
                    Underflow => "Число меньше допустимого диапазона",
                    InvalidOperation => "Недопустимая математическая операция",
                    DivisionByZero => "Деление на ноль",
                },
                e.runtime_code()
            )
        }
        CoreError::InvalidAnalysis(reason) | CoreError::InvalidStructure(reason) => {
            format!("Невозможно выполнить расчёт: {}", reason_text(reason))
        }
        _ => format!("Данные механизма неполны: {error}"),
    }
}
fn reason_text(reason: &str) -> &str {
    match reason {
        "original graph cycle defines exactly one input" => {
            "Исходный графический цикл поддерживает одно ведущее звено. Для нескольких входов используйте расчётную серию."
        }
        "original motion defines exactly one input" => {
            "Исходное движение поддерживает одно ведущее звено. Для нескольких входов используйте расчётную серию."
        }
        "original cyclic motion requires a rotating input" => {
            "Циклическое движение требует вращательного ведущего звена. Используйте расчётную серию для поступательного входа."
        }
        "angle normalization no longer progresses" => {
            "Угол слишком велик для исходной процедуры нормализации."
        }
        "nearly parallel equation directions" => "Направления уравнений почти параллельны.",
        "all input links require a series"
        | "input series missing"
        | "not enough input steps"
        | "input step count differs from input group count" => {
            "Задайте параметры каждого ведущего звена."
        }
        "reactions were not calculated" => "Реакции не рассчитаны.",
        "original force traversal does not solve translational inputs" => {
            "Исходная силовая процедура не определяет реакции поступательного ведущего звена. Кинематика остаётся доступной."
        }
        _ => reason,
    }
}

#[derive(Clone, Debug)]
pub struct SeriesResult {
    pub frames: Vec<Result<CalculationSnapshot, CoreError>>,
    /// Partially defined leading groups, only for a labelled failed preview.
    pub failed_previews: Vec<Option<Mechanism>>,
    /// A force error leaves kinematics usable, but never certifies reactions.
    pub kinematics_valid: Vec<bool>,
    pub config: AnalysisConfig,
    pub clock: ReportClock,
    pub kind: SeriesKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SeriesKind {
    Analysis,
    Motion,
    Graphs,
}
impl SeriesKind {
    pub fn is_analogs(self) -> bool {
        self != Self::Analysis
    }
}

/// A calculation runs on its own thread. Failed positions are represented as
/// errors and cannot quietly become zeros or an earlier valid snapshot.
pub fn calculate(model: Mechanism, config: AnalysisConfig, kind: SeriesKind) -> SeriesResult {
    let mut frames = Vec::with_capacity(config.positions);
    let mut failed_previews = Vec::with_capacity(config.positions);
    let mut kinematics_valid = Vec::with_capacity(config.positions);
    let time = clock();
    match KinematicSampler::new(&model, &config) {
        Ok(mut sampler) => {
            while let Some(frame) = sampler.next_frame() {
                failed_previews.push(frame.is_err().then(|| sampler.preview_mechanism().clone()));
                kinematics_valid.push(sampler.last_kinematics_succeeded());
                frames.push(frame);
            }
        }
        Err(e) => {
            frames.push(Err(e));
            failed_previews.push(None);
            kinematics_valid.push(false);
        }
    }
    SeriesResult {
        frames,
        failed_previews,
        kinematics_valid,
        config,
        clock: time,
        kind,
    }
}

/// Oscillation preserves the original separate pass starts. Merely reversing
/// the 72 displayed indices would omit the exact second boundary.
pub fn calculate_passes(mut model: Mechanism, configs: Vec<AnalysisConfig>) -> SeriesResult {
    let mut configs = configs.into_iter();
    let first = configs
        .next()
        .expect("motion configuration contains at least one pass");
    let mut result = calculate(model.clone(), first, SeriesKind::Motion);
    for config in configs {
        // U01 carries the actual end-of-pass state, including already computed
        // leading groups after a failed pose, into the reverse pass.
        if let Some(last) = result.frames.last().and_then(|f| f.as_ref().ok()) {
            model = last.mechanism.clone();
        } else if let Some(Some(last)) = result.failed_previews.last() {
            model = last.clone();
        }
        let next = calculate(model.clone(), config, SeriesKind::Motion);
        let offset = result.frames.len();
        result.frames.extend(next.frames.into_iter().map(|frame| {
            frame.map(|mut frame| {
                frame.position_index += offset;
                frame
            })
        }));
        result.failed_previews.extend(next.failed_previews);
        result.kinematics_valid.extend(next.kinematics_valid);
        result.config.positions = result.frames.len();
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn actual_application_report_changes_only_the_product_banner_of_the_original_report() {
        let input = include_bytes!("../../../../tests/fixtures/diada-v320-derived/SCENE.DI");
        let expected = include_bytes!("../../../../tests/fixtures/diada-v320-derived/SCENE.DIF");
        let loaded =
            diada_io::load_dialogue(&diada_io::Journal::from_bytes(input), "SCENE", &TpNumbers)
                .unwrap();
        let fields = InputFields::for_model(&loaded.mechanism);
        let config = config(&fields, 12, false).unwrap();
        let result = calculate(loaded.mechanism, config, SeriesKind::Analysis);
        let frames = result
            .frames
            .into_iter()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        let bytes = report(
            "SCENE".into(),
            InputSource::File,
            ReportClock {
                year: 1999,
                month: 3,
                day: 10,
                hour: 10,
                minute: 38,
                second: 0,
            },
            &frames,
            &ReportSelection::default(),
            false,
        )
        .unwrap();
        let legacy_banner = b"Program DIADA v3.20 Serial Number 32D400 ";
        let mut expected_current = Vec::new();
        let mut banners = 0;
        for line in expected.split_inclusive(|byte| *byte == b'\n') {
            if let Some(rest) = line.strip_prefix(legacy_banner) {
                expected_current.extend_from_slice(b"Program diada-rs v0.1.0 ");
                expected_current.extend_from_slice(rest);
                banners += 1;
            } else {
                expected_current.extend_from_slice(line);
            }
        }
        assert_eq!(banners, 2, "Both six-position block banners must change.");
        assert_eq!(
            bytes, expected_current,
            "only the product banner changes; CP866/CRLF, date/source, block boundaries and all numeric bytes remain exact"
        );
        assert!(!bytes.windows(b"32D400".len()).any(|part| part == b"32D400"));
    }
    #[test]
    fn motion_grid_uses_original_five_degree_input_steps_and_normalized_derivatives() {
        let input = include_bytes!("../../../../tests/fixtures/diada-v320-derived/SCENE.DI");
        let loaded =
            diada_io::load_dialogue(&diada_io::Journal::from_bytes(input), "SCENE", &TpNumbers)
                .unwrap();
        let mut fields = InputFields::for_model(&loaded.mechanism);
        fields[0].step = "5".into();
        let result = calculate(
            loaded.mechanism.clone(),
            config(&fields, 72, false).unwrap(),
            SeriesKind::Motion,
        );
        assert_eq!(result.frames.len(), 72);
        let mut direct = loaded.mechanism;
        direct
            .calculate_position(&[InputPosition {
                position: parse("30").unwrap(),
                other_position: Real48::ZERO,
                rate: parse("1").unwrap(),
                rate_derivative: Real48::ZERO,
            }])
            .unwrap();
        assert_eq!(result.frames[6].as_ref().unwrap().mechanism, direct);
    }
}
