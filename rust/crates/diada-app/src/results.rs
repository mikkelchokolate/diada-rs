use crate::drawing::ObjectKey;
use diada_io::{NumberFormatter, PositionReport, ReportNumber, TpNumbers};
use eframe::egui::{self, RichText};
fn number(value: ReportNumber, decimals: i16) -> String {
    TpNumbers
        .format(value, 0, decimals)
        .map(|text| crate::ui_numbers::compact(&text))
        .unwrap_or_else(|_| "***".into())
}
pub fn show(
    ui: &mut egui::Ui,
    report: &PositionReport,
    model: &diada_core::Mechanism,
    selected: &mut Option<ObjectKey>,
) {
    egui::ScrollArea::both()
        .id_salt("numeric_results")
        .show(ui, |ui| {
            ui.heading("Звенья");
            egui::Grid::new("link_results")
                .striped(true)
                .show(ui, |ui| {
                    for heading in ["Имя", "Fi · град", "Omega · рад/с", "Eps · рад/с²"]
                    {
                        ui.strong(heading);
                    }
                    ui.end_row();
                    for link in &report.links {
                        let key = model
                            .link_by_name(diada_core::Name(link.name.0))
                            .map(|id| ObjectKey::Link(id.0));
                        if ui
                            .selectable_label(*selected == key, link.name.text())
                            .clicked()
                        {
                            *selected = key;
                        }
                        for (value, precision) in
                            [(link.angle_degrees, 2), (link.omega, 3), (link.eps, 3)]
                        {
                            ui.monospace(number(value, precision));
                        }
                        ui.end_row();
                    }
                });
            if let Some(moment) = &report.balance_moment {
                ui.add_space(10.0);
                ui.label(format!(
                    "Уравновешивающий момент на звене {}: {} Н·м",
                    moment.on_link.text(),
                    number(moment.value, 4)
                ));
            }
            ui.add_space(15.0);
            ui.heading("Кинематические пары и точки");
            ui.small("Координаты — м; скорость — м/с; ускорение — м/с²; направления — град.");
            egui::Grid::new("point_results")
                .striped(true)
                .show(ui, |ui| {
                    for heading in [
                        "Имя", "X", "Y", "|V|", "уг. V", "Vx", "Vy", "|a|", "уг. a", "ax", "ay",
                    ] {
                        ui.strong(heading);
                    }
                    ui.end_row();
                    for point in &report.points {
                        let key = model
                            .pair_by_name(diada_core::Name(point.name.0))
                            .map(|id| ObjectKey::Pair(id.0));
                        if ui
                            .selectable_label(*selected == key, point.name.text())
                            .clicked()
                        {
                            *selected = key;
                        }
                        for (j, value) in point.values.iter().enumerate() {
                            ui.monospace(number(*value, if j == 3 || j == 7 { 2 } else { 4 }));
                        }
                        ui.end_row();
                    }
                });
            if !report.reactions.is_empty() {
                ui.add_space(15.0);
                ui.heading("Реакции");
                for reaction in &report.reactions {
                    ui.label(
                        RichText::new(format!(
                            "Пара {}: на звено {} со стороны {}",
                            reaction.name.text(),
                            reaction.on_link.text(),
                            reaction.from_link.text()
                        ))
                        .strong(),
                    );
                    ui.horizontal(|ui| {
                        for (i, label) in ["|R|, Н", "уг. R, град", "Rx, Н", "Ry, Н"]
                            .iter()
                            .enumerate()
                        {
                            ui.label(format!(
                                "{label}: {}",
                                number(reaction.values[i], if i == 1 { 2 } else { 4 })
                            ));
                        }
                    });
                    if let Some(moment) = reaction.moment {
                        ui.label(format!("Момент реакции: {} Н·м", number(moment, 4)));
                    }
                    match &reaction.application {
                        diada_io::ReactionApplication::Rotation => {}
                        diada_io::ReactionApplication::PerpendicularFoot {
                            rotating_pair,
                            guide,
                        } => {
                            ui.small(format!(
                                "Основание перпендикуляра из пары {} на направляющую {}",
                                rotating_pair.text(),
                                guide.text()
                            ));
                        }
                        diada_io::ReactionApplication::GuideIntersection { first, second } => {
                            ui.small(format!(
                                "Пересечение направляющих {} и {}",
                                first.text(),
                                second.text()
                            ));
                        }
                    }
                    ui.separator();
                }
            }
        });
}
