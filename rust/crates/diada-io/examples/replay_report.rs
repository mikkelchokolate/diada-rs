use diada_core::{CalculationSnapshot, InputPosition};
use diada_io::{
    DialogueNumbers, EditableDialogue, InputSource, LegacyName, ReportClock, ReportHeader,
    ReportSelection, TpNumbers, position_reports, write_dif,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() < 3 {
        return Err(
            "usage: replay_report input.DI output.DIF [positions [start step rate acceleration]]"
                .into(),
        );
    }
    let codec = TpNumbers;
    let title = std::path::Path::new(&args[1])
        .file_stem()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let document = EditableDialogue::open(&std::fs::read(&args[1])?, &title, &codec)?;
    let mut model = document.loaded().mechanism.clone();
    let forces = model.force_analysis;
    println!(
        "{} links, {} pairs, {} groups, {} points, {} raw field bindings",
        model.links.len(),
        model.pairs.len(),
        model.groups.len(),
        model.points.len(),
        document.bindings().len()
    );
    let positions = args
        .get(3)
        .map(|s| s.parse::<usize>())
        .transpose()?
        .unwrap_or(12);
    let value = |index, default: &str| {
        codec
            .parse_real(
                args.get(index)
                    .map(String::as_str)
                    .unwrap_or(default)
                    .as_bytes(),
            )
            .map_err(|error| -> Box<dyn std::error::Error> { error.into() })
    };
    let mut position = value(4, "0")?;
    let step = value(5, "30")?;
    let rate = value(6, "1")?;
    let rate_derivative = value(7, "0")?;
    let mut snapshots = Vec::new();
    for index in 0..positions {
        let inputs = vec![
            InputPosition {
                position,
                other_position: diada_core::Real48::ZERO,
                rate,
                rate_derivative
            };
            model.inputs.len()
        ];
        model.calculate_position(&inputs)?;
        if forces {
            model.calculate_forces()?;
        }
        snapshots.push(CalculationSnapshot {
            position_index: index + 1,
            inputs,
            mechanism: model.clone(),
        });
        position = position
            .load_extended()
            .checked_add(step.load_extended())?
            .store_real()?;
    }
    let report = position_reports(&snapshots, forces)?;
    let header = ReportHeader {
        mechanism_name: title,
        source: InputSource::File,
        clock: ReportClock {
            year: 1999,
            month: 3,
            day: 10,
            hour: 10,
            minute: 38,
            second: 0,
        },
    };
    let names = |index: usize| {
        args.get(index)
            .map(|text| {
                text.split(',')
                    .filter(|text| !text.is_empty())
                    .map(LegacyName::from_text)
                    .collect::<Result<Vec<_>, _>>()
            })
            .transpose()
    };
    let selection = ReportSelection {
        links: names(8)?,
        points: names(9)?,
        reactions: names(10)?,
    };
    let output = write_dif(&header, &report, &selection, &codec)?;
    std::fs::write(&args[2], &output)?;
    println!(
        "{} positions, {} report bytes",
        snapshots.len(),
        output.len()
    );
    Ok(())
}
