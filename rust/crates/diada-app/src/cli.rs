//! Standalone batch interface also used by reproducible whole-session checks.
use crate::analysis::{self, InputFields};
use diada_io::*;
use std::path::Path;

/// Use the original two-byte keyboard name normalization, including its
/// historical >'Z' subtraction. General Unicode uppercasing changes that rule.
pub(crate) fn legacy_name(text: &str) -> Result<LegacyName, String> {
    let mut writer = JournalWriter::new();
    writer.write_name(LegacyName::from_text(text).map_err(|e| e.to_string())?);
    writer
        .into_journal()
        .cursor()
        .read_name()
        .map_err(|e| e.to_string())
}

fn distinct_report_paths(input: &Path, output: &Path) -> Result<(), String> {
    let input = std::fs::canonicalize(input).map_err(|e| e.to_string())?;
    let output = match std::fs::canonicalize(output) {
        Ok(path) => path,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.to_string()),
    };
    let same = if cfg!(windows) {
        input
            .to_string_lossy()
            .eq_ignore_ascii_case(&output.to_string_lossy())
    } else {
        input == output
    };
    if same {
        Err("Файл отчёта совпадает с входным механизмом. Выберите другое имя .DIF.".into())
    } else {
        Ok(())
    }
}

pub fn run() -> Option<Result<(), String>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args
        .first()
        .is_some_and(|s| matches!(s.as_str(), "--version" | "-V"))
    {
        println!("{} {}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));
        return Some(Ok(()));
    }
    if args.first().is_some_and(|s| s == "--help") {
        println!(
            "{} {}\n\ndiada-rs [mechanism.DI]\ndiada-rs --report INPUT.DI OUTPUT.DIF [--positions N] [--input NAME START RATE ACCELERATION STEP] [--forces] [--clock YYYY-MM-DDTHH:MM:SS]\ndiada-rs --version\nWithout --input: standard 12 positions (0 degrees, step 30, speed 1, acceleration 0). --input angles in degrees, rate in rad/s; translational values in m, m/s. --clock is for reproducible comparison.",
            env!("CARGO_PKG_NAME"),
            env!("CARGO_PKG_VERSION")
        );
        return Some(Ok(()));
    }
    if args.first().is_none_or(|s| s != "--report") {
        return None;
    }
    Some((|| {
        if args.len() < 3 {
            return Err("--report требует входной .DI и выходной .DIF.".into());
        }
        distinct_report_paths(Path::new(&args[1]), Path::new(&args[2]))?;
        let bytes = std::fs::read(&args[1]).map_err(|e| e.to_string())?;
        let title = std::path::Path::new(&args[1])
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        let loaded = load_dialogue(&Journal::from_bytes(&bytes), title.clone(), &TpNumbers)
            .map_err(|e| e.to_string())?;
        let mut inputs = InputFields::for_model(&loaded.mechanism);
        let mut positions = 12;
        let mut forces = false;
        let mut clock = analysis::clock();
        let mut index = 3;
        while index < args.len() {
            match args[index].as_str() {
                "--forces" => {
                    forces = true;
                    index += 1;
                }
                "--positions" => {
                    positions = args
                        .get(index + 1)
                        .ok_or("Missing --positions number")?
                        .parse()
                        .map_err(|_| "Invalid --positions number")?;
                    index += 2;
                }
                "--clock" => {
                    let value = args.get(index + 1).ok_or("Missing --clock")?;
                    let time = chrono::NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H:%M:%S")
                        .map_err(|e| e.to_string())?;
                    use chrono::{Datelike, Timelike};
                    clock = ReportClock {
                        year: time.year() as u16,
                        month: time.month() as u8,
                        day: time.day() as u8,
                        hour: time.hour() as u8,
                        minute: time.minute() as u8,
                        second: time.second() as u8,
                    };
                    index += 2;
                }
                "--input" => {
                    if args.len() < index + 6 {
                        return Err("--input NAME START RATE ACCELERATION STEP".into());
                    }
                    let name = legacy_name(&args[index + 1])?;
                    let input = inputs
                        .iter_mut()
                        .find(|f| loaded.mechanism.links[f.link.0].name.0 == name.0)
                        .ok_or("Unknown input link")?;
                    input.start = args[index + 2].clone();
                    input.rate = args[index + 3].clone();
                    input.acceleration = args[index + 4].clone();
                    input.step = args[index + 5].clone();
                    index += 6;
                }
                option => return Err(format!("Неизвестный параметр {option}")),
            }
        }
        if forces && !loaded.mechanism.force_analysis {
            return Err("В механизме не заданы данные силового анализа.".into());
        }
        let config = analysis::config(&inputs, positions, forces)?;
        let frames = loaded
            .mechanism
            .sample_kinematics(&config)
            .map_err(|e| analysis::error(&e))?;
        let output = analysis::report(
            title,
            InputSource::File,
            clock,
            &frames,
            &ReportSelection::default(),
            forces,
        )?;
        std::fs::write(&args[2], &output).map_err(|e| e.to_string())?;
        println!("{} positions; {} bytes written", frames.len(), output.len());
        Ok(())
    })())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn input_selector_matches_original_keyboard_case_and_symbol_rules() {
        assert_eq!(legacy_name("aB").unwrap(), LegacyName(*b"AB"));
        assert_eq!(legacy_name("q").unwrap(), LegacyName(*b"Q "));
        // Original ENGINE accepts '[' and subtracts 32, rather than applying
        // modern ASCII alphabet-only uppercase.
        assert_eq!(legacy_name("[").unwrap(), LegacyName(*b"; "));
    }

    #[test]
    fn report_cannot_replace_its_resolved_input() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory =
            std::env::temp_dir().join(format!("diada-report-path-{}-{unique}", std::process::id()));
        std::fs::create_dir(&directory).unwrap();
        let input = directory.join("model.DI");
        std::fs::write(&input, b"original mechanism").unwrap();
        assert!(distinct_report_paths(&input, &directory.join(".").join("model.DI")).is_err());
        #[cfg(windows)]
        assert!(distinct_report_paths(&input, &directory.join("MODEL.di")).is_err());
        assert!(distinct_report_paths(&input, &directory.join("model.DIF")).is_ok());
        assert_eq!(std::fs::read(&input).unwrap(), b"original mechanism");
        std::fs::remove_file(input).unwrap();
        std::fs::remove_dir(directory).unwrap();
    }
}
