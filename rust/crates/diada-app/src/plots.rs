use crate::analysis::{SeriesKind, SeriesResult};
use diada_core::*;
use eframe::egui::{self, Align2, Color32, FontId, Pos2, Rect, Stroke};
pub struct GraphChoice {
    category: GraphCategory,
    object: usize,
    function: usize,
}
impl Default for GraphChoice {
    fn default() -> Self {
        Self {
            category: GraphCategory::Pair,
            object: 0,
            function: 0,
        }
    }
}
impl GraphChoice {
    /// Select a real original graph branch for this app's own capture harness.
    /// A rejected selector leaves the current selection intact.
    pub fn configure(
        &mut self,
        model: &Mechanism,
        mode: AnalysisMode,
        category: &str,
        object: Option<&str>,
        function: Option<&str>,
    ) -> Result<(), String> {
        let category = match category.to_ascii_lowercase().as_str() {
            "pair" => GraphCategory::Pair,
            "link" => GraphCategory::Link,
            "point" => GraphCategory::Point,
            "force" => GraphCategory::Force,
            _ => return Err(format!("Неизвестная категория графика: {category}")),
        };
        if category == GraphCategory::Force && mode != AnalysisMode::Kinetostatics {
            return Err("Силы не определялись".into());
        }
        let objects = model.graph_objects(category);
        if objects.is_empty() {
            return Err(match category {
                GraphCategory::Pair => "Нет вращательных пар",
                GraphCategory::Point => "Точек нет",
                GraphCategory::Force => "Силы не определялись",
                GraphCategory::Link => "Звенья отсутствуют",
            }
            .into());
        }
        let object_index = if let Some(text) = object {
            let name = crate::cli::legacy_name(text)?;
            objects
                .iter()
                .position(|object| {
                    model
                        .graph_object_name(*object)
                        .is_ok_and(|candidate| candidate.0 == name.0)
                })
                .ok_or_else(|| format!("В выбранной категории нет объекта {text}"))?
        } else {
            0
        };
        let values = model
            .graph_values(objects[object_index])
            .map_err(|error| crate::analysis::error(&error))?;
        let commands: &[&str] = match values {
            GraphValues::Cartesian(_) => &["x", "y", "vx", "vy", "ax", "ay"],
            GraphValues::Angular(_) => &["fi", "omega", "eps"],
            GraphValues::Linear(_) => &["s", "u", "u'"],
            GraphValues::Reaction(_) => &["rx", "ry", "m", "hodo"],
        };
        let function_index = if let Some(text) = function {
            let normalized = text.to_ascii_lowercase();
            let normalized = if normalized == "hodograph" {
                "hodo"
            } else {
                &normalized
            };
            commands
                .iter()
                .position(|candidate| *candidate == normalized)
                .ok_or_else(|| format!("Для выбранного объекта нет функции {text}"))?
        } else {
            0
        };
        self.category = category;
        self.object = object_index;
        self.function = function_index;
        Ok(())
    }
}
fn display(n: Real48) -> f64 {
    n.load_extended().to_display_f64()
}
pub fn show(
    ui: &mut egui::Ui,
    series: Option<&SeriesResult>,
    choice: &mut GraphChoice,
    current: &mut usize,
) {
    let Some(series) = series else {
        ui.label("Подготовьте графики в разделе «Движение и графики».");
        return;
    };
    if series.kind != SeriesKind::Graphs {
        ui.label("Подготовьте графики: 73 исходных положения с нормированными производными.");
        return;
    }
    let Some(first) = series.frames.iter().find_map(|f| f.as_ref().ok()) else {
        ui.label("Нет определённых положений.");
        return;
    };
    let model = &first.mechanism;
    ui.horizontal(|ui| {
        for (category, label) in [
            (GraphCategory::Pair, "Пара"),
            (GraphCategory::Link, "Звено"),
            (GraphCategory::Point, "Точка"),
            (GraphCategory::Force, "Сила"),
        ] {
            if ui
                .selectable_value(&mut choice.category, category, label)
                .changed()
            {
                choice.object = 0;
                choice.function = 0;
            }
        }
    });
    let objects = if choice.category == GraphCategory::Force
        && series.config.mode != AnalysisMode::Kinetostatics
    {
        Vec::new()
    } else {
        model.graph_objects(choice.category)
    };
    if objects.is_empty() {
        ui.label(match choice.category {
            GraphCategory::Pair => "Нет вращательных пар",
            GraphCategory::Point => "Точек нет",
            GraphCategory::Force => "Силы не определялись",
            _ => "Звенья отсутствуют",
        });
        return;
    }
    choice.object = choice.object.min(objects.len() - 1);
    egui::ComboBox::from_id_salt("graph_object")
        .selected_text(
            model
                .graph_object_name(objects[choice.object])
                .map(|n| n.display())
                .unwrap_or_default(),
        )
        .show_ui(ui, |ui| {
            for (i, object) in objects.iter().enumerate() {
                ui.selectable_value(
                    &mut choice.object,
                    i,
                    model
                        .graph_object_name(*object)
                        .map(|n| n.display())
                        .unwrap_or_default(),
                );
            }
        });
    let object = objects[choice.object];
    let Ok(sample) = model.graph_values(object) else {
        ui.label("Значения объекта не определены.");
        return;
    };
    let functions: &[&str] = match sample {
        GraphValues::Cartesian(_) => &[
            "X · м",
            "Y · м",
            "Vx · м/с",
            "Vy · м/с",
            "ax · м/с²",
            "ay · м/с²",
        ],
        GraphValues::Angular(_) => &["Fi · рад", "Omega · рад/с", "Eps · рад/с²"],
        GraphValues::Linear(_) => &["S · м", "u · м/с", "u' · м/с²"],
        GraphValues::Reaction(_) => &["Rx · Н", "Ry · Н", "M · Н·м", "Годограф Rx / Ry"],
    };
    choice.function = choice.function.min(functions.len() - 1);
    egui::ComboBox::from_id_salt("graph_function")
        .selected_text(functions[choice.function])
        .show_ui(ui, |ui| {
            for (i, name) in functions.iter().enumerate() {
                ui.selectable_value(&mut choice.function, i, *name);
            }
        });
    let hodo = choice.category == GraphCategory::Force && choice.function == 3;
    let values: Vec<Option<(f64, f64, String)>> = series
        .frames
        .iter()
        .enumerate()
        .map(|(i, frame)| {
            let frame = frame.as_ref().ok()?;
            let values = frame.mechanism.graph_values(object).ok()?;
            let (x, y) = if hodo {
                (Some(values.values()[0]), values.values()[1])
            } else {
                (None, *values.values().get(choice.function)?)
            };
            let number = |value, decimals| {
                diada_compat::format_real(value, 5, Some(decimals))
                    .ok()
                    .map(|text| crate::ui_numbers::compact(&text))
            };
            let shown = if matches!(values, GraphValues::Angular(_)) && choice.function == 0 {
                format!(
                    "{} град",
                    number(diada_core::geometry::wrap_rad(y).ok()?.1, 1)?
                )
            } else if hodo {
                format!("Rx = {}, Ry = {} Н", number(x?, 2)?, number(y, 2)?)
            } else {
                number(y, 2)?
            };
            Some((x.map(display).unwrap_or(i as f64 * 5.0), display(y), shown))
        })
        .collect();
    ui.label(format!(
        "{} · {} · {}",
        model
            .graph_object_name(object)
            .map(|n| n.display())
            .unwrap_or_default(),
        functions[choice.function],
        values
            .get(*current)
            .and_then(|v| v.as_ref())
            .map(|v| v.2.as_str())
            .unwrap_or("***")
    ));
    plot(ui, &values, current, hodo);
}
fn plot(ui: &mut egui::Ui, values: &[Option<(f64, f64, String)>], current: &mut usize, hodo: bool) {
    let (response, painter) = ui.allocate_painter(
        ui.available_size().max(egui::vec2(100.0, 100.0)),
        egui::Sense::click_and_drag(),
    );
    let rect = response.rect.shrink2(egui::vec2(48.0, 30.0));
    let mut min = [f64::INFINITY; 2];
    let mut max = [f64::NEG_INFINITY; 2];
    for (x, y, _) in values.iter().flatten() {
        for (i, v) in [*x, *y].into_iter().enumerate() {
            min[i] = min[i].min(v);
            max[i] = max[i].max(v);
        }
    }
    if !min[0].is_finite() {
        return;
    }
    for i in 0..2 {
        if max[i] == min[i] {
            let gap = max[i].abs().max(1.0) * 0.05;
            min[i] -= gap;
            max[i] += gap;
        }
    }
    if hodo {
        let scale =
            ((max[0] - min[0]) / rect.width() as f64).max((max[1] - min[1]) / rect.height() as f64);
        for i in 0..2 {
            let center = (max[i] + min[i]) / 2.0;
            let half = scale
                * if i == 0 {
                    rect.width() as f64
                } else {
                    rect.height() as f64
                }
                / 2.0;
            min[i] = center - half;
            max[i] = center + half;
        }
    }
    let transform = |x: f64, y: f64| {
        Pos2::new(
            rect.left() + ((x - min[0]) / (max[0] - min[0])) as f32 * rect.width(),
            rect.bottom() - ((y - min[1]) / (max[1] - min[1])) as f32 * rect.height(),
        )
    };
    for j in 0..=5 {
        let f = j as f32 / 5.0;
        let x = rect.left() + rect.width() * f;
        let y = rect.bottom() - rect.height() * f;
        let stroke = Stroke::new(0.7, Color32::from_gray(205));
        painter.line_segment(
            [Pos2::new(x, rect.top()), Pos2::new(x, rect.bottom())],
            stroke,
        );
        painter.line_segment(
            [Pos2::new(rect.left(), y), Pos2::new(rect.right(), y)],
            stroke,
        );
        painter.text(
            Pos2::new(x, rect.bottom() + 8.0),
            Align2::CENTER_TOP,
            format!("{:.1}", min[0] + f as f64 * (max[0] - min[0])),
            FontId::monospace(11.0),
            ui.visuals().text_color(),
        );
        painter.text(
            Pos2::new(rect.left() - 7.0, y),
            Align2::RIGHT_CENTER,
            format!("{:.2}", min[1] + f as f64 * (max[1] - min[1])),
            FontId::monospace(11.0),
            ui.visuals().text_color(),
        );
    }
    let mut previous = None;
    for value in values {
        if let Some((x, y, _)) = value {
            let at = transform(*x, *y);
            if let Some(prev) = previous {
                painter.line_segment([prev, at], Stroke::new(2.0, Color32::from_rgb(0, 119, 127)));
            }
            previous = Some(at);
        } else {
            previous = None;
        }
    }
    if let Some(Some((x, y, _))) = values.get(*current) {
        let at = transform(*x, *y);
        painter.circle_filled(at, 4.0, Color32::from_rgb(191, 86, 24));
        if !hodo {
            painter.line_segment(
                [Pos2::new(at.x, rect.top()), Pos2::new(at.x, rect.bottom())],
                Stroke::new(1.0, Color32::from_rgb(191, 86, 24)),
            );
        }
    }
    if (response.clicked() || response.dragged())
        && let Some(pointer) = response.interact_pointer_pos()
        && let Some((index, _)) = values
            .iter()
            .enumerate()
            .filter_map(|(i, v)| {
                v.as_ref()
                    .map(|(x, y, _)| (i, transform(*x, *y).distance_sq(pointer)))
            })
            .min_by(|a, b| a.1.total_cmp(&b.1))
    {
        *current = index;
    }
    let _ = Rect::NOTHING;
}

#[cfg(test)]
mod tests {
    use super::*;
    use diada_io::{Journal, TpNumbers, load_dialogue};

    fn model(bytes: &[u8]) -> Mechanism {
        load_dialogue(&Journal::from_bytes(bytes), "SCENE", &TpNumbers)
            .unwrap()
            .mechanism
    }

    #[test]
    fn source_categories_distinguish_linear_and_angular_link_functions() {
        let model = model(include_bytes!(
            "../../../../tests/fixtures/diada-v320-derived/dyad-02/SCENE.DI"
        ));
        let mut choice = GraphChoice::default();
        choice
            .configure(
                &model,
                AnalysisMode::Kinematics,
                "link",
                Some("3"),
                Some("u'"),
            )
            .unwrap();
        assert_eq!(choice.category, GraphCategory::Link);
        assert_eq!(choice.function, 2);
        let before = (choice.category, choice.object, choice.function);
        assert!(
            choice
                .configure(
                    &model,
                    AnalysisMode::Kinematics,
                    "link",
                    Some("3"),
                    Some("fi")
                )
                .is_err()
        );
        assert_eq!((choice.category, choice.object, choice.function), before);
        choice
            .configure(
                &model,
                AnalysisMode::Kinematics,
                "link",
                Some("2"),
                Some("omega"),
            )
            .unwrap();
        assert_eq!(choice.function, 1);
        // Grounded C is not an eligible moving revolute pair.
        assert!(
            choice
                .configure(
                    &model,
                    AnalysisMode::Kinematics,
                    "pair",
                    Some("C"),
                    Some("x")
                )
                .is_err()
        );
    }

    #[test]
    fn force_hodograph_requires_calculated_force_context() {
        let model = model(include_bytes!(
            "../../../../tests/fixtures/diada-v320-derived/force-fixed-02/SCENE.DI"
        ));
        let mut choice = GraphChoice::default();
        assert!(
            choice
                .configure(
                    &model,
                    AnalysisMode::Kinematics,
                    "force",
                    Some("c"),
                    Some("hodo")
                )
                .is_err()
        );
        choice
            .configure(
                &model,
                AnalysisMode::Kinetostatics,
                "force",
                Some("c"),
                Some("hodo"),
            )
            .unwrap();
        assert_eq!(choice.category, GraphCategory::Force);
        assert_eq!(choice.function, 3);
        choice
            .configure(
                &model,
                AnalysisMode::Kinetostatics,
                "force",
                Some("c"),
                Some("M"),
            )
            .unwrap();
        assert_eq!(choice.function, 2);
    }

    #[test]
    fn point_selector_and_projection_retain_original_named_object() {
        let model = model(include_bytes!(
            "../../../../tests/fixtures/diada-v320-derived/SCENE.DI"
        ));
        let mut choice = GraphChoice::default();
        choice
            .configure(
                &model,
                AnalysisMode::Kinematics,
                "point",
                Some("s2"),
                Some("Vx"),
            )
            .unwrap();
        let object = model.graph_objects(choice.category)[choice.object];
        assert_eq!(model.graph_object_name(object).unwrap().0, *b"S2");
        assert_eq!(choice.function, 2);
    }
}
