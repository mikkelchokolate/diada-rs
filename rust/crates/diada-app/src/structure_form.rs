//! A table changes only the pair-entry part of the original dialogue. Ground,
//! input links, dimensions and loads remain questions of the same controller.
use crate::cli::legacy_name;
use diada_core::{LinkId, Mechanism, Name, PairId, PairKind};
use diada_io::{
    DialogueController, DialogueState, FieldPath, JournalWriter, LegacyName, TpNumbers,
};
use eframe::egui::{self, Color32, RichText};
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FormAction {
    Apply,
    Cancel,
}

#[derive(Clone, Debug, Default)]
struct NameCell {
    text: String,
    /// Imported names are already normalized. Keep their identity when an
    /// unchanged displayed symbol would otherwise normalize a second time.
    imported: Option<LegacyName>,
}
impl NameCell {
    fn imported(name: Name) -> Self {
        let imported = LegacyName(name.0);
        Self {
            text: imported.text().trim_end_matches(' ').into(),
            imported: Some(imported),
        }
    }
    fn parse(&self, label: &str) -> Result<ParsedName, String> {
        if let Some(name) = self.imported
            && self.text == name.text().trim_end_matches(' ')
        {
            if name.is_blank() {
                return Err(format!("Введите {label}."));
            }
            if name.0.iter().any(|c| !(b' '..=b'_').contains(c)) {
                return Err(format!(
                    "{label}: имя не относится к исходной клавиатурной кодировке."
                ));
            }
            // ENGINE subtracts 32 once for each keyboard byte above 'Z'.
            // Stored '['..'_' therefore need raw '{'..DEL when rewritten.
            let raw = name.0.map(|c| if c > b'Z' { c + 32 } else { c });
            return Ok(ParsedName {
                normalized: name,
                raw: LegacyName(raw),
            });
        }
        let normalized = legacy_name(&self.text).map_err(|_| {
            format!("{label}: нужно имя из одного или двух символов исходной клавиатуры.")
        })?;
        if normalized.is_blank() {
            return Err(format!("Введите {label}; пробел не является именем."));
        }
        let raw = LegacyName::from_text(&self.text)
            .map_err(|_| format!("{label}: имя содержит недопустимые символы."))?;
        if raw.0.iter().any(|c| !(32..=127).contains(c)) {
            return Err(format!(
                "{label}: допустимы один или два символа исходной клавиатуры."
            ));
        }
        Ok(ParsedName { normalized, raw })
    }
}

#[derive(Clone, Debug, Default)]
struct PairRow {
    pair: NameCell,
    kind: Option<u8>,
    a: NameCell,
    b: NameCell,
}
impl PairRow {
    fn parse(&self) -> Result<ParsedRow, String> {
        let kind = self
            .kind
            .filter(|kind| matches!(kind, b'R' | b'P'))
            .ok_or_else(|| "Выберите тип пары: R — шарнир или P — ползун.".to_string())?;
        let pair = self.pair.parse("имя пары")?;
        let a = self.a.parse("имя первого звена")?;
        let b = self.b.parse("имя второго звена")?;
        if a.normalized == b.normalized {
            return Err("Пара должна соединять два разных звена.".into());
        }
        Ok(ParsedRow { pair, kind, a, b })
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ParsedName {
    normalized: LegacyName,
    raw: LegacyName,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ParsedRow {
    pair: ParsedName,
    kind: u8,
    a: ParsedName,
    b: ParsedName,
}
impl ParsedRow {
    fn ends(self) -> [[u8; 2]; 2] {
        let mut ends = [self.a.normalized.0, self.b.normalized.0];
        ends.sort();
        ends
    }
}

#[derive(Clone, Debug)]
pub struct StructureForm {
    title: String,
    rows: Vec<PairRow>,
    preview: Mechanism,
    errors: Vec<Option<String>>,
    show_errors: bool,
    global_error: Option<String>,
    focus_new_row: Option<usize>,
    selected_row: Option<usize>,
}
impl StructureForm {
    pub fn new(title: impl Into<String>) -> Self {
        let title = title.into();
        let mut form = Self {
            preview: Mechanism {
                title: title.clone(),
                ..Mechanism::default()
            },
            title,
            rows: vec![PairRow::default()],
            errors: Vec::new(),
            show_errors: false,
            global_error: None,
            focus_new_row: None,
            selected_row: None,
        };
        form.refresh_preview();
        form
    }
    pub fn from_model(model: &Mechanism) -> Self {
        let mut form = Self::new(model.title.clone());
        form.rows = model
            .pairs
            .iter()
            .filter(|pair| matches!(pair.kind, PairKind::Revolute | PairKind::Prismatic))
            .map(|pair| PairRow {
                pair: NameCell::imported(pair.name),
                kind: Some(if pair.kind == PairKind::Revolute {
                    b'R'
                } else {
                    b'P'
                }),
                a: model
                    .link(pair.links[0])
                    .map(|link| NameCell::imported(link.name))
                    .unwrap_or_default(),
                b: model
                    .link(pair.links[1])
                    .map(|link| NameCell::imported(link.name))
                    .unwrap_or_default(),
            })
            .collect();
        form.refresh_preview();
        form
    }
    pub fn preview(&self) -> &Mechanism {
        &self.preview
    }
    pub fn select_row(&mut self, index: usize) {
        self.selected_row = (index < self.rows.len()).then_some(index);
    }
    /// Resolve the actual cached objects, rather than treating a table index or
    /// a shared multi-link hinge name as a unique pair identifier.
    pub fn selected_preview_objects(&self) -> Option<(PairId, [LinkId; 2])> {
        let index = self.selected_row?;
        if self.errors.get(index)?.is_some() {
            return None;
        }
        let row = self.rows.get(index)?.parse().ok()?;
        let kind = if row.kind == b'R' {
            PairKind::Revolute
        } else {
            PairKind::Prismatic
        };
        let wanted = [Name(row.a.normalized.0), Name(row.b.normalized.0)];
        self.preview
            .pairs
            .iter()
            .enumerate()
            .find_map(|(index, pair)| {
                let names = [
                    self.preview.link(pair.links[0])?.name,
                    self.preview.link(pair.links[1])?.name,
                ];
                (pair.name == Name(row.pair.normalized.0)
                    && pair.kind == kind
                    && (names == wanted || names == [wanted[1], wanted[0]]))
                .then_some((PairId(index), pair.links))
            })
    }
    fn remove_row(&mut self, index: usize) {
        self.rows.remove(index);
        for selected in [&mut self.selected_row, &mut self.focus_new_row] {
            *selected = selected.and_then(|selected| match selected.cmp(&index) {
                std::cmp::Ordering::Less => Some(selected),
                std::cmp::Ordering::Equal => None,
                std::cmp::Ordering::Greater => Some(selected - 1),
            });
        }
    }
    pub fn set_error(&mut self, error: impl Into<String>) {
        self.global_error = Some(error.into());
        self.show_errors = true;
    }

    fn validation(&self) -> (Vec<Option<ParsedRow>>, Vec<Option<String>>) {
        let mut parsed = Vec::with_capacity(self.rows.len());
        let mut errors = Vec::with_capacity(self.rows.len());
        for row in &self.rows {
            match row.parse() {
                Ok(row) => {
                    parsed.push(Some(row));
                    errors.push(None);
                }
                Err(error) => {
                    parsed.push(None);
                    errors.push(Some(error));
                }
            }
        }
        let mut connections = HashMap::new();
        let mut names = HashMap::<[u8; 2], (u8, usize)>::new();
        for (index, row) in parsed
            .iter()
            .enumerate()
            .filter_map(|(i, r)| r.map(|r| (i, r)))
        {
            if let Some(previous) = connections.insert(row.ends(), index) {
                errors[index] = Some(format!(
                    "Эти звенья уже соединены в строке {}. Измените или удалите одну из строк.",
                    previous + 1
                ));
                errors[previous] = Some(format!(
                    "Эта связь повторяется в строке {}. Для переименования исправьте существующую строку.",
                    index + 1
                ));
            }
            if let Some((kind, previous)) = names.get(&row.pair.normalized.0) {
                if *kind != row.kind {
                    errors[index] = Some(format!(
                        "Имя пары {} уже имеет другой тип в строке {}.",
                        row.pair.normalized.text().trim(),
                        previous + 1
                    ));
                    errors[*previous] = Some(format!(
                        "То же имя пары имеет другой тип в строке {}.",
                        index + 1
                    ));
                }
            } else {
                names.insert(row.pair.normalized.0, (row.kind, index));
            }
            // Equal R names on different links are original multi-link joint
            // occurrences, not duplicate rows. Equal P occurrences likewise
            // remain subject to the original controller's structural rules.
        }
        for (row, error) in parsed.iter_mut().zip(&errors) {
            if error.is_some() {
                *row = None;
            }
        }
        (parsed, errors)
    }

    fn controller(
        &self,
        rows: &[ParsedRow],
        complete: bool,
    ) -> Result<(DialogueController, DialogueState), String> {
        let mut writer = JournalWriter::new();
        writer
            .write_text("_Начало_описания_пар")
            .map_err(|e| e.to_string())?;
        for row in rows {
            writer.write_character(row.kind);
            writer.write_name(row.a.raw);
            writer.write_name(row.b.raw);
            writer.write_name(row.pair.raw);
        }
        if complete {
            writer.write_character(b' ');
            writer
                .write_text("_Конец_описания_пар")
                .map_err(|e| e.to_string())?;
            writer.write_yes_no(true);
        }
        let mut controller = DialogueController::resume(writer.into_journal(), self.title.clone());
        let state = controller
            .next(&TpNumbers)
            .map_err(|e| format!("Описание пар не принято: {e}"))?;
        let expected = if complete {
            FieldPath::Ground
        } else {
            FieldPath::PairCommand(rows.len())
        };
        match &state {
            DialogueState::Question { question, preview }
                if question.path == expected && preview.pairs.len() == rows.len() => {}
            DialogueState::Question { question, .. } => {
                return Err(format!(
                    "Исходный диалог требует уточнения: {}",
                    question.text
                ));
            }
            _ => {
                return Err(
                    "Описание пар не завершилось на ожидаемом вопросе исходного диалога.".into(),
                );
            }
        }
        Ok((controller, state))
    }
    fn refresh_preview(&mut self) {
        let (rows, errors) = self.validation();
        self.errors = errors;
        let valid: Vec<_> = rows.into_iter().flatten().collect();
        self.preview = self
            .controller(&valid, false)
            .ok()
            .and_then(|(controller, _)| controller.preview().cloned())
            .unwrap_or_else(|| Mechanism {
                title: self.title.clone(),
                ..Mechanism::default()
            });
    }
    /// No ground, input, numerical data or source history is fabricated here.
    pub fn commit(&self) -> Result<(DialogueController, DialogueState), String> {
        if self.rows.is_empty() {
            return Err("Добавьте хотя бы одну кинематическую пару.".into());
        }
        let (rows, errors) = self.validation();
        if let Some((index, error)) = errors
            .iter()
            .enumerate()
            .find_map(|(i, e)| e.as_ref().map(|e| (i, e)))
        {
            return Err(format!("Строка {}: {error}", index + 1));
        }
        let (controller, state) =
            self.controller(&rows.into_iter().flatten().collect::<Vec<_>>(), true)?;
        if let Some(model) = controller.preview()
            && model.formal_mobility() <= 0
        {
            return Err(crate::fields::mobility_error(model));
        }
        Ok((controller, state))
    }
    /// Equal semantic structure preserves the existing document and its raw
    /// answers/history. It does not rewrite an untouched legacy journal.
    pub fn same_structure(&self, model: &Mechanism) -> bool {
        let expected: Vec<_> = model
            .pairs
            .iter()
            .filter(|pair| matches!(pair.kind, PairKind::Revolute | PairKind::Prismatic))
            .collect();
        if expected.len() != self.rows.len() {
            return false;
        }
        self.rows.iter().zip(expected).all(|(row, pair)| {
            row.parse().is_ok_and(|row| {
                row.pair.normalized.0 == pair.name.0
                    && row.kind
                        == if pair.kind == PairKind::Revolute {
                            b'R'
                        } else {
                            b'P'
                        }
                    && model
                        .link(pair.links[0])
                        .is_some_and(|link| row.a.normalized.0 == link.name.0)
                    && model
                        .link(pair.links[1])
                        .is_some_and(|link| row.b.normalized.0 == link.name.0)
            })
        })
    }

    pub fn ui(&mut self, ui: &mut egui::Ui) -> Option<FormAction> {
        ui.heading("Все пары и звенья");
        ui.label("Укажите имя пары, тип R / P и два соединяемых звена. Стойка, размеры и нагрузки вводятся после применения.");
        ui.small("Слева — связи заполненных строк; положения и размеры ещё не заданы.");
        ui.add_space(ui.spacing().item_spacing.y);
        let mut changed = false;
        let mut remove = None;
        let focus_new_row = self.focus_new_row.take();
        let previous_selection = self.selected_row;
        let mut selected_row = self.selected_row;
        let mut footer_rows = 1;
        let mut row_width = 0.0;
        let mut row_height = ui.spacing().interact_size.y;
        for label in ["Добавить пару", "Применить структуру", "Отмена"]
        {
            let galley = ui.painter().layout_no_wrap(
                label.into(),
                egui::TextStyle::Button.resolve(ui.style()),
                ui.visuals().text_color(),
            );
            let width = (galley.size().x + 2.0 * ui.spacing().button_padding.x)
                .max(ui.spacing().interact_size.x);
            row_height = row_height.max(galley.size().y + 2.0 * ui.spacing().button_padding.y);
            let spacing = if row_width > 0.0 {
                ui.spacing().item_spacing.x
            } else {
                0.0
            };
            if row_width > 0.0 && row_width + spacing + width > ui.available_width() {
                footer_rows += 1;
                row_width = width;
            } else {
                row_width += spacing + width;
            }
        }
        let footer_height =
            footer_rows as f32 * row_height + footer_rows as f32 * ui.spacing().item_spacing.y;
        let grid_height = (ui.available_height() - footer_height).max(0.0);
        egui::ScrollArea::both()
            .id_salt("structure_form_scroll")
            .max_height(grid_height)
            .min_scrolled_height(0.0)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                egui::Grid::new("structure_form_rows")
                    .with_row_color(move |row, style| {
                        if row > 0 && previous_selection == Some(row - 1) {
                            Some(Color32::from_rgba_unmultiplied(0, 125, 130, 32))
                        } else if row % 2 == 1 {
                            Some(style.visuals.faint_bg_color)
                        } else {
                            None
                        }
                    })
                    .spacing(egui::vec2(10.0, 7.0))
                    .show(ui, |ui| {
                        for label in ["Пара", "Тип", "Звено 1", "Звено 2", ""] {
                            ui.strong(label);
                        }
                        ui.end_row();
                        for (index, row) in self.rows.iter_mut().enumerate() {
                            let first_cell = ui.horizontal(|ui| {
                                let row_number = format!("{}.", index + 1);
                                let color = if self.show_errors
                                    && self.errors.get(index).is_some_and(|e| e.is_some())
                                {
                                    Color32::from_rgb(172, 49, 41)
                                } else if selected_row == Some(index) {
                                    Color32::from_rgb(0, 125, 130)
                                } else {
                                    ui.visuals().text_color()
                                };
                                if ui
                                    .add(
                                        egui::Button::new(
                                            RichText::new(row_number).small().color(color),
                                        )
                                        .frame(false),
                                    )
                                    .clicked()
                                {
                                    selected_row = Some(index);
                                }
                                let response = ui.add(
                                    egui::TextEdit::singleline(&mut row.pair.text)
                                        .desired_width(55.0),
                                );
                                changed |= response.changed();
                                if response.gained_focus()
                                    || response.clicked()
                                    || response.changed()
                                {
                                    selected_row = Some(index);
                                }
                                if focus_new_row == Some(index) {
                                    response.request_focus();
                                    response.scroll_to_me(Some(egui::Align::Center));
                                    selected_row = Some(index);
                                }
                            });
                            let before = row.kind;
                            let kind_response =
                                egui::ComboBox::from_id_salt(("structure_kind", index))
                                    .selected_text(match row.kind {
                                        Some(b'R') => "R · шарнир",
                                        Some(b'P') => "P · ползун",
                                        _ => "R / P",
                                    })
                                    .show_ui(ui, |ui| {
                                        for (kind, label) in
                                            [(b'R', "R · шарнир"), (b'P', "P · ползун")]
                                        {
                                            let response = ui.selectable_value(
                                                &mut row.kind,
                                                Some(kind),
                                                label,
                                            );
                                            if response.clicked() || response.gained_focus() {
                                                selected_row = Some(index);
                                            }
                                        }
                                    });
                            if kind_response.response.clicked()
                                || kind_response.response.gained_focus()
                                || before != row.kind
                            {
                                selected_row = Some(index);
                            }
                            changed |= before != row.kind;
                            let a_response = ui.add(
                                egui::TextEdit::singleline(&mut row.a.text).desired_width(65.0),
                            );
                            let b_response = ui.add(
                                egui::TextEdit::singleline(&mut row.b.text).desired_width(65.0),
                            );
                            for response in [&a_response, &b_response] {
                                changed |= response.changed();
                                if response.gained_focus()
                                    || response.clicked()
                                    || response.changed()
                                {
                                    selected_row = Some(index);
                                }
                            }
                            let delete_response = ui.button("Удалить");
                            if delete_response.clicked() {
                                remove = Some(index);
                            }
                            let row_rect = first_cell
                                .response
                                .rect
                                .union(kind_response.response.rect)
                                .union(a_response.rect)
                                .union(b_response.rect)
                                .union(delete_response.rect);
                            // Observe background clicks without registering an
                            // overlapping widget that would steal cell input.
                            if ui.is_enabled()
                                && ui.rect_contains_pointer(row_rect)
                                && ui.input(|i| i.pointer.primary_clicked())
                            {
                                selected_row = Some(index);
                            }
                            ui.end_row();
                        }
                    });
                if self.show_errors {
                    for (index, error) in self.errors.iter().enumerate() {
                        if let Some(error) = error {
                            ui.label(
                                RichText::new(format!("Строка {}: {error}", index + 1))
                                    .color(Color32::from_rgb(172, 49, 41)),
                            );
                        }
                    }
                }
                if let Some(error) = &self.global_error {
                    ui.colored_label(Color32::from_rgb(172, 49, 41), error);
                }
            });
        self.selected_row = selected_row;
        if let Some(index) = remove {
            self.remove_row(index);
            changed = true;
        }
        let mut action = None;
        ui.horizontal_wrapped(|ui| {
            if ui.button("Добавить пару").clicked() {
                self.rows.push(PairRow::default());
                self.focus_new_row = Some(self.rows.len() - 1);
                self.selected_row = self.focus_new_row;
                ui.ctx().request_repaint();
                changed = true;
            }
            if ui.button("Применить структуру").clicked() {
                self.show_errors = true;
                action = Some(FormAction::Apply);
            }
            if ui.button("Отмена").clicked() {
                action = Some(FormAction::Cancel);
            }
        });
        if changed {
            self.global_error = None;
            self.refresh_preview();
        }
        if self.selected_row != previous_selection {
            ui.ctx().request_repaint();
        }
        action
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> Mechanism {
        diada_io::load_dialogue(
            &diada_io::Journal::from_bytes(include_bytes!(
                "../../../../tests/fixtures/diada-v320-derived/dyad-01/SCENE.DI"
            )),
            "SCENE",
            &TpNumbers,
        )
        .unwrap()
        .mechanism
    }
    #[test]
    fn batch_entry_preserves_original_structure_prefix_and_stops_at_ground() {
        let model = fixture();
        let form = StructureForm::from_model(&model);
        let (controller, state) = form.commit().unwrap();
        assert!(
            matches!(state, DialogueState::Question { question, .. } if question.path == FieldPath::Ground)
        );
        let original = diada_io::Journal::from_bytes(include_bytes!(
            "../../../../tests/fixtures/diada-v320-derived/dyad-01/SCENE.DI"
        ));
        let end = original
            .lines()
            .iter()
            .position(|line| line.text() == "_Ввод_стойки")
            .unwrap();
        let expected: Vec<u8> = original.lines()[..=end]
            .iter()
            .flat_map(|line| {
                let mut b = line.bytes.clone();
                b.extend_from_slice(b"\r\n");
                b
            })
            .collect();
        assert_eq!(controller.current_journal().to_bytes(), expected);
        assert_eq!(form.preview().pairs.len(), model.pairs.len());
        assert!(
            form.preview().ground.is_none()
                && form.preview().inputs.is_empty()
                && form.preview().groups.is_empty()
        );
        assert!(form.same_structure(&model));
    }
    #[test]
    fn invalid_connection_does_not_change_the_accepted_model_or_invent_a_preview_row() {
        let model = fixture();
        let original = model.clone();
        let mut form = StructureForm::from_model(&model);
        form.rows[1].b.text = form.rows[1].a.text.clone();
        form.refresh_preview();
        assert!(form.commit().unwrap_err().starts_with("Строка 2:"));
        assert_eq!(model, original);
        assert_eq!(form.preview().pairs.len(), model.pairs.len() - 1);
        assert!(!form.same_structure(&model));
    }
    #[test]
    fn keyboard_names_are_normalized_once_and_imported_symbols_keep_identity() {
        let mut form = StructureForm::new("NAMES");
        form.rows[0] = PairRow {
            pair: NameCell {
                text: "{".into(),
                imported: None,
            },
            kind: Some(b'R'),
            a: NameCell {
                text: "aB".into(),
                imported: None,
            },
            b: NameCell {
                text: "q".into(),
                imported: None,
            },
        };
        form.refresh_preview();
        let (_, state) = form.commit().unwrap();
        let DialogueState::Question { preview, .. } = state else {
            panic!("expected ground")
        };
        assert_eq!(preview.pairs[0].name, Name(*b"[ "));
        assert_eq!(preview.links[0].name, Name(*b"AB"));
        assert_eq!(preview.links[1].name, Name(*b"Q "));
        let mut imported = StructureForm::from_model(&preview);
        assert_eq!(imported.rows[0].pair.text, "[");
        assert!(imported.same_structure(&preview));
        imported.select_row(0);
        assert_eq!(
            selected_names(&imported),
            Some((Name(*b"[ "), [Name(*b"AB"), Name(*b"Q ")]))
        );
        let (_, state) = imported.commit().unwrap();
        let DialogueState::Question {
            preview: rewritten, ..
        } = state
        else {
            panic!("expected ground")
        };
        assert_eq!(rewritten.pairs[0].name, preview.pairs[0].name);
    }
    #[test]
    fn original_multi_link_joint_occurrences_are_retained_but_conflicts_are_explicit() {
        let mut model = Mechanism::default();
        let a = model.add_link(Name(*b"0 "));
        let b = model.add_link(Name(*b"1 "));
        let c = model.add_link(Name(*b"2 "));
        model
            .add_pair(PairKind::Revolute, Name(*b"A "), [a, b])
            .unwrap();
        model
            .add_pair(PairKind::Revolute, Name(*b"A "), [a, c])
            .unwrap();
        let mut form = StructureForm::from_model(&model);
        assert!(form.same_structure(&model));
        let (_, state) = form.commit().unwrap();
        let DialogueState::Question { preview, .. } = state else {
            panic!("expected ground")
        };
        assert_eq!(preview.pairs.len(), 2);
        assert_eq!(preview.pairs[0].name, preview.pairs[1].name);
        form.rows[1].kind = Some(b'P');
        assert!(form.commit().unwrap_err().contains("другой тип"));
        form.rows[1].kind = Some(b'R');
        form.rows[1].b = form.rows[0].b.clone();
        assert!(form.commit().unwrap_err().contains("повторяется"));
    }
    fn selected_names(form: &StructureForm) -> Option<(Name, [Name; 2])> {
        let (pair, links) = form.selected_preview_objects()?;
        Some((
            form.preview().pair(pair)?.name,
            [
                form.preview().link(links[0])?.name,
                form.preview().link(links[1])?.name,
            ],
        ))
    }
    #[test]
    fn selection_finds_the_connection_after_an_incomplete_row_is_omitted() {
        let model = fixture();
        let pair = &model.pairs[1];
        let expected = (
            pair.name,
            [
                model.link(pair.links[0]).unwrap().name,
                model.link(pair.links[1]).unwrap().name,
            ],
        );
        let mut form = StructureForm::from_model(&model);
        form.rows.insert(0, PairRow::default());
        form.refresh_preview();
        form.select_row(2);
        assert_eq!(selected_names(&form), Some(expected));
        assert_eq!(form.preview().pairs.len(), model.pairs.len());
        form.select_row(0);
        assert!(form.selected_preview_objects().is_none());
        form.rows[2].b = form.rows[2].a.clone();
        form.refresh_preview();
        form.select_row(2);
        assert!(form.selected_preview_objects().is_none());
    }
    #[test]
    fn selecting_a_shared_hinge_name_uses_both_links_to_find_its_occurrence() {
        let mut model = Mechanism::default();
        let a = model.add_link(Name(*b"0 "));
        let b = model.add_link(Name(*b"1 "));
        let c = model.add_link(Name(*b"2 "));
        model
            .add_pair(PairKind::Revolute, Name(*b"A "), [a, b])
            .unwrap();
        model
            .add_pair(PairKind::Revolute, Name(*b"A "), [a, c])
            .unwrap();
        let mut form = StructureForm::from_model(&model);
        form.select_row(1);
        assert_eq!(
            selected_names(&form),
            Some((Name(*b"A "), [Name(*b"0 "), Name(*b"2 ")]))
        );
        let (pair, links) = form.selected_preview_objects().unwrap();
        assert_eq!(form.preview().pair(pair).unwrap().links, links);
        assert_ne!(links, form.preview().pairs[0].links);
        form.select_row(0);
        assert_eq!(
            selected_names(&form),
            Some((Name(*b"A "), [Name(*b"0 "), Name(*b"1 ")]))
        );
    }
    #[test]
    fn deleting_a_preceding_row_keeps_the_selected_connection_and_deleting_it_clears_selection() {
        let mut form = StructureForm::from_model(&fixture());
        form.select_row(1);
        let expected = selected_names(&form);
        assert!(expected.is_some());
        form.remove_row(0);
        form.refresh_preview();
        assert_eq!(selected_names(&form), expected);
        form.remove_row(0);
        form.refresh_preview();
        assert!(form.selected_preview_objects().is_none());
        form.select_row(usize::MAX);
        assert!(form.selected_preview_objects().is_none());
    }

    fn paint_form_with_input(
        context: &egui::Context,
        form: &mut StructureForm,
        events: Vec<egui::Event>,
        modifiers: egui::Modifiers,
    ) -> egui::FullOutput {
        let mut input_events = vec![egui::Event::ModifiersChanged(modifiers)];
        input_events.extend(events);
        let raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(500.0, 560.0),
            )),
            events: input_events,
            ..Default::default()
        };
        let mut output = context.run_ui(raw, |ui| {
            assert!(form.ui(ui).is_none(), "Selecting a row cannot apply it.");
        });
        output.textures_delta.clear();
        output
    }

    fn painted_text_bounds(output: &egui::FullOutput, value: &str) -> egui::Rect {
        let matches = output
            .shapes
            .iter()
            .filter_map(|paint| match &paint.shape {
                egui::Shape::Text(text) if text.galley.text() == value => {
                    Some(egui::Rect::from_min_size(text.pos, text.galley.size()))
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(matches.len(), 1, "Expected one rendered {value:?}.");
        matches[0]
    }

    fn painted_widget_bounds(output: &egui::FullOutput, text: egui::Rect) -> egui::Rect {
        output
            .shapes
            .iter()
            .filter_map(|paint| match &paint.shape {
                egui::Shape::Rect(frame) if frame.rect.contains(text.center()) => Some(frame.rect),
                _ => None,
            })
            .min_by(|a, b| a.area().total_cmp(&b.area()))
            .expect("The rendered field has a visible frame.")
    }

    fn click_form(
        context: &egui::Context,
        form: &mut StructureForm,
        point: egui::Pos2,
    ) -> egui::FullOutput {
        for pressed in [true, false] {
            paint_form_with_input(
                context,
                form,
                vec![
                    egui::Event::PointerMoved(point),
                    egui::Event::PointerButton {
                        pos: point,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
                egui::Modifiers::NONE,
            );
        }
        paint_form_with_input(context, form, vec![], egui::Modifiers::NONE)
    }

    type RowValues = ([String; 3], Option<u8>);
    fn row_values(form: &StructureForm) -> Vec<RowValues> {
        form.rows
            .iter()
            .map(|row| {
                (
                    [
                        row.pair.text.clone(),
                        row.a.text.clone(),
                        row.b.text.clone(),
                    ],
                    row.kind,
                )
            })
            .collect()
    }

    #[test]
    fn actual_pointer_clicks_on_row_number_and_background_select_without_rewriting_answers() {
        let model = fixture();
        let mut form = StructureForm::from_model(&model);
        let original_rows = row_values(&form);
        let original_journal = form.commit().unwrap().0.current_journal().to_bytes();
        let context = egui::Context::default();
        paint_form_with_input(&context, &mut form, vec![], egui::Modifiers::NONE);
        let output = paint_form_with_input(&context, &mut form, vec![], egui::Modifiers::NONE);
        let row_number = painted_text_bounds(&output, "3.");
        let output = click_form(&context, &mut form, row_number.center());
        assert_eq!(
            selected_names(&form),
            Some((Name(*b"B "), [Name(*b"2 "), Name(*b"3 ")]))
        );
        // Use the actual gap between the rendered A field and its type box,
        // rather than a hard-coded row coordinate or the selection setter.
        let pair_text = painted_text_bounds(&output, "A");
        let pair_box = painted_widget_bounds(&output, pair_text);
        let kind_text = output
            .shapes
            .iter()
            .find_map(|paint| match &paint.shape {
                egui::Shape::Text(text)
                    if text.galley.text() == "R · шарнир"
                        && (text.pos.y - pair_text.top()).abs() < pair_text.height() =>
                {
                    Some(egui::Rect::from_min_size(text.pos, text.galley.size()))
                }
                _ => None,
            })
            .expect("A has a rendered R type cell.");
        let kind_box = painted_widget_bounds(&output, kind_text);
        assert!(kind_box.left() > pair_box.right());
        let gap = egui::Pos2::new(
            (pair_box.right() + kind_box.left()) * 0.5,
            pair_text.center().y,
        );
        assert!(!pair_box.contains(gap) && !kind_box.contains(gap));
        click_form(&context, &mut form, gap);
        assert_eq!(
            selected_names(&form),
            Some((Name(*b"A "), [Name(*b"1 "), Name(*b"2 ")]))
        );
        assert_eq!(row_values(&form), original_rows);
        assert!(form.same_structure(&model));
        assert_eq!(
            form.commit().unwrap().0.current_journal().to_bytes(),
            original_journal
        );
    }

    #[test]
    fn actual_field_focus_and_typing_update_only_the_selected_pair_and_its_preview_identity() {
        let model = fixture();
        let original_model = model.clone();
        let mut form = StructureForm::from_model(&model);
        let original_rows = row_values(&form);
        let original_journal = form.commit().unwrap().0.current_journal().to_bytes();
        let context = egui::Context::default();
        paint_form_with_input(&context, &mut form, vec![], egui::Modifiers::NONE);
        let output = paint_form_with_input(&context, &mut form, vec![], egui::Modifiers::NONE);
        let field_text = painted_text_bounds(&output, "B");
        click_form(&context, &mut form, field_text.center());
        assert_eq!(
            selected_names(&form),
            Some((Name(*b"B "), [Name(*b"2 "), Name(*b"3 ")]))
        );
        assert_eq!(row_values(&form), original_rows);
        assert_eq!(
            form.commit().unwrap().0.current_journal().to_bytes(),
            original_journal
        );
        let command = egui::Modifiers {
            ctrl: true,
            command: true,
            ..egui::Modifiers::NONE
        };
        paint_form_with_input(
            &context,
            &mut form,
            vec![egui::Event::Key {
                key: egui::Key::A,
                physical_key: Some(egui::Key::A),
                pressed: true,
                repeat: false,
                modifiers: command,
            }],
            command,
        );
        paint_form_with_input(
            &context,
            &mut form,
            vec![egui::Event::Text("q".into())],
            egui::Modifiers::NONE,
        );
        assert_eq!(form.rows[2].pair.text, "q");
        assert_eq!(
            selected_names(&form),
            Some((Name(*b"Q "), [Name(*b"2 "), Name(*b"3 ")]))
        );
        let edited_rows = row_values(&form);
        for (index, (before, after)) in original_rows.iter().zip(&edited_rows).enumerate() {
            if index == 2 {
                assert_eq!(&before.0[1..], &after.0[1..]);
                assert_eq!(before.1, after.1);
            } else {
                assert_eq!(before, after);
            }
        }
        let edited_journal = form.commit().unwrap().0.current_journal().to_bytes();
        let original_lines = diada_io::Journal::from_bytes(&original_journal);
        let edited_lines = diada_io::Journal::from_bytes(&edited_journal);
        assert_eq!(original_lines.lines().len(), edited_lines.lines().len());
        assert_eq!(
            original_lines
                .lines()
                .iter()
                .zip(edited_lines.lines())
                .filter(|(before, after)| before.bytes != after.bytes)
                .count(),
            1,
            "Typing in the pair name cannot rewrite other answers."
        );
        assert_eq!(model, original_model);
    }
}
