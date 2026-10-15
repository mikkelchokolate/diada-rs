#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod analysis;
mod capture;
mod cli;
mod drawing;
mod dyad_feedback;
mod failure;
mod fields;
mod live_preview;
mod plots;
mod preview;
mod results;
mod schematic_layout;
mod structure_form;
mod trajectory_trace;
mod ui_numbers;
mod workbench;

fn main() -> eframe::Result {
    if let Some(result) = cli::run() {
        if let Err(error) = result {
            eprintln!("{error}");
            std::process::exit(1);
        }
        return Ok(());
    }
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("ДиАДА")
            .with_inner_size(capture::window_size().unwrap_or([1320.0, 840.0]))
            .with_min_inner_size([840.0, 560.0]),
        ..Default::default()
    };
    eframe::run_native(
        "diada",
        options,
        Box::new(|context| Ok(Box::new(workbench::Workbench::new(context)))),
    )
}
