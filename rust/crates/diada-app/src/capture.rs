//! Captures this app's own rendered viewport for visual regression review.
//! This does not inspect or automate any other desktop window.
use eframe::egui;
use std::{io::BufWriter, path::PathBuf};
pub fn window_size() -> Option<[f32; 2]> {
    let args: Vec<_> = std::env::args().collect();
    if !args.iter().any(|s| s == "--capture") {
        return None;
    }
    let at = args.iter().position(|s| s == "--capture-size")?;
    let (width, height) = args.get(at + 1)?.split_once('x')?;
    let size = [width.parse::<f32>().ok()?, height.parse::<f32>().ok()?];
    (size[0].is_finite() && size[1].is_finite() && size[0] >= 840.0 && size[1] >= 560.0)
        .then_some(size)
}
pub struct CaptureGraph {
    pub category: String,
    pub object: Option<String>,
    pub function: Option<String>,
}
pub struct Capture {
    path: PathBuf,
    frames: usize,
    requested: bool,
    pub view: String,
    pub frame: usize,
    pub graph: Option<CaptureGraph>,
    pub highlight: Option<String>,
    pub connection_row: Option<usize>,
}
impl Capture {
    pub fn from_args() -> Option<Self> {
        let args: Vec<_> = std::env::args().collect();
        let at = args.iter().position(|s| s == "--capture")?;
        let path = PathBuf::from(args.get(at + 1)?);
        let view = args
            .iter()
            .position(|s| s == "--view")
            .and_then(|i| args.get(i + 1))
            .cloned()
            .unwrap_or_else(|| "mechanism".into());
        let argument = |flag: &str| {
            args.iter().position(|s| s == flag).map(|index| {
                args.get(index + 1)
                    .filter(|s| !s.starts_with("--"))
                    .cloned()
                    .unwrap_or_default()
            })
        };
        let category = argument("--graph-category");
        let object = argument("--graph-object");
        let function = argument("--graph-function");
        let graph =
            (category.is_some() || object.is_some() || function.is_some()).then(|| CaptureGraph {
                category: category.unwrap_or_else(|| "pair".into()),
                object,
                function,
            });
        Some(Self {
            path,
            frames: 0,
            requested: false,
            view,
            frame: args
                .iter()
                .position(|s| s == "--frame")
                .and_then(|i| args.get(i + 1))
                .and_then(|s| s.parse().ok())
                .unwrap_or(0),
            graph,
            highlight: argument("--highlight"),
            connection_row: argument("--connection-row")
                .and_then(|value| value.parse::<usize>().ok())
                .filter(|row| *row > 0)
                .map(|row| row - 1),
        })
    }
    pub fn update(&mut self, ctx: &egui::Context, ready: bool) {
        let image = ctx.input(|i| {
            i.events.iter().find_map(|e| {
                if let egui::Event::Screenshot { image, .. } = e {
                    Some(image.clone())
                } else {
                    None
                }
            })
        });
        if let Some(image) = image {
            if let Some(parent) = self.path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let result = (|| {
                let file = std::fs::File::create(&self.path)?;
                let mut encoder = png::Encoder::new(
                    BufWriter::new(file),
                    image.size[0] as u32,
                    image.size[1] as u32,
                );
                encoder.set_color(png::ColorType::Rgba);
                encoder.set_depth(png::BitDepth::Eight);
                let mut writer = encoder.write_header()?;
                let pixels = image
                    .pixels
                    .iter()
                    .flat_map(|p| p.to_array())
                    .collect::<Vec<_>>();
                writer.write_image_data(&pixels)?;
                Ok::<_, Box<dyn std::error::Error>>(())
            })();
            if let Err(error) = result {
                eprintln!("Screenshot failed: {error}");
            }
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }
        if ready {
            self.frames += 1;
            if self.frames >= 6 && !self.requested {
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
                self.requested = true;
            }
        }
        ctx.request_repaint();
    }
}
