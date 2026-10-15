use crate::{
    analysis::{self, InputFields, SeriesKind, SeriesResult},
    drawing::{ObjectKey, Scene, View},
    fields,
};
use diada_core::*;
use diada_io::*;
use eframe::egui::{self, Color32, RichText};
use std::{
    path::PathBuf,
    sync::mpsc,
    time::{Duration, Instant},
};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Mechanism,
    Plans,
    Table,
    Graphs,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Action {
    New,
    Open,
    Close,
}
enum RunAction {
    Calculate,
    Standard,
    Motion,
    Graphs,
    Export,
    Selection,
}
#[derive(Clone)]
struct Edit {
    path: FieldPath,
    question: ReplayQuestion,
    old: String,
}

pub struct Workbench {
    document: Option<EditableDialogue>,
    draft: Option<DialogueController>,
    structure: Option<crate::structure_form::StructureForm>,
    structure_was_dirty: bool,
    state: Option<DialogueState>,
    path: Option<PathBuf>,
    source: InputSource,
    dirty: bool,
    pending: Option<Action>,
    edit: Option<Edit>,
    answer: String,
    /// Display text and original spelling for an untouched numeric editor.
    answer_source: Option<(String, String)>,
    hover_answer: Option<&'static str>,
    focus: bool,
    undo: Vec<EditableDialogue>,
    redo: Vec<EditableDialogue>,
    draft_undo: Vec<DialogueController>,
    error: Option<String>,
    notice: String,
    tab: Tab,
    view: View,
    fields: Vec<InputFields>,
    positions: usize,
    forces: bool,
    series: Option<SeriesResult>,
    receiver: Option<mpsc::Receiver<SeriesResult>>,
    current: usize,
    playing: bool,
    fps: f32,
    tick: Instant,
    oscillation: bool,
    motion_start: String,
    motion_end: String,
    show_m: bool,
    show_v: bool,
    show_a: bool,
    trajectories: bool,
    v_view: View,
    a_view: View,
    selection: ReportSelection,
    selection_open: bool,
    help: bool,
    section: &'static str,
    graph: crate::plots::GraphChoice,
    capture: Option<crate::capture::Capture>,
    draft_was_dirty: bool,
    restore_answers: Vec<(FieldPath, String)>,
    motion_trace: crate::trajectory_trace::MotionTrace,
}
impl Workbench {
    pub fn new(context: &eframe::CreationContext<'_>) -> Self {
        context.egui_ctx.set_theme(egui::Theme::Light);
        let mut style = (*context.egui_ctx.style_of(egui::Theme::Light)).clone();
        style.spacing.item_spacing = egui::vec2(8.0, 7.0);
        for text in [egui::TextStyle::Body, egui::TextStyle::Button] {
            if let Some(font) = style.text_styles.get_mut(&text) {
                font.size = 15.0;
            }
        }
        style.visuals.selection.bg_fill = Color32::from_rgb(0, 119, 127);
        context.egui_ctx.set_style_of(egui::Theme::Light, style);
        let mut app = Self::blank();
        if let Some(path) = std::env::args_os()
            .nth(1)
            .filter(|p| !p.to_string_lossy().starts_with('-'))
        {
            app.load_path(PathBuf::from(path));
        }
        if let Some(capture) = &app.capture {
            app.tab = match capture.view.as_str() {
                "plans" | "motion-plans" | "velocity" | "acceleration" => Tab::Plans,
                "table" => Tab::Table,
                "graphs" => Tab::Graphs,
                _ => Tab::Mechanism,
            };
            app.show_v = app.tab == Tab::Plans;
            app.show_a = app.tab == Tab::Plans;
            if matches!(capture.view.as_str(), "velocity" | "acceleration") {
                app.show_m = false;
                app.show_v = capture.view == "velocity";
                app.show_a = capture.view == "acceleration";
            }
            if capture.view == "structure" {
                app.section = "Структура";
            }
        }
        if app
            .capture
            .as_ref()
            .is_some_and(|c| c.view == "connections")
        {
            if app.model().is_some() {
                app.open_structure();
            } else {
                app.perform(Action::New);
            }
            if let Some(row) = app
                .capture
                .as_ref()
                .and_then(|capture| capture.connection_row)
                && let Some(form) = &mut app.structure
            {
                form.select_row(row);
            }
        } else if app.capture.is_some() && app.document.is_some() {
            if app
                .capture
                .as_ref()
                .is_some_and(|c| c.view.starts_with("motion"))
            {
                app.motion();
            } else if app.tab == Tab::Graphs {
                app.graph_series();
            } else {
                app.calculate();
            }
        }
        app
    }
    fn blank() -> Self {
        Self {
            document: None,
            draft: None,
            structure: None,
            structure_was_dirty: false,
            state: None,
            path: None,
            source: InputSource::Keyboard,
            dirty: false,
            pending: None,
            edit: None,
            answer: String::new(),
            answer_source: None,
            hover_answer: None,
            focus: false,
            undo: Vec::new(),
            redo: Vec::new(),
            draft_undo: Vec::new(),
            error: None,
            notice: "Создайте механизм или откройте файл ДиАДА.".into(),
            tab: Tab::Mechanism,
            view: Default::default(),
            fields: Vec::new(),
            positions: 12,
            forces: false,
            series: None,
            receiver: None,
            current: 0,
            playing: false,
            fps: 12.0,
            tick: Instant::now(),
            oscillation: false,
            motion_start: "0".into(),
            motion_end: "90".into(),
            show_m: true,
            show_v: false,
            show_a: false,
            trajectories: false,
            v_view: Default::default(),
            a_view: Default::default(),
            selection: Default::default(),
            selection_open: false,
            help: false,
            section: "Размеры",
            graph: Default::default(),
            capture: crate::capture::Capture::from_args(),
            draft_was_dirty: false,
            restore_answers: Vec::new(),
            motion_trace: Default::default(),
        }
    }
    pub fn load_path(&mut self, path: PathBuf) {
        let title = path
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        let result = std::fs::read(&path)
            .map_err(|e| e.to_string())
            .and_then(|bytes| {
                let mut draft = DialogueController::resume(Journal::from_bytes(&bytes), title);
                let state = draft
                    .next(&TpNumbers)
                    .map_err(|e| format!("Не удалось прочитать файл: {e}"))?;
                Ok((draft, state))
            });
        match result {
            Ok((draft, state)) => {
                self.reset();
                self.path = Some(path);
                self.source = InputSource::File;
                self.draft = Some(draft);
                self.accept(state);
                self.dirty = false;
            }
            Err(e) => self.error = Some(e),
        }
    }
    fn reset(&mut self) {
        self.hover_answer = None;
        self.document = None;
        self.draft = None;
        self.structure = None;
        self.state = None;
        self.path = None;
        self.edit = None;
        self.undo.clear();
        self.redo.clear();
        self.draft_undo.clear();
        self.restore_answers.clear();
        self.series = None;
        self.receiver = None;
        self.current = 0;
        self.motion_trace.clear();
        self.trajectories = false;
        self.playing = false;
        self.error = None;
        self.answer.clear();
        self.answer_source = None;
        self.fields.clear();
        self.view = Default::default();
        self.v_view = Default::default();
        self.a_view = Default::default();
        self.selection = Default::default();
        self.dirty = false;
    }
    fn request(&mut self, action: Action) {
        if self.dirty {
            self.pending = Some(action)
        } else {
            self.perform(action)
        }
    }
    fn perform(&mut self, action: Action) {
        self.pending = None;
        match action {
            Action::New => {
                self.reset();
                self.source = InputSource::Keyboard;
                self.structure = Some(crate::structure_form::StructureForm::new("UNTITLED"));
                self.structure_was_dirty = false;
                self.section = "Структура";
                self.dirty = true;
                self.notice = "Введите все пары и соединяемые звенья в одной таблице.".into();
            }
            Action::Open => {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("Механизм ДиАДА", &["di", "DI"])
                    .pick_file()
                {
                    self.load_path(path);
                }
            }
            Action::Close => {}
        }
    }
    fn accept(&mut self, state: DialogueState) {
        self.hover_answer = None;
        self.answer.clear();
        self.answer_source = None;
        self.focus = true;
        self.error = None;
        match &state {
            DialogueState::Complete(loaded) => {
                self.redo.clear();
                if let Some(old) = self.document.take() {
                    self.undo.push(old);
                }
                self.fields = InputFields::for_model(&loaded.mechanism);
                self.forces = loaded.mechanism.force_analysis;
                self.document = Some(EditableDialogue::from_loaded(loaded.clone()));
                self.draft = None;
                self.state = None;
                self.edit = None;
                self.draft_undo.clear();
                self.restore_answers.clear();
                self.invalidate();
                self.view.fixed_bounds = None;
                self.notice = "Механизм готов. Выберите положение или серию для расчёта.".into();
                return;
            }
            DialogueState::Question { question, .. } => {
                self.section = fields::section(&question.path);
                if let Some(AnswerValue::Number(v)) = question.current {
                    self.set_answer(crate::ui_numbers::real(v), true);
                }
                if let Some((_, raw)) = self
                    .restore_answers
                    .iter()
                    .find(|(p, _)| p == &question.path)
                {
                    let raw = raw.clone();
                    self.set_answer(raw, question.kind == QuestionKind::Number);
                }
            }
            _ => {}
        }
        self.state = Some(state);
        if matches!(&self.state, Some(DialogueState::Question { question, .. }) if matches!(question.path, FieldPath::PairCommand(_)))
        {
            self.open_structure();
        }
    }
    fn submit(&mut self, text: &str) {
        let original = self.raw_answer(text).to_owned();
        let text = original.as_str();
        self.hover_answer = None;
        if let Some(edit) = self.edit.clone() {
            if !fields::structural(&edit.path) && text == edit.old {
                self.edit = None;
                self.answer.clear();
                self.answer_source = None;
                self.error = None;
                self.notice = "Параметр не изменён.".into();
                return;
            }
            if fields::structural(&edit.path) {
                if let Some(document) = &self.document {
                    match document.begin_edit(&edit.path) {
                        Ok(mut draft) => match draft.next(&TpNumbers).and_then(|_| {
                            let before = draft.clone();
                            let state = draft.answer(text, &TpNumbers)?;
                            let reason = Self::rejected_dyad_reason(&before, text, &state);
                            Ok((state, reason))
                        }) {
                            Ok((state, reason)) => {
                                self.draft_was_dirty = self.dirty;
                                self.draft = Some(draft);
                                self.edit = None;
                                self.restore_answers.clear();
                                self.invalidate();
                                self.accept(state);
                                self.error = reason;
                                self.dirty = true;
                            }
                            Err(e) => self.error = Some(format!("Ответ не принят: {e}")),
                        },
                        Err(e) => self.error = Some(e.to_string()),
                    }
                }
            } else if let Some(document) = self.document.as_mut() {
                let old = document.clone();
                match document.edit_text(&edit.path, text, &TpNumbers) {
                    Ok(_) => {
                        self.undo.push(old);
                        self.redo.clear();
                        self.edit = None;
                        self.dirty = true;
                        self.invalidate();
                        self.error = None;
                        self.notice = "Изменение принято. Пересчитайте положение.".into();
                    }
                    Err(e) => {
                        let optional = matches!(
                            edit.path,
                            FieldPath::LinkOnGuide(_)
                                | FieldPath::LinkParameter(_)
                                | FieldPath::PointOnGuide(_)
                                | FieldPath::PointDistance(_)
                                | FieldPath::ForceMagnitude(_)
                        );
                        if matches!(e, ReplayError::DependentQuestionsChanged(_))
                            || optional && !matches!(e, ReplayError::Numeric { .. })
                            || matches!(edit.path, FieldPath::ForceMagnitude(_))
                                && analysis::parse(text).is_ok()
                        {
                            self.begin_change(&edit.path, text);
                        } else {
                            self.error = Some(format!("Ответ не принят: {e}"));
                        }
                    }
                }
            }
        } else if let Some(draft) = self.draft.as_mut() {
            let old = draft.clone();
            let previous_question = draft.question().cloned();
            match draft.answer(text, &TpNumbers) {
                Ok(state) => {
                    let repeated = matches!((&previous_question,&state),(Some(old),DialogueState::Question{question,..})if old.path==question.path);
                    let dyad_reason = Self::rejected_dyad_reason(&old, text, &state);
                    self.draft_undo.push(old);
                    self.accept(state);
                    self.dirty = true;
                    if let Some(reason) = dyad_reason {
                        self.error = Some(reason);
                    } else if repeated {
                        self.error=Some("Ответ не подходит для этого шага. Проверьте имя или допустимость значения; уточнение приведено в вопросе.".into());
                    }
                }
                Err(e) => self.error = Some(self.dialogue_error(&e)),
            }
        }
    }
    fn rejected_dyad_reason(
        before: &DialogueController,
        text: &str,
        after: &DialogueState,
    ) -> Option<String> {
        let previous = before.question()?;
        let FieldPath::GroupLink(index, 1) = previous.path else {
            return None;
        };
        let DialogueState::Question { question, .. } = after else {
            return None;
        };
        if question.path != FieldPath::GroupLink(index, 0) {
            return None;
        }
        let first = Name(previous.objects.first()?.0);
        let second = Name(crate::cli::legacy_name(text).ok()?.0);
        crate::dyad_feedback::rejection(before.preview()?, first, second).or_else(|| {
            Some("Эта пара звеньев не принята как очередная диада. Проверьте их соединение и порядок анализа.".into())
        })
    }
    fn dialogue_error(&self, error: &ReplayError) -> String {
        if matches!(
            error,
            ReplayError::Core(CoreError::InvalidStructure(
                "formal mobility must be positive"
            ))
        ) && let Some(model) = self.model()
        {
            if model.formal_mobility() <= 0 {
                return fields::mobility_error(model);
            }
            return format!(
                "{} Диалог вернул ошибку проверки структуры. Вернитесь в таблицу связей и проверьте данные.",
                fields::mobility_summary(model)
            );
        }
        format!("Ответ не принят: {error}")
    }
    fn begin_change(&mut self, path: &FieldPath, text: &str) {
        let Some(document) = &self.document else {
            return;
        };
        let restore = document
            .bindings()
            .iter()
            .rposition(|b| &b.path == path)
            .map(|index| {
                document.bindings()[index + 1..]
                    .iter()
                    .map(|b| (b.path.clone(), b.answer_text(document.current_journal())))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let result = document.begin_edit(path).and_then(|mut draft| {
            draft.next(&TpNumbers)?;
            let state = draft.answer(text, &TpNumbers)?;
            Ok((draft, state))
        });
        match result {
            Ok((draft, state)) => {
                self.restore_answers = restore;
                self.draft_was_dirty = self.dirty;
                self.draft = Some(draft);
                self.edit = None;
                self.invalidate();
                self.accept(state);
                self.dirty = true;
                self.notice="Изменился состав вопросов. Уточните зависимые параметры; прежний механизм сохранён до завершения.".into();
            }
            Err(error) => self.error = Some(format!("Ответ не принят: {error}")),
        }
    }
    fn restore_remaining(&mut self) {
        // A changed optional question can introduce a genuinely new field.
        // Never shift the old numeric journal into that new field.
        while let Some(DialogueState::Question { question, .. }) = &self.state {
            let path = question.path.clone();
            let Some(index) = self.restore_answers.iter().position(|(p, _)| p == &path) else {
                self.notice = "Для этого вопроса прежнего ответа нет. Введите его, затем можно принять остальные.".into();
                break;
            };
            let text = self.restore_answers[index].1.clone();
            self.restore_answers.drain(..=index);
            self.submit(&text);
            if self.error.is_some() {
                break;
            }
        }
    }
    fn cancel_draft(&mut self) {
        if self.document.is_some() {
            self.draft = None;
            self.state = None;
            self.edit = None;
            self.draft_undo.clear();
            self.restore_answers.clear();
            self.dirty = self.draft_was_dirty;
            self.error = None;
            self.notice = "Незавершённое изменение отменено.".into();
        }
    }
    fn invalidate(&mut self) {
        self.series = None;
        self.receiver = None;
        self.current = 0;
        self.motion_trace.clear();
        self.playing = false;
        self.view.fixed_bounds = None;
        self.view.fit_bounds = None;
        self.v_view.fixed_bounds = None;
        self.v_view.fit_bounds = None;
        self.a_view.fixed_bounds = None;
        self.a_view.fit_bounds = None;
    }
    fn open_structure(&mut self) {
        let Some(model) = self.model().cloned() else {
            return;
        };
        self.structure_was_dirty = self.dirty;
        self.structure = Some(crate::structure_form::StructureForm::from_model(&model));
        self.edit = None;
        self.invalidate();
        self.section = "Структура";
        self.dirty = true;
        self.error = None;
        self.notice = "Измените связи в таблице; прежний механизм сохранён до завершения.".into();
    }
    fn cancel_structure(&mut self) {
        let returned_to_pairs = matches!(&self.state, Some(DialogueState::Question { question, .. }) if matches!(question.path, FieldPath::PairCommand(_)));
        self.structure = None;
        self.dirty = self.structure_was_dirty;
        self.error = None;
        if returned_to_pairs {
            if !self.draft_undo.is_empty() {
                self.undo();
            } else if self.document.is_some() {
                self.cancel_draft();
            } else {
                self.reset();
            }
        }
        if self.document.is_none() && self.draft.is_none() {
            self.reset();
        }
        self.notice = "Табличное изменение структуры отменено.".into();
    }
    fn apply_structure(&mut self) {
        let Some(form) = &self.structure else {
            return;
        };
        let base = match &self.state {
            Some(
                DialogueState::Question { preview, .. }
                | DialogueState::ReviewRequired { preview, .. },
            ) => Some(preview),
            Some(DialogueState::Complete(loaded)) => Some(&loaded.mechanism),
            None => self.document.as_ref().map(|d| &d.loaded().mechanism),
        };
        if base.is_some_and(|m| form.same_structure(m)) {
            if matches!(&self.state, Some(DialogueState::Question { question, .. }) if matches!(question.path, FieldPath::PairCommand(_)))
            {
                if let Err(error) = form.commit() {
                    self.structure.as_mut().unwrap().set_error(error.clone());
                    self.error = Some(error);
                    return;
                }
                let result = self.draft.clone().ok_or_else(|| "Черновик структуры недоступен.".to_string()).and_then(|mut draft| {
                    let state = draft.answer(" ", &TpNumbers).map_err(|error| error.to_string())?;
                    if !matches!(&state, DialogueState::Question { question, .. } if question.path == FieldPath::ConfirmPairs) {
                        return Err("Диалог не подтвердил завершение списка пар.".into());
                    }
                    let state = draft.answer("Y", &TpNumbers).map_err(|error| error.to_string())?;
                    Ok((draft, state))
                });
                match result {
                    Ok((draft, state)) => {
                        self.structure = None;
                        self.draft = Some(draft);
                        self.accept(state);
                        self.notice = "Связи подтверждены. Укажите стойку и ведущие звенья.".into();
                    }
                    Err(error) => {
                        self.structure.as_mut().unwrap().set_error(error.clone());
                        self.error = Some(error);
                    }
                }
                return;
            }
            self.cancel_structure();
            self.notice = "Связи не изменились; исходные данные сохранены.".into();
            return;
        }
        match form.commit() {
            Ok((draft, state)) => {
                if self.draft.is_none() {
                    self.draft_was_dirty = self.structure_was_dirty;
                }
                self.structure = None;
                self.draft = Some(draft);
                self.draft_undo.clear();
                self.restore_answers.clear();
                self.invalidate();
                self.accept(state);
                self.dirty = true;
                self.notice = "Все связи приняты. Теперь укажите стойку и ведущие звенья.".into();
            }
            Err(error) => {
                if let Some(form) = &mut self.structure {
                    form.set_error(error.clone());
                }
                self.error = Some(error);
            }
        }
    }
    fn start_edit(&mut self, path: FieldPath) {
        if matches!(
            path,
            FieldPath::PairCommand(_)
                | FieldPath::PairLink(_, _)
                | FieldPath::PairName(_)
                | FieldPath::RenamePairConfirm(_)
                | FieldPath::ConfirmPairs
        ) {
            self.open_structure();
            return;
        }
        if let Some(draft) = self.draft.as_mut() {
            let previous = draft.clone();
            let raw = draft
                .bindings()
                .iter()
                .rev()
                .find(|b| b.path == path)
                .map(|b| {
                    (
                        b.answer_text(draft.journal()),
                        matches!(&b.value, AnswerValue::Number(_)),
                    )
                });
            match draft.rewind(&path, &TpNumbers) {
                Ok(state) => {
                    self.draft_undo.push(previous);
                    self.accept(state);
                    if let Some((raw, numeric)) = raw {
                        self.set_answer(raw, numeric);
                    }
                }
                Err(e) => self.error = Some(e.to_string()),
            }
            return;
        }
        if let Some(document) = &self.document {
            if let Some(binding) = document.bindings().iter().rev().find(|b| b.path == path) {
                let old = fields::raw(document, binding);
                match document
                    .begin_edit(&path)
                    .and_then(|mut draft| draft.next(&TpNumbers))
                {
                    Ok(DialogueState::Question { question, .. }) => {
                        self.view.selected =
                            fields::describe(&path, &document.loaded().mechanism).2;
                        self.set_answer(old.clone(), question.kind == QuestionKind::Number);
                        self.focus = true;
                        self.edit = Some(Edit {
                            path,
                            question,
                            old,
                        });
                        self.error = None;
                    }
                    Err(e) => self.error = Some(e.to_string()),
                    _ => {}
                }
            }
        } else if let (Some(draft), Some(DialogueState::ReviewRequired { bindings, .. })) =
            (&self.draft, &self.state)
            && let Some(binding) = bindings.iter().rev().find(|b| b.path == path)
        {
            let mut bytes = Vec::new();
            for line in &draft.journal().lines()[..binding.start_line - 1] {
                bytes.extend_from_slice(&line.bytes);
                bytes.extend_from_slice(match line.ending {
                    LineEnding::Crlf => b"\r\n",
                    LineEnding::Cr => b"\r",
                    LineEnding::Lf => b"\n",
                    LineEnding::None => b"",
                });
            }
            let prefix = Journal::from_bytes(&bytes);
            let title = self.model().map(|m| m.title.clone()).unwrap_or_default();
            let mut next = DialogueController::resume(prefix, title);
            match next.next(&TpNumbers) {
                Ok(state) => {
                    self.draft = Some(next);
                    self.accept(state);
                }
                Err(e) => self.error = Some(e.to_string()),
            }
        }
    }
    fn clear_selection(&mut self) {
        self.view.selected = None;
        self.v_view.selected = None;
        self.a_view.selected = None;
        if self.edit.take().is_some() {
            self.answer.clear();
            self.answer_source = None;
            self.hover_answer = None;
            self.focus = false;
            self.error = None;
            self.view.highlighted.clear();
        }
    }
    fn canvas_interaction(&mut self, interaction: crate::drawing::SceneInteraction) {
        if interaction.clicked {
            if let Some(key) = interaction.object {
                self.view.selected = Some(key);
            } else {
                self.clear_selection();
            }
        }
    }
    fn model(&self) -> Option<&Mechanism> {
        if let Some(form) = &self.structure {
            return Some(form.preview());
        }
        if let Some(state) = &self.state {
            return Some(match state {
                DialogueState::Question { preview, .. }
                | DialogueState::ReviewRequired { preview, .. } => preview,
                DialogueState::Complete(loaded) => &loaded.mechanism,
            });
        }
        self.document.as_ref().map(|d| &d.loaded().mechanism)
    }
    fn set_answer(&mut self, raw: String, numeric: bool) {
        self.answer = if numeric {
            crate::ui_numbers::compact(&raw)
        } else {
            raw.clone()
        };
        self.answer_source = numeric.then(|| (self.answer.clone(), raw));
    }
    fn raw_answer<'a>(&'a self, text: &'a str) -> &'a str {
        if let Some((display, raw)) = &self.answer_source
            && text == display
        {
            raw
        } else {
            text
        }
    }
    fn preview_answer(&self) -> &str {
        self.raw_answer(self.hover_answer.unwrap_or(&self.answer))
    }
    fn draft_projection(&self) -> Option<(crate::live_preview::LivePreview, Option<String>)> {
        let draft = self.draft.as_ref()?;
        let mut candidate = draft.clone();
        let mut next_hint = None;
        if let Some(DialogueState::Question { question, .. }) = &self.state {
            let answer = self.preview_answer();
            let previewable = question.kind == QuestionKind::YesNo && self.hover_answer.is_some()
                || question.kind == QuestionKind::Number && analysis::parse(answer).is_ok();
            if previewable {
                let mut trial = candidate.clone();
                if let Ok(next) = trial.answer(answer, &TpNumbers)
                    && !matches!(&next, DialogueState::Question {question: pending, ..} if pending.path == question.path)
                {
                    if self.hover_answer.is_some() {
                        next_hint = Some(match &next {
                            DialogueState::Question { question, preview } => format!(
                                "После ответа «{}»: {}",
                                if answer == "Y" { "Да" } else { "Нет" },
                                fields::question_text(question, Some(preview)),
                            ),
                            DialogueState::ReviewRequired { .. } => {
                                "После ответа: проверка введённых данных.".into()
                            }
                            DialogueState::Complete(_) => {
                                "После ответа: описание механизма будет завершено.".into()
                            }
                        });
                    }
                    candidate = trial;
                }
            }
        }
        Some((
            crate::live_preview::build(candidate.preview()?, candidate.bindings(), &self.fields),
            next_hint,
        ))
    }
    fn save(&mut self, save_as: bool) -> bool {
        if self.draft.is_some() || self.structure.is_some() {
            self.error = Some("Завершите изменение структуры перед сохранением.".into());
            return false;
        }
        let Some(document) = &self.document else {
            self.error = Some("Завершите описание механизма перед сохранением.".into());
            return false;
        };
        let path = (if save_as { None } else { self.path.clone() }).or_else(|| {
            rfd::FileDialog::new()
                .add_filter("Механизм ДиАДА", &["DI"])
                .set_file_name(
                    self.path
                        .as_ref()
                        .and_then(|p| p.file_name())
                        .map(|s| s.to_string_lossy().into_owned())
                        .unwrap_or_else(|| "MECHANISM.DI".into()),
                )
                .save_file()
        });
        let Some(path) = path else {
            return false;
        };
        match document.save(&path) {
            Ok(_) => {
                self.path = Some(path);
                self.dirty = false;
                self.notice = "Сохранены текущие данные механизма в формате .DI.".into();
                true
            }
            Err(e) => {
                self.error = Some(format!("Не удалось сохранить механизм: {e}"));
                false
            }
        }
    }
    fn undo(&mut self) {
        if self.structure.is_some() {
            return;
        }
        if self.draft.is_some() {
            if let Some(mut previous) = self.draft_undo.pop() {
                match previous.next(&TpNumbers) {
                    Ok(state) => {
                        self.draft = Some(previous);
                        self.accept(state);
                    }
                    Err(e) => self.error = Some(e.to_string()),
                }
            }
        } else if let Some(previous) = self.undo.pop() {
            if let Some(current) = self.document.replace(previous) {
                self.redo.push(current);
            }
            self.invalidate();
            self.dirty = true;
            self.edit = None;
            self.sync_inputs();
        }
    }
    fn redo(&mut self) {
        if self.draft.is_some() || self.structure.is_some() {
            return;
        }
        if let Some(next) = self.redo.pop() {
            if let Some(current) = self.document.replace(next) {
                self.undo.push(current);
            }
            self.invalidate();
            self.dirty = true;
            self.edit = None;
            self.sync_inputs();
        }
    }
    fn sync_inputs(&mut self) {
        if let Some(document) = &self.document {
            let mut next = InputFields::for_model(&document.loaded().mechanism);
            for field in &mut next {
                if let Some(old) = self
                    .fields
                    .iter()
                    .find(|old| old.name == field.name && old.rotational == field.rotational)
                {
                    let id = field.link;
                    *field = old.clone();
                    field.link = id;
                }
            }
            self.fields = next;
            self.forces = document.loaded().mechanism.force_analysis;
            self.playing = false;
        }
    }
    fn begin_series(&mut self) {
        let bounds = [
            self.view.fixed_bounds,
            self.v_view.fixed_bounds,
            self.a_view.fixed_bounds,
        ];
        self.invalidate();
        // A new sampling grid must not move the same mechanism on screen.
        self.view.fixed_bounds = bounds[0];
        self.v_view.fixed_bounds = bounds[1];
        self.a_view.fixed_bounds = bounds[2];
    }
    fn launch(&mut self, config: AnalysisConfig, kind: SeriesKind) {
        let Some(model) = self.document.as_ref().map(|d| d.loaded().mechanism.clone()) else {
            return;
        };
        let (sender, receiver) = mpsc::channel();
        self.begin_series();
        self.receiver = Some(receiver);
        self.playing = false;
        self.error = None;
        self.notice = "Выполняется расчёт…".into();
        std::thread::spawn(move || {
            let result = analysis::calculate(model, config, kind);
            let _ = sender.send(result);
        });
    }
    fn calculate(&mut self) {
        match analysis::config(&self.fields, self.positions, self.forces) {
            Ok(config) => self.launch(config, SeriesKind::Analysis),
            Err(e) => self.error = Some(e),
        }
    }
    fn motion(&mut self) {
        let Some(model) = self.document.as_ref().map(|d| d.loaded().mechanism.clone()) else {
            return;
        };
        let rate = self
            .fields
            .first()
            .ok_or("Не задано ведущее звено.")
            .and_then(|f| {
                analysis::parse(&f.rate)
                    .map_err(|_| "Не удалось прочитать скорость ведущего звена.")
            });
        let range = if self.oscillation {
            match (
                analysis::parse(&self.motion_start),
                analysis::parse(&self.motion_end),
            ) {
                (Ok(a), Ok(b)) => Some([a, b]),
                _ => {
                    self.error = Some("Не удалось прочитать границы качания.".into());
                    return;
                }
            }
        } else {
            None
        };
        let configs = rate.map_err(str::to_owned).and_then(|rate| {
            model
                .original_motion_configs(rate, range, AnalysisMode::Kinematics)
                .map_err(|e| analysis::error(&e))
        });
        match configs {
            Ok(configs) => {
                let (sender, receiver) = mpsc::channel();
                self.begin_series();
                self.receiver = Some(receiver);
                self.playing = false;
                self.trajectories = !crate::preview::trajectory_points(&model).is_empty();
                self.error = None;
                self.notice = "Подготавливается движение…".into();
                std::thread::spawn(move || {
                    let _ = sender.send(analysis::calculate_passes(model, configs));
                });
            }
            Err(error) => self.error = Some(error),
        }
    }
    fn graph_series(&mut self) {
        let Some(model) = self.model() else {
            return;
        };
        match model.original_graph_config(if self.forces {
            AnalysisMode::Kinetostatics
        } else {
            AnalysisMode::Kinematics
        }) {
            Ok(config) => self.launch(config, SeriesKind::Graphs),
            Err(e) => self.error = Some(analysis::error(&e)),
        }
    }
    fn export_report(&mut self) {
        let Some(series) = &self.series else {
            return;
        };
        let frames = series.frames.iter().cloned().collect::<Result<Vec<_>, _>>();
        let bytes = frames.map_err(|e| analysis::error(&e)).and_then(|frames| {
            analysis::report(
                self.path
                    .as_ref()
                    .and_then(|p| p.file_stem())
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "UNTITLED".into()),
                self.source,
                series.clock,
                &frames,
                &self.selection,
                series.config.mode == AnalysisMode::Kinetostatics,
            )
        });
        match bytes {
            Ok(bytes) => {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("Отчёт ДиАДА", &["DIF"])
                    .set_file_name("REPORT.DIF")
                    .save_file()
                {
                    match std::fs::write(&path, bytes) {
                        Ok(()) => self.notice = format!("Отчёт сохранён: {}", path.display()),
                        Err(e) => self.error = Some(e.to_string()),
                    }
                }
            }
            Err(e) => self.error = Some(format!("Отчёт не сформирован: {e}")),
        }
    }
    fn receive(&mut self) {
        if let Some(receiver) = &self.receiver {
            match receiver.try_recv() {
                Ok(series) => {
                    self.notice = format!(
                        "Положений: {}. {}",
                        series.frames.len(),
                        if series.kind.is_analogs() {
                            "Аналоги: |скорость| = 1, ускорение = 0."
                        } else {
                            "Заданные входные параметры."
                        }
                    );
                    self.current = self
                        .capture
                        .as_ref()
                        .map(|c| c.frame.min(series.frames.len().saturating_sub(1)))
                        .unwrap_or(0);
                    self.motion_trace.clear();
                    if let Some(text) = self.capture.as_ref().and_then(|c| c.highlight.as_deref())
                        && let Some(model) =
                            series.frames.iter().flatten().next().map(|f| &f.mechanism)
                    {
                        match crate::cli::legacy_name(text).and_then(|name| {
                            model
                                .link_by_name(Name(name.0))
                                .map(|id| ObjectKey::Link(id.0))
                                .ok_or_else(|| format!("Нет звена {text}"))
                        }) {
                            Ok(key) => self.view.selected = Some(key),
                            Err(error) => self.error = Some(error),
                        }
                    }
                    if let Some(graph) = self.capture.as_ref().and_then(|c| c.graph.as_ref())
                        && let Some(model) =
                            series.frames.iter().flatten().next().map(|f| &f.mechanism)
                        && let Err(error) = self.graph.configure(
                            model,
                            series.config.mode,
                            &graph.category,
                            graph.object.as_deref(),
                            graph.function.as_deref(),
                        )
                    {
                        self.error = Some(error);
                    }
                    let mut bounds = egui::Rect::NOTHING;
                    for (index, frame) in series.frames.iter().enumerate() {
                        let scene = match frame {
                            Ok(frame) => {
                                if series.kind == SeriesKind::Motion {
                                    Scene::motion(&frame.mechanism)
                                } else {
                                    Scene::mechanism(&frame.mechanism)
                                }
                            }
                            Err(error) => {
                                let Some(model) =
                                    series.failed_previews.get(index).and_then(|p| p.as_ref())
                                else {
                                    continue;
                                };
                                crate::failure::preview(
                                    model,
                                    error,
                                    series.kinematics_valid[index],
                                    series.kind == SeriesKind::Motion,
                                )
                            }
                        };
                        let b = scene.bounds();
                        bounds.extend_with(b.min);
                        bounds.extend_with(b.max);
                    }
                    // Every series can be played or stepped through. Keep one
                    // world-to-screen transform so fixed supports stay fixed.
                    self.view.fit_bounds = bounds.is_finite().then_some(bounds);
                    if self.view.fixed_bounds.is_none() {
                        self.view.fixed_bounds = self.view.fit_bounds;
                    }
                    for (acceleration, view) in
                        [(false, &mut self.v_view), (true, &mut self.a_view)]
                    {
                        let mut bounds = egui::Rect::NOTHING;
                        for (index, frame) in series.frames.iter().enumerate() {
                            let model = frame.as_ref().ok().map(|f| &f.mechanism).or_else(|| {
                                series.kinematics_valid[index]
                                    .then(|| series.failed_previews[index].as_ref())
                                    .flatten()
                            });
                            if let Some(model) = model
                                && let Ok(scene) = crate::preview::plan(model, acceleration)
                            {
                                let b = scene.bounds();
                                bounds.extend_with(b.min);
                                bounds.extend_with(b.max);
                            }
                        }
                        view.fit_bounds = bounds.is_finite().then_some(bounds);
                        if view.fixed_bounds.is_none() {
                            view.fixed_bounds = view.fit_bounds;
                        }
                    }
                    self.series = Some(series);
                    self.receiver = None;
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.receiver = None;
                    self.error = Some("Расчёт прерван.".into());
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
    }
    fn toolbar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            ui.label(
                RichText::new("ДиАДА")
                    .size(23.0)
                    .strong()
                    .color(Color32::from_rgb(0, 119, 127)),
            );
            ui.separator();
            if ui.button("Создать").clicked() {
                self.request(Action::New);
            }
            if ui.button("Открыть…").clicked() {
                self.request(Action::Open);
            }
            if ui
                .add_enabled(
                    self.document.is_some() && self.draft.is_none() && self.structure.is_none(),
                    egui::Button::new("Сохранить"),
                )
                .clicked()
            {
                self.save(false);
            }
            if ui
                .add_enabled(
                    self.document.is_some() && self.draft.is_none() && self.structure.is_none(),
                    egui::Button::new("Сохранить как…"),
                )
                .clicked()
            {
                self.save(true);
            }
            if ui
                .add_enabled(
                    self.structure.is_none()
                        && (!self.undo.is_empty() || !self.draft_undo.is_empty()),
                    egui::Button::new("Отменить"),
                )
                .clicked()
            {
                self.undo();
            }
            if ui
                .add_enabled(
                    self.draft.is_none() && self.structure.is_none() && !self.redo.is_empty(),
                    egui::Button::new("Повторить"),
                )
                .clicked()
            {
                self.redo();
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Справка").clicked() {
                    self.help = true;
                }
                ui.add(
                    egui::Label::new(format!(
                        "{}{}",
                        self.path
                            .as_ref()
                            .and_then(|p| p.file_name())
                            .map(|s| s.to_string_lossy().into_owned())
                            .unwrap_or_else(|| "Новый механизм".into()),
                        if self.dirty { " •" } else { "" }
                    ))
                    .truncate(),
                )
                .on_hover_text(
                    self.path
                        .as_ref()
                        .map(|p| p.display().to_string())
                        .unwrap_or_else(|| "Новый механизм".into()),
                );
            });
        });
        ui.horizontal(|ui| {
            if self.structure.is_some() {
                ui.strong("Ввод структуры");
            } else {
                for (tab, label) in [
                    (Tab::Mechanism, "Механизм"),
                    (Tab::Plans, "Планы M / V / A"),
                    (Tab::Table, "Результаты"),
                    (Tab::Graphs, "Графики"),
                ] {
                    ui.selectable_value(&mut self.tab, tab, label);
                }
            }
            ui.separator();
            if ui.button("Вписать").clicked() {
                let prepared = self.series.is_some()
                    && self.edit.is_none()
                    && self.draft.is_none()
                    && self.structure.is_none();
                for view in [&mut self.view, &mut self.v_view, &mut self.a_view] {
                    view.zoom = 1.0;
                    view.pan = egui::Vec2::ZERO;
                    view.fixed_bounds = prepared.then_some(view.fit_bounds).flatten();
                }
            }
            ui.checkbox(&mut self.view.grid, "Сетка");
            if self.receiver.is_some() {
                ui.spinner();
                ui.label("Расчёт…");
            }
        });
    }
    fn question_panel(&mut self, ui: &mut egui::Ui) {
        self.hover_answer = None;
        if let Some(error) = &self.error {
            ui.add_space(15.0);
            ui.colored_label(Color32::from_rgb(172, 49, 41), error);
        }
        if self.structure.is_some() {
            ui.heading("Все связи одним шагом");
            ui.label("Заполните таблицу справа: имя пары, её тип и два соединяемых звена. Названия звеньев могут повторяться в разных строках.");
            ui.label("R — вращательная пара, P — поступательная. Добавляйте и удаляйте строки; затем примите всю таблицу одной кнопкой.");
            ui.small("На схеме показана структура заполненных строк. Размеры и нагрузки будут введены после назначения стойки и ведущих звеньев.");
            return;
        }
        let question =
            self.edit
                .as_ref()
                .map(|e| e.question.clone())
                .or_else(|| match &self.state {
                    Some(DialogueState::Question { question, .. }) => Some(question.clone()),
                    _ => None,
                });
        if let Some(question) = question {
            ui.heading(if self.edit.is_some() {
                "Изменение параметра"
            } else {
                "Текущий вопрос"
            });
            ui.add_space(6.0);
            ui.label(RichText::new(fields::question_text(&question, self.model())).size(17.0));
            if question.path == FieldPath::Ground {
                if let Some(model) = self.model() {
                    ui.small(fields::mobility_summary(model));
                }
                if ui.button("Изменить связи").clicked() {
                    self.open_structure();
                    return;
                }
            }
            if !question.objects.is_empty() {
                ui.label(format!(
                    "На схеме: {}",
                    question
                        .objects
                        .iter()
                        .map(|n| n.text())
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }
            if let Some(unit) = question.units {
                ui.label(format!("Единицы: {unit}"));
            }
            if let Some(edit) = &self.edit {
                let old = if question.kind == QuestionKind::Number {
                    crate::ui_numbers::compact(&edit.old)
                } else {
                    edit.old.clone()
                };
                ui.label(format!("Сейчас: {old}"));
            }
            ui.add_space(12.0);
            let mut submit: Option<String> = None;
            match question.kind {
                QuestionKind::YesNo => {
                    ui.horizontal(|ui| {
                        let yes = ui.button("Да");
                        if yes.hovered() && self.pending.is_none() {
                            self.hover_answer = Some("Y");
                        }
                        if yes.clicked() {
                            submit = Some("Y".into());
                        }
                        let no = ui.button("Нет");
                        if no.hovered() && self.pending.is_none() {
                            self.hover_answer = Some("N");
                        }
                        if no.clicked() {
                            submit = Some("N".into());
                        }
                    });
                }
                QuestionKind::PairCommand => {
                    ui.horizontal(|ui| {
                        if ui.button("R · шарнир").clicked() {
                            submit = Some("R".into());
                        }
                        if ui.button("P · ползун").clicked() {
                            submit = Some("P".into());
                        }
                    });
                    ui.horizontal(|ui| {
                        if ui.button("Удалить пару").clicked() {
                            submit = Some("E".into());
                        }
                        if ui.button("Готово").clicked() {
                            submit = Some(" ".into());
                        }
                    });
                }
                _ => {
                    let response = ui.add(
                        egui::TextEdit::singleline(&mut self.answer).desired_width(f32::INFINITY),
                    );
                    if self.focus {
                        response.request_focus();
                        self.focus = false;
                    }
                    let enter = (response.has_focus() || response.lost_focus())
                        && ui.input(|i| i.key_pressed(egui::Key::Enter));
                    ui.horizontal(|ui| {
                        if ui.button("Принять").clicked() || enter {
                            submit = Some(self.answer.clone());
                        }
                        if question.kind == QuestionKind::Name
                            && ui.button("Пробел / закончить").clicked()
                        {
                            submit = Some(" ".into());
                        }
                    });
                    ui.small(if question.kind == QuestionKind::Name {
                        "Имя: один или два латинских символа."
                    } else {
                        "Дробная часть через точку; поддерживается 1.2E-3."
                    });
                }
            }
            if let Some(text) = submit {
                self.hover_answer = None;
                self.submit(&text);
            }
            if (self
                .edit
                .as_ref()
                .is_some_and(|e| !fields::structural(&e.path))
                || !self.restore_answers.is_empty())
                && ui.button("Принять остальные прежние · F10").clicked()
            {
                if self.edit.is_some() {
                    self.submit(&self.answer.clone());
                }
                self.restore_remaining();
            }
            ui.add_space(12.0);
            if self.edit.is_some() {
                if ui.button("Отменить изменение").clicked() {
                    self.clear_selection();
                }
            } else if ui
                .add_enabled(
                    !self.draft_undo.is_empty(),
                    egui::Button::new("Предыдущий ответ"),
                )
                .clicked()
            {
                self.undo();
            }
            if self.draft.is_some()
                && self.document.is_some()
                && ui.button("Отменить изменение структуры").clicked()
            {
                self.cancel_draft();
            }
        } else if matches!(self.state, Some(DialogueState::ReviewRequired { .. })) {
            ui.heading("Проверьте данные");
            ui.label("Выберите параметр в таблице, чтобы исправить ответ и продолжить описание.");
            if self.document.is_some() && ui.button("Отменить незавершённое изменение").clicked()
            {
                self.cancel_draft();
            }
        } else if self.document.is_some() {
            self.analysis_panel(ui);
        } else {
            ui.heading("ДиАДА");
            ui.label("Схема, параметры и вопрос рядом. При вводе выделяются элементы, к которым относится вопрос.");
            ui.add_space(18.0);
            if ui.button("Создать механизм").clicked() {
                self.request(Action::New);
            }
            if ui.button("Открыть .DI…").clicked() {
                self.request(Action::Open);
            }
        }
    }
    fn analysis_panel(&mut self, ui: &mut egui::Ui) {
        let previous = (
            self.fields.clone(),
            self.positions,
            self.forces,
            self.oscillation,
            self.motion_start.clone(),
            self.motion_end.clone(),
        );
        let mut command = None;
        ui.heading("Параметры расчёта");
        egui::ScrollArea::vertical()
            .id_salt("analysis_settings")
            .show(ui, |ui| {
                for input in &mut self.fields {
                    ui.separator();
                    ui.strong(format!("Ведущее звено {}", input.name));
                    for (label, value, unit) in [
                        (
                            "Начальное положение",
                            &mut input.start,
                            if input.rotational { "град" } else { "м" },
                        ),
                        (
                            "Скорость",
                            &mut input.rate,
                            if input.rotational {
                                "рад/с"
                            } else {
                                "м/с"
                            },
                        ),
                        (
                            "Ускорение",
                            &mut input.acceleration,
                            if input.rotational {
                                "рад/с²"
                            } else {
                                "м/с²"
                            },
                        ),
                        (
                            "Шаг серии",
                            &mut input.step,
                            if input.rotational { "град" } else { "м" },
                        ),
                    ] {
                        ui.horizontal(|ui| {
                            ui.label(label);
                            ui.add(egui::TextEdit::singleline(value).desired_width(75.0));
                            ui.small(unit);
                        });
                    }
                }
                ui.horizontal(|ui| {
                    ui.label("Положений");
                    ui.add(egui::DragValue::new(&mut self.positions).range(1..=65535));
                });
                let available = self
                    .document
                    .as_ref()
                    .is_some_and(|d| d.loaded().mechanism.force_analysis);
                ui.add_enabled(
                    available,
                    egui::Checkbox::new(&mut self.forces, "Силовой расчёт"),
                );
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(self.receiver.is_none(), egui::Button::new("Рассчитать"))
                        .clicked()
                    {
                        command = Some(RunAction::Calculate);
                    }
                    if ui.button("Стандартные 12").clicked() {
                        command = Some(RunAction::Standard);
                    }
                });
                ui.separator();
                ui.strong("Движение и графики");
                ui.checkbox(&mut self.oscillation, "Качание");
                if self.oscillation {
                    ui.horizontal(|ui| {
                        ui.label("От");
                        ui.add(
                            egui::TextEdit::singleline(&mut self.motion_start).desired_width(65.0),
                        );
                        ui.label("до");
                        ui.add(
                            egui::TextEdit::singleline(&mut self.motion_end).desired_width(65.0),
                        );
                        ui.small(if self.fields.first().is_some_and(|f| f.rotational) {
                            "град"
                        } else {
                            "м"
                        });
                    });
                }
                let rotating = self.fields.first().is_some_and(|f| f.rotational);
                ui.small(if rotating {
                    "Движение: знак скорости сохраняется, |ω| = 1. Графики: ω = 1, ε = 0."
                } else {
                    "Графики: v = 1, a = 0. Циклическое движение требует вращательного входа."
                });
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(self.receiver.is_none(), egui::Button::new("Движение · 72"))
                        .clicked()
                    {
                        command = Some(RunAction::Motion);
                    }
                    if ui
                        .add_enabled(
                            self.receiver.is_none(),
                            egui::Button::new(if rotating {
                                "Графики · 0–360°"
                            } else {
                                "Графики · 0–360 м"
                            }),
                        )
                        .clicked()
                    {
                        command = Some(RunAction::Graphs);
                    }
                });
                ui.separator();
                if ui
                    .add_enabled(self.series.is_some(), egui::Button::new("Отчёт .DIF…"))
                    .clicked()
                {
                    command = Some(RunAction::Export);
                }
                if ui.button("Состав отчёта…").clicked() {
                    command = Some(RunAction::Selection);
                }
            });
        if previous
            != (
                self.fields.clone(),
                self.positions,
                self.forces,
                self.oscillation,
                self.motion_start.clone(),
                self.motion_end.clone(),
            )
        {
            self.begin_series();
            self.notice = "Параметры изменены. Выполните новый расчёт.".into();
        }
        match command {
            Some(RunAction::Calculate) => self.calculate(),
            Some(RunAction::Standard) => {
                if let Some(m) = self.model() {
                    self.fields = InputFields::for_model(m);
                }
                self.positions = 12;
                self.calculate();
            }
            Some(RunAction::Motion) => self.motion(),
            Some(RunAction::Graphs) => self.graph_series(),
            Some(RunAction::Export) => self.export_report(),
            Some(RunAction::Selection) => self.selection_open = true,
            None => {}
        }
    }
    fn parameters(&mut self, ui: &mut egui::Ui) {
        if let Some(form) = &mut self.structure {
            let action = form.ui(ui);
            match action {
                Some(crate::structure_form::FormAction::Apply) => self.apply_structure(),
                Some(crate::structure_form::FormAction::Cancel) => self.cancel_structure(),
                None => {}
            }
            return;
        }
        let background = ui.interact(
            ui.max_rect(),
            ui.id().with("parameters_background"),
            egui::Sense::click(),
        );
        ui.horizontal(|ui| {
            ui.strong("Параметры механизма");
            for section in ["Размеры", "Нагрузки", "Структура"] {
                ui.selectable_value(&mut self.section, section, section);
            }
        });
        let entries = if let Some(draft) = &self.draft {
            draft
                .bindings()
                .iter()
                .map(|b| {
                    (
                        b.path.clone(),
                        b.answer_text(draft.journal()),
                        matches!(&b.value, AnswerValue::Number(_)),
                    )
                })
                .collect::<Vec<_>>()
        } else if let Some(document) = &self.document {
            document
                .bindings()
                .iter()
                .map(|b| {
                    (
                        b.path.clone(),
                        fields::raw(document, b),
                        matches!(&b.value, AnswerValue::Number(_)),
                    )
                })
                .collect::<Vec<_>>()
        } else if let Some(DialogueState::ReviewRequired { bindings, .. }) = &self.state {
            bindings
                .iter()
                .map(|b| {
                    (
                        b.path.clone(),
                        match &b.value {
                            AnswerValue::Name(v) => v.text(),
                            AnswerValue::Number(v) => crate::ui_numbers::real(*v),
                            AnswerValue::YesNo(v) => if *v { "Y" } else { "N" }.into(),
                            AnswerValue::Character(v) => decode_cp866(&[*v]),
                        },
                        matches!(&b.value, AnswerValue::Number(_)),
                    )
                })
                .collect()
        } else {
            Vec::new()
        };
        let model = self.model().cloned().unwrap_or_default();
        let mut chosen = None;
        egui::ScrollArea::vertical()
            .id_salt("parameters")
            .show(ui, |ui| {
                if self.section == "Структура" {
                    Self::structure(ui, &model, &mut self.view.selected);
                    ui.separator();
                }
                egui::Grid::new("parameter_table")
                    .num_columns(3)
                    .striped(true)
                    .spacing([20.0, 5.0])
                    .show(ui, |ui| {
                        ui.strong("Параметр");
                        ui.strong("Значение");
                        ui.strong("Единицы");
                        ui.end_row();
                        let mut seen = Vec::new();
                        for (index, (path, _, _)) in entries.iter().enumerate() {
                            if entries[index + 1..]
                                .iter()
                                .any(|(other, _, _)| other == path)
                            {
                                continue;
                            }
                            let (path, text, numeric) = entries[index].clone();
                            if seen.contains(&path)
                                || fields::section(&path) != self.section
                                || fields::confirmation(&path)
                            {
                                continue;
                            }
                            seen.push(path.clone());
                            let (label, unit, key) = fields::describe(&path, &model);
                            let selected = self.edit.as_ref().is_some_and(|e| e.path == path)
                                || key.is_some() && key == self.view.selected;
                            if ui.selectable_label(selected, label).clicked() {
                                chosen = Some(path.clone());
                            }
                            let display = if numeric {
                                crate::ui_numbers::compact(&text)
                            } else {
                                text.trim_end().to_owned()
                            };
                            if ui.selectable_label(selected, display).double_clicked() {
                                chosen = Some(path);
                            }
                            ui.small(unit);
                            ui.end_row();
                        }
                    });
            });
        if let Some(path) = chosen {
            self.start_edit(path);
        } else if background.clicked() {
            self.clear_selection();
        }
    }
    fn structure(ui: &mut egui::Ui, model: &Mechanism, selected: &mut Option<ObjectKey>) {
        let name = |id: LinkId| {
            model
                .links
                .get(id.0)
                .map(|l| l.name.display())
                .unwrap_or_default()
        };
        ui.label(format!(
            "Стойка: {} · Ведущие звенья: {}",
            model.ground.map(name).unwrap_or_default(),
            model
                .inputs
                .iter()
                .map(|id| name(*id))
                .collect::<Vec<_>>()
                .join(", ")
        ));
        egui::Grid::new("structure_pairs")
            .num_columns(3)
            .striped(true)
            .show(ui, |ui| {
                ui.strong("Пара / точка");
                ui.strong("Тип");
                ui.strong("Звенья");
                ui.end_row();
                for (index, pair) in model.pairs.iter().enumerate() {
                    let key = ObjectKey::Pair(index);
                    if ui
                        .selectable_label(*selected == Some(key), pair.name.display())
                        .clicked()
                    {
                        *selected = Some(key);
                    }
                    ui.label(match pair.kind {
                        PairKind::Revolute => "R · шарнир",
                        PairKind::Prismatic => "P · ползун",
                        PairKind::Point => "T · точка",
                        PairKind::MassCenter => "M · центр масс",
                        PairKind::ForcePoint => "F · точка силы",
                    });
                    ui.label(
                        if matches!(pair.kind, PairKind::Revolute | PairKind::Prismatic) {
                            format!("{}, {}", name(pair.links[0]), name(pair.links[1]))
                        } else {
                            name(pair.links[0])
                        },
                    );
                    ui.end_row();
                }
            });
        if !model.groups.is_empty() {
            ui.add_space(8.0);
            egui::Grid::new("structure_groups")
                .num_columns(3)
                .striped(true)
                .show(ui, |ui| {
                    ui.strong("Группа");
                    ui.strong("Вид");
                    ui.strong("Звенья");
                    ui.end_row();
                    for (index, group) in model.groups.iter().enumerate() {
                        ui.label((index + 1).to_string());
                        ui.label(match group.kind {
                            GroupKind::Ground => "Стойка",
                            GroupKind::Input => "Ведущее звено",
                            GroupKind::Dyad => "Диада",
                            GroupKind::Attached => "Присоединённая точка",
                        });
                        ui.label(
                            group
                                .link_b
                                .map(|b| format!("{}, {}", name(group.link_a), name(b)))
                                .unwrap_or_else(|| name(group.link_a)),
                        );
                        ui.end_row();
                    }
                });
        }
    }
    fn controls(&mut self, ui: &mut egui::Ui) {
        let Some(series) = &self.series else {
            return;
        };
        let len = series.frames.len();
        if len == 0 {
            return;
        }
        ui.horizontal(|ui| {
            if ui.button("◀").clicked() {
                self.current = (self.current + len - 1) % len;
                self.playing = false;
            }
            if ui
                .button(if self.playing {
                    "Пауза"
                } else {
                    "Пуск"
                })
                .clicked()
            {
                self.playing = !self.playing;
                self.tick = Instant::now();
            }
            if ui.button("▶").clicked() {
                self.current = (self.current + 1) % len;
                self.playing = false;
            }
            ui.add(egui::Slider::new(&mut self.current, 0..=len - 1).show_value(false));
            ui.label(format!("{} / {}", self.current + 1, len));
            ui.label("Кадров/с");
            ui.add(egui::DragValue::new(&mut self.fps).range(1.0..=60.0));
            ui.small(if series.kind.is_analogs() {
                "Аналоги"
            } else {
                "Заданные производные"
            });
        });
        let values = if let Some(Ok(frame)) = series.frames.get(self.current) {
            frame
                .inputs
                .iter()
                .zip(&series.config.inputs)
                .map(|(input, config)| {
                    let field = self.fields.iter().find(|f| f.link == config.link);
                    let number = |v| {
                        diada_compat::format_real(v, 0, Some(2))
                            .map(|text| crate::ui_numbers::compact(&text))
                            .unwrap_or_default()
                    };
                    format!(
                        "{}: {} = {} {}; {} = {}; {} = {}",
                        field.map(|f| f.name.as_str()).unwrap_or("?"),
                        if field.is_some_and(|f| f.rotational) {
                            "φ"
                        } else {
                            "s"
                        },
                        number(input.position),
                        if field.is_some_and(|f| f.rotational) {
                            "°"
                        } else {
                            "м"
                        },
                        if field.is_some_and(|f| f.rotational) {
                            "ω"
                        } else {
                            "v"
                        },
                        number(input.rate),
                        if field.is_some_and(|f| f.rotational) {
                            "ε"
                        } else {
                            "a"
                        },
                        number(input.rate_derivative)
                    )
                })
                .collect::<Vec<_>>()
                .join(" · ")
        } else {
            String::new()
        };
        // A failed frame must not remove a footer row and move the canvas.
        let text = if values.is_empty() { " " } else { &values };
        ui.add(egui::Label::new(RichText::new(text).small()).truncate())
            .on_hover_text(values);
    }
    fn trace_visible(&self) -> bool {
        self.trajectories
            && self.edit.is_none()
            && self.draft.is_none()
            && self.structure.is_none()
            && self
                .series
                .as_ref()
                .is_some_and(|series| series.kind == SeriesKind::Motion)
    }
    fn record_motion_trace(&mut self) {
        if let Some(series) = &self.series
            && series.kind == SeriesKind::Motion
            && let Some(Ok(frame)) = series.frames.get(self.current)
        {
            self.motion_trace.record(&frame.mechanism, self.current);
        }
    }
    fn canvas(&mut self, ui: &mut egui::Ui) {
        self.canvas_contents(ui, true);
    }
    fn canvas_contents(&mut self, ui: &mut egui::Ui, show_body: bool) {
        let frame = self
            .series
            .as_ref()
            .and_then(|s| s.frames.get(self.current));
        if let Some(series) = &self.series {
            // Reserve the same two status rows on every frame, including
            // assembling ones, so an error never shifts a fixed support.
            if let Some(Err(error)) = frame {
                let message = analysis::error(error);
                ui.add(
                    egui::Label::new(RichText::new(&message).color(Color32::from_rgb(172, 49, 41)))
                        .truncate(),
                )
                .on_hover_text(message);
                let valid = series
                    .kinematics_valid
                    .get(self.current)
                    .copied()
                    .unwrap_or(false);
                let detail = if valid {
                    "Положение определено; реакции недоступны."
                } else {
                    "Показана только геометрия, определённая до ошибки."
                };
                ui.add(egui::Label::new(detail).truncate())
                    .on_hover_text(detail);
            } else {
                ui.add(egui::Label::new(" ").truncate());
                ui.add(egui::Label::new(" ").truncate());
            }
        }
        if let Some(Err(error)) = frame {
            let series = self.series.as_ref().unwrap();
            let valid = series
                .kinematics_valid
                .get(self.current)
                .copied()
                .unwrap_or(false);
            if let Some(partial) = series
                .failed_previews
                .get(self.current)
                .and_then(|p| p.as_ref())
            {
                let mut scene = crate::failure::preview(
                    partial,
                    error,
                    valid,
                    series.kind == SeriesKind::Motion,
                );
                if self.trace_visible() {
                    scene.trace_dots = self.motion_trace.dots();
                }
                if !show_body {
                    scene = Scene {
                        trace_dots: scene.trace_dots,
                        units: scene.units,
                        ..Default::default()
                    };
                }
                let interaction = scene.paint(ui, &mut self.view);
                self.canvas_interaction(interaction);
            }
            return;
        }
        let model = frame
            .and_then(|f| f.as_ref().ok())
            .map(|f| f.mechanism.clone())
            .or_else(|| self.model().cloned());
        let Some(mut model) = model else {
            ui.centered_and_justified(|ui| {
                ui.label(
                    RichText::new("ДиАДА")
                        .size(30.0)
                        .color(Color32::from_gray(130)),
                );
            });
            return;
        };
        let incomplete =
            self.document.is_none() || self.state.is_some() || self.structure.is_some();
        let mut scene = if incomplete {
            Scene::topology(&model)
        } else if self
            .series
            .as_ref()
            .is_some_and(|s| s.kind == SeriesKind::Motion)
        {
            Scene::motion(&model)
        } else {
            Scene::mechanism(&model)
        };
        let preview_answer = self.preview_answer().to_owned();
        let mut schematic = incomplete;
        let mut draft_notice = None;
        let mut provisional = Vec::new();
        let mut geometry_ready = !incomplete;
        if self.structure.is_none()
            && let Some((live, hint)) = self.draft_projection()
        {
            schematic = live.schematic;
            geometry_ready = live.geometry_ready;
            provisional = live.provisional;
            draft_notice = Some(live.notice);
            scene = live.scene;
            model = live.display_model;
            if let Some(hint) = hint {
                ui.small(hint);
            }
        }
        let mut preview_failed = false;
        if !incomplete && self.series.is_none() {
            if let Ok(config) = analysis::config(&self.fields, 1, false) {
                let inputs = config
                    .inputs
                    .iter()
                    .map(|s| InputPosition {
                        position: s.start,
                        other_position: s.other_start,
                        rate: s.rate,
                        rate_derivative: s.rate_derivative,
                    })
                    .collect::<Vec<_>>();
                match model.calculate_position(&inputs) {
                    Ok(()) => scene = Scene::mechanism(&model),
                    Err(error) => {
                        ui.colored_label(Color32::from_rgb(172, 49, 41), analysis::error(&error));
                        scene = Scene::topology(&model);
                        preview_failed = true;
                    }
                }
            } else {
                scene = Scene::topology(&model);
                preview_failed = true;
                ui.label("Проверьте входные параметры: числовое положение пока не определено.");
            }
        }
        let mut candidate_failed = false;
        if let (Some(edit), Some(document)) = (&self.edit, &self.document)
            && !fields::structural(&edit.path)
            && preview_answer != edit.old
        {
            let mut candidate = document.clone();
            if candidate
                .edit_text(&edit.path, &preview_answer, &TpNumbers)
                .is_ok()
            {
                let mut proposed = candidate.loaded().mechanism.clone();
                let inputs = frame
                    .and_then(|f| f.as_ref().ok())
                    .map(|f| f.inputs.clone())
                    .or_else(|| {
                        analysis::config(&self.fields, 1, false).ok().map(|c| {
                            c.inputs
                                .into_iter()
                                .map(|s| InputPosition {
                                    position: s.start,
                                    other_position: s.other_start,
                                    rate: s.rate,
                                    rate_derivative: s.rate_derivative,
                                })
                                .collect()
                        })
                    });
                if let Some(inputs) = inputs {
                    match proposed.calculate_position(&inputs) {
                        Ok(()) => {
                            scene = Scene::mechanism(&proposed);
                            model = proposed;
                            schematic = false;
                            ui.small("Предпросмотр изменения · данные ещё не приняты");
                        }
                        Err(error) => {
                            candidate_failed = true;
                            ui.colored_label(
                                Color32::from_rgb(172, 49, 41),
                                format!("Предпросмотр: {}", analysis::error(&error)),
                            );
                            ui.small("Изменение не принято. Показана последняя подтверждённая геометрия.");
                        }
                    }
                }
            } else if let Ok(mut trial) = document.begin_edit(&edit.path)
                && trial.next(&TpNumbers).is_ok()
                && let Ok(next) = trial.answer(&preview_answer, &TpNumbers)
                && !matches!(&next, DialogueState::Question { question, .. } if question.path == edit.path)
                && let Some(proposed) = trial.preview()
            {
                let live = crate::live_preview::build(proposed, trial.bindings(), &self.fields);
                scene = live.scene;
                model = live.display_model;
                schematic = live.schematic;
                geometry_ready = live.geometry_ready;
                provisional = live.provisional;
                ui.small(live.notice);
            }
        }
        if let Some(notice) = draft_notice {
            ui.small(notice);
        } else if incomplete || preview_failed {
            ui.small("Структурный предпросмотр · размеры ещё вводятся");
        } else if self.series.is_none() {
            ui.small("Геометрия из описания · рассчитайте положение для проверки");
        }
        let question = self
            .edit
            .as_ref()
            .map(|e| &e.question)
            .or(match &self.state {
                Some(DialogueState::Question { question, .. }) => Some(question),
                _ => None,
            });
        self.view.highlighted.clear();
        let structure_preview = self.structure.is_some();
        let saved_selection = self.view.selected;
        if let Some(form) = &self.structure {
            self.view.selected = None;
            if let Some((pair_id, links)) = form.selected_preview_objects()
                && let Some(pair) = model.pair(pair_id)
            {
                for (index, candidate) in model.pairs.iter().enumerate() {
                    if candidate.name == pair.name && candidate.kind == pair.kind {
                        self.view.highlighted.push(ObjectKey::Pair(index));
                    }
                }
                self.view
                    .highlighted
                    .extend(links.map(|link| ObjectKey::Link(link.0)));
            }
        } else if let Some(question) = question {
            for name in &question.objects {
                for (i, pair) in model.pairs.iter().enumerate() {
                    if pair.name.0 == name.0 {
                        self.view.highlighted.push(ObjectKey::Pair(i));
                    }
                }
                for (i, link) in model.links.iter().enumerate() {
                    if link.name.0 == name.0 {
                        self.view.highlighted.push(ObjectKey::Link(i));
                    }
                }
            }
            if let FieldPath::LinkOnGuide(id) = question.path
                && let Some((hinge, guide)) = fields::rp_pairs(&model, id)
            {
                self.view
                    .highlighted
                    .extend([ObjectKey::Pair(hinge.0), ObjectKey::Pair(guide.0)]);
            }
            let annotation_answer = if candidate_failed {
                self.edit
                    .as_ref()
                    .map(|e| e.old.as_str())
                    .unwrap_or(&preview_answer)
            } else {
                &preview_answer
            };
            crate::preview::annotate(
                &mut scene,
                &model,
                question,
                annotation_answer,
                schematic
                    || preview_failed
                    || !geometry_ready
                        && matches!(question.path, FieldPath::LinkOnGuide(id) if provisional.contains(&ObjectKey::Link(id.0))),
            );
            if let Some(edit) = &self.edit
                && question.kind == QuestionKind::Number
                && !candidate_failed
                && annotation_answer != edit.old
                && analysis::parse(annotation_answer).is_ok()
            {
                crate::preview::compare_dimension(
                    &mut scene,
                    question,
                    &edit.old,
                    annotation_answer,
                );
            }
        }
        if self.trace_visible() {
            scene.trace_dots = self.motion_trace.dots();
        }
        if !show_body {
            scene = Scene {
                trace_dots: scene.trace_dots,
                units: scene.units,
                ..Default::default()
            };
        }
        let interaction = scene.paint(ui, &mut self.view);
        if structure_preview {
            self.view.selected = saved_selection;
        } else {
            self.canvas_interaction(interaction);
        }
    }
    fn plans(&mut self, ui: &mut egui::Ui) {
        let has_points = self
            .model()
            .is_some_and(|model| !crate::preview::trajectory_points(model).is_empty());
        let motion_ready = self.edit.is_none()
            && self.draft.is_none()
            && self.structure.is_none()
            && self
                .series
                .as_ref()
                .is_some_and(|series| series.kind == SeriesKind::Motion);
        ui.horizontal_wrapped(|ui| {
            ui.checkbox(&mut self.show_m, "M · механизм");
            ui.checkbox(&mut self.show_v, "V · скорости");
            ui.checkbox(&mut self.show_a, "A · ускорения");
            ui.add_enabled(motion_ready && has_points, egui::Checkbox::new(&mut self.trajectories, "Траектории точек"))
                .on_hover_text("Как в ДиАДА: точечный след дополнительных точек, центров масс и точек сил. След накапливается в 72 позициях при просмотре движения.");
        });
        let hint = if self.document.is_none()
            || self.draft.is_some()
            || self.edit.is_some()
            || self.structure.is_some()
        {
            "Завершите ввод механизма, затем подготовьте движение для показа следа дополнительных точек."
        } else if !has_points {
            "Для траекторий задайте дополнительные точки для исследования, центры масс или точки сил при описании механизма."
        } else if !motion_ready {
            "Траектории доступны в движении. Подготовьте «Движение · 72», затем нажмите «Пуск» или переключайте кадры."
        } else if self.trajectories {
            "Точечный след накапливается при просмотре кадров движения, как в ДиАДА."
        } else {
            " "
        };
        ui.add(egui::Label::new(RichText::new(hint).small()).truncate())
            .on_hover_text(hint);
        if !self.show_m && self.series.is_some() {
            let message = self
                .series
                .as_ref()
                .and_then(|s| s.frames.get(self.current))
                .and_then(|frame| frame.as_ref().err())
                .map(analysis::error)
                .unwrap_or_else(|| " ".into());
            ui.add(
                egui::Label::new(RichText::new(&message).color(Color32::from_rgb(172, 49, 41)))
                    .truncate(),
            )
            .on_hover_text(message);
        }
        let model = self
            .series
            .as_ref()
            .and_then(|s| s.frames.get(self.current))
            .and_then(|f| f.as_ref().ok())
            .map(|f| f.mechanism.clone())
            .or_else(|| {
                self.series
                    .as_ref()
                    .filter(|s| s.kinematics_valid.get(self.current) == Some(&true))
                    .and_then(|s| s.failed_previews.get(self.current))
                    .and_then(|p| p.clone())
            });
        let show_trace = self.trace_visible();
        let count = usize::from(self.show_m || show_trace)
            + usize::from(self.show_v)
            + usize::from(self.show_a);
        if count == 0 {
            ui.label("Выберите план.");
            return;
        }
        ui.columns(count, |columns| {
            let mut n = 0;
            if self.show_m || show_trace {
                self.canvas_contents(&mut columns[n], self.show_m);
                n += 1;
            }
            for (show, acceleration) in [(self.show_v, false), (self.show_a, true)] {
                if !show {
                    continue;
                }
                let ui = &mut columns[n];
                n += 1;
                ui.label(if acceleration {
                    "План ускорений"
                } else {
                    "План скоростей"
                });
                if let Some(model) = &model {
                    match crate::preview::plan(model, acceleration) {
                        Ok(scene) => {
                            scene.paint(
                                ui,
                                if acceleration {
                                    &mut self.a_view
                                } else {
                                    &mut self.v_view
                                },
                            );
                        }
                        Err(e) => {
                            ui.label(e);
                        }
                    }
                } else {
                    ui.label("Рассчитайте положение механизма.");
                }
            }
        });
    }
    fn results(&mut self, ui: &mut egui::Ui) {
        if let Some(series) = &self.series {
            match series.frames.get(self.current) {
                Some(Ok(frame)) => match diada_io::position_report(
                    frame,
                    series.config.mode == AnalysisMode::Kinetostatics,
                ) {
                    Ok(report) => {
                        crate::results::show(ui, &report, &frame.mechanism, &mut self.view.selected)
                    }
                    Err(e) => {
                        ui.label(e.to_string());
                    }
                },
                Some(Err(error)) => {
                    ui.colored_label(Color32::from_rgb(172, 49, 41), analysis::error(error));
                    if series.kinematics_valid.get(self.current) == Some(&true)
                        && let Some(model) = series
                            .failed_previews
                            .get(self.current)
                            .and_then(|p| p.as_ref())
                    {
                        ui.label("Кинематика определена. Реакции и силовой отчёт недоступны.");
                        let snapshot = CalculationSnapshot {
                            position_index: self.current,
                            inputs: Vec::new(),
                            mechanism: model.clone(),
                        };
                        if let Ok(report) = diada_io::position_report(&snapshot, false) {
                            crate::results::show(ui, &report, model, &mut self.view.selected);
                        }
                    }
                }
                _ => {}
            }
        } else {
            ui.label("Выполните расчёт положения или серии.");
        }
    }
    fn selection_dialog(&mut self, ctx: &egui::Context) {
        let Some(frame) = self
            .series
            .as_ref()
            .and_then(|s| s.frames.iter().find_map(|f| f.as_ref().ok()))
        else {
            if self.selection_open {
                self.error = Some("Сначала выполните расчёт для выбора объектов отчёта.".into());
                self.selection_open = false;
            }
            return;
        };
        let Ok(report) = diada_io::position_report(
            frame,
            self.series
                .as_ref()
                .is_some_and(|s| s.config.mode == AnalysisMode::Kinetostatics),
        ) else {
            return;
        };
        egui::Window::new("Состав отчёта")
            .open(&mut self.selection_open)
            .show(ctx, |ui| {
                ui.label("Порядок строк — как в оригинале. Первое ведущее звено выводится всегда.");
                for (label, names, selection) in [
                    (
                        "Звенья",
                        report.links.iter().map(|v| v.name).collect::<Vec<_>>(),
                        &mut self.selection.links,
                    ),
                    (
                        "Пары и точки",
                        report.points.iter().map(|v| v.name).collect::<Vec<_>>(),
                        &mut self.selection.points,
                    ),
                    (
                        "Реакции",
                        report.reactions.iter().map(|v| v.name).collect::<Vec<_>>(),
                        &mut self.selection.reactions,
                    ),
                ] {
                    ui.separator();
                    ui.strong(label);
                    ui.horizontal(|ui| {
                        let mut all = selection.is_none();
                        if ui.checkbox(&mut all, "Все").changed() {
                            *selection = if all { None } else { Some(Vec::new()) };
                        }
                        if ui.button("Ничего").clicked() {
                            *selection = Some(Vec::new());
                        }
                    });
                    if let Some(selected) = selection {
                        ui.horizontal_wrapped(|ui| {
                            for name in names {
                                let mut checked = selected.contains(&name);
                                if ui.checkbox(&mut checked, name.text()).changed() {
                                    if checked {
                                        selected.push(name)
                                    } else {
                                        selected.retain(|n| *n != name)
                                    }
                                }
                            }
                        });
                    }
                }
            });
    }
    fn modal(&mut self, ctx: &egui::Context) {
        if let Some(action) = self.pending {
            let mut choice = None;
            egui::Modal::new(egui::Id::new("save_before_action")).show(ctx, |ui| {
                ui.heading("Есть несохранённые изменения");
                ui.label("Сохранить текущий механизм перед продолжением?");
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(
                            self.document.is_some()
                                && self.draft.is_none()
                                && self.structure.is_none(),
                            egui::Button::new("Сохранить"),
                        )
                        .clicked()
                    {
                        choice = Some(0);
                    }
                    if ui.button("Без сохранения").clicked() {
                        choice = Some(1);
                    }
                    if ui.button("Отмена").clicked() {
                        choice = Some(2);
                    }
                });
            });
            let proceed = match choice {
                Some(0) => self.save(false),
                Some(1) => true,
                Some(2) => {
                    self.pending = None;
                    false
                }
                _ => false,
            };
            if proceed {
                self.perform(action);
                if action == Action::Close {
                    self.dirty = false;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
        }
    }
    fn shortcuts(&mut self, ctx: &egui::Context) {
        if self.pending.is_some() {
            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
                self.pending = None;
            }
            return;
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::S)) {
            self.save(false);
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::O)) {
            self.request(Action::Open);
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::N)) {
            self.request(Action::New);
        }
        if self.structure.is_none()
            && ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::Z))
        {
            self.undo();
        }
        if self.structure.is_none()
            && ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::Y))
        {
            self.redo();
        }
        if self.structure.is_none()
            && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::F10))
        {
            if self
                .edit
                .as_ref()
                .is_some_and(|e| !fields::structural(&e.path))
            {
                self.submit(&self.answer.clone());
            }
            self.restore_remaining();
        }
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            if self.structure.is_some() {
                self.cancel_structure();
            }
            self.clear_selection();
            self.playing = false;
            self.help = false;
        }
        if !ctx.egui_wants_keyboard_input()
            && let Some(series) = &self.series
        {
            let len = series.frames.len();
            if len > 0 {
                if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowRight)) {
                    self.current = (self.current + 1) % len;
                }
                if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowLeft)) {
                    self.current = (self.current + len - 1) % len;
                }
            }
        }
    }
}
impl eframe::App for Workbench {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.hover_answer = None;
        let ctx = ui.ctx().clone();
        self.receive();
        self.shortcuts(&ctx);
        if ctx.input(|i| i.viewport().close_requested()) && self.dirty && self.capture.is_none() {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.pending = Some(Action::Close);
        }
        if self.playing {
            if let Some(series) = &self.series
                && !series.frames.is_empty()
                && self.tick.elapsed() >= Duration::from_secs_f32(1.0 / self.fps)
            {
                self.current = (self.current + 1) % series.frames.len();
                self.tick = Instant::now();
            }
            ctx.request_repaint_after(Duration::from_secs_f32(1.0 / self.fps));
        }
        if self.receiver.is_some() {
            ctx.request_repaint_after(Duration::from_millis(50));
        }
        egui::Panel::top("toolbar").show(ui, |ui| self.toolbar(ui));
        egui::Panel::bottom("status").show(ui, |ui| {
            ui.small(&self.notice);
            self.controls(ui);
        });
        self.record_motion_trace();
        if self.structure.is_some() {
            egui::Panel::right("structure_editor")
                .default_size(460.0)
                .min_size(420.0)
                .max_size((ctx.content_rect().width() - 300.0).max(420.0))
                .resizable(true)
                .show(ui, |ui| {
                    ui.take_available_width();
                    self.parameters(ui);
                });
            egui::CentralPanel::default().show(ui, |ui| self.canvas(ui));
        } else {
            egui::Panel::right("question")
                .default_size(325.0)
                .min_size(265.0)
                .resizable(true)
                .show(ui, |ui| {
                    ui.take_available_width();
                    if self.document.is_some() && self.edit.is_none() && self.draft.is_none() {
                        self.question_panel(ui);
                    } else {
                        egui::ScrollArea::vertical()
                            .id_salt("question_scroll")
                            .show(ui, |ui| self.question_panel(ui));
                    }
                });
            egui::Panel::bottom("parameters_panel")
                .default_size(225.0)
                .min_size(110.0)
                .resizable(true)
                .show(ui, |ui| {
                    ui.take_available_height();
                    self.parameters(ui);
                });
            egui::CentralPanel::default().show(ui, |ui| match self.tab {
                Tab::Mechanism => self.canvas(ui),
                Tab::Plans => self.plans(ui),
                Tab::Table => self.results(ui),
                Tab::Graphs => {
                    crate::plots::show(ui, self.series.as_ref(), &mut self.graph, &mut self.current)
                }
            });
        }
        self.selection_dialog(&ctx);
        self.modal(&ctx);
        if let Some(capture) = &mut self.capture {
            capture.update(
                &ctx,
                self.receiver.is_none()
                    && (self.series.is_some()
                        || self.document.is_none()
                        || self.error.is_some()
                        || self.structure.is_some()),
            );
        }
        if self.help {
            egui::Window::new("Работа с ДиАДА").open(&mut self.help).default_width(520.0).show(&ctx,|ui|{
            ui.heading("Схема, параметры и вопрос рядом");ui.label("Создайте механизм: задайте все пары и звенья в общей таблице связей, затем ответьте на вопросы о стойке, входах, размерах и нагрузках. Можно открыть готовый .DI. Выберите параметр в таблице для изменения. После изменения структуры зависимые размеры вводятся заново.");
            ui.separator();ui.label("Расчёт использует положение, скорость, ускорение и шаг каждого входа. Стандартная серия: 12 положений через 30°, скорость 1, ускорение 0. Движение — 72 кадра на проход; качание идёт в обе стороны. Графики — 73 точки 0–360° включительно.");
            ui.separator();ui.label(".DI — описание механизма; .DIF — совместимый текстовый отчёт DOS для чтения и последующей печати. Неопределённые значения не выдаются как достоверные результаты.");
            ui.separator();ui.label("Колесо — масштаб; перетаскивание — перемещение. Ctrl+S — сохранить, Ctrl+O — открыть, Ctrl+N — создать, Ctrl+Z/Ctrl+Y — отменить/повторить. Стрелки влево/вправо — положение серии.");
        });
        }
    }
}

#[cfg(test)]
#[path = "workbench_resize_tests.rs"]
mod resize_tests;

#[cfg(test)]
#[path = "workbench_live_tests.rs"]
mod live_tests;

#[cfg(test)]
#[path = "workbench_trajectory_tests.rs"]
mod trajectory_tests;

#[cfg(test)]
#[path = "workbench_dyad_tests.rs"]
mod dyad_tests;

#[cfg(test)]
#[path = "workbench_motion_tests.rs"]
mod motion_tests;

#[cfg(test)]
#[path = "workbench_number_tests.rs"]
mod number_tests;

#[cfg(test)]
#[path = "workbench_selection_tests.rs"]
mod selection_tests;

#[cfg(test)]
mod tests {
    use super::*;
    fn loaded(bytes: &[u8]) -> Workbench {
        let mut app = Workbench::blank();
        let doc = EditableDialogue::open(bytes, "SCENE", &TpNumbers).unwrap();
        app.fields = InputFields::for_model(&doc.loaded().mechanism);
        app.document = Some(doc);
        app
    }
    const SCENE: &[u8] = include_bytes!("../../../../tests/fixtures/diada-v320-derived/SCENE.DI");
    const FORCE: &[u8] =
        include_bytes!("../../../../tests/fixtures/diada-v320-derived/force-fixed-02/SCENE.DI");
    #[test]
    fn enter_in_the_actual_egui_number_editor_accepts_after_textedit_surrenders_focus() {
        let mut app = loaded(SCENE);
        let path = app
            .document
            .as_ref()
            .unwrap()
            .bindings()
            .iter()
            .find(|b| matches!(b.path, FieldPath::GroundAngle(_)))
            .unwrap()
            .path
            .clone();
        app.start_edit(path.clone());
        app.answer = "17.25".into();
        let ctx = egui::Context::default();
        let raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(340.0, 600.0),
            )),
            ..Default::default()
        };
        let mut output = ctx.run_ui(raw.clone(), |ui| app.question_panel(ui));
        output.textures_delta.clear();
        let mut enter = raw;
        enter.events.push(egui::Event::Key {
            key: egui::Key::Enter,
            physical_key: Some(egui::Key::Enter),
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        });
        let mut output = ctx.run_ui(enter, |ui| app.question_panel(ui));
        output.textures_delta.clear();
        assert!(app.edit.is_none(), "Enter must submit the typed response");
        assert_eq!(
            app.document.as_ref().unwrap().answer_text(&path).unwrap(),
            "17.25"
        );
    }
    #[test]
    fn motion_fit_keeps_the_defined_input_visible_when_the_dyad_fails_over_half_the_cycle() {
        let mut model = Mechanism::default();
        let ground = model.add_link(Name([b'0', b' ']));
        let crank = model.add_link(Name([b'1', b' ']));
        let rod = model.add_link(Name([b'2', b' ']));
        let rocker = model.add_link(Name([b'3', b' ']));
        model
            .add_pair(PairKind::Revolute, Name([b'O', b' ']), [ground, crank])
            .unwrap();
        model
            .add_pair(PairKind::Revolute, Name([b'A', b' ']), [crank, rod])
            .unwrap();
        let fixed = model
            .add_pair(PairKind::Revolute, Name([b'D', b' ']), [ground, rocker])
            .unwrap();
        model
            .add_pair(PairKind::Revolute, Name([b'B', b' ']), [rod, rocker])
            .unwrap();
        model.pairs[fixed.0].state.x = analysis::parse("4").unwrap();
        for (id, length) in [(crank, "1"), (rod, "1.5"), (rocker, "2")] {
            model.links[id.0].geometry.parameter = analysis::parse(length).unwrap();
        }
        model.set_ground_and_inputs(ground, vec![crank]).unwrap();
        model.prepare_initial_groups().unwrap();
        let dyad = model.append_dyad(rod, rocker).unwrap();
        model.groups[dyad.0].assembly = b'+';
        let configs = model
            .original_motion_configs(
                analysis::parse("1").unwrap(),
                None,
                AnalysisMode::Kinematics,
            )
            .unwrap();
        let series = analysis::calculate_passes(model, configs);
        assert!(series.frames[36].is_err());
        let (sender, receiver) = mpsc::channel();
        sender.send(series).unwrap();
        let mut app = Workbench::blank();
        app.receiver = Some(receiver);
        app.receive();
        let bounds = app.view.fixed_bounds.unwrap();
        assert!(
            bounds.min.x <= -0.999,
            "negative crank positions cannot disappear when the follower fails: {bounds:?}"
        );
        assert!(
            bounds.max.y >= 0.999,
            "the full defined crank circle must fit, not only its assembling sector: {bounds:?}"
        );
    }
    #[test]
    fn new_mechanism_starts_with_one_connections_table_and_empty_apply_is_transactional() {
        let mut app = Workbench::blank();
        app.perform(Action::New);
        assert!(app.structure.is_some());
        assert!(app.draft.is_none());
        assert!(app.state.is_none());
        app.apply_structure();
        assert!(app.error.is_some());
        assert!(app.structure.is_some());
        assert!(app.document.is_none());
        app.cancel_structure();
        assert!(!app.dirty);
        assert!(app.structure.is_none());
    }
    #[test]
    fn the_users_seven_connections_accept_ground_zero_via_the_actual_egui_enter_key() {
        let mut model = Mechanism::default();
        for (name, kind, a, b) in [
            (*b"O ", PairKind::Revolute, b'0', b'1'),
            (*b"A ", PairKind::Revolute, b'1', b'2'),
            (*b"B ", PairKind::Revolute, b'2', b'3'),
            (*b"C ", PairKind::Prismatic, b'3', b'0'),
            (*b"D ", PairKind::Revolute, b'2', b'4'),
            (*b"E ", PairKind::Revolute, b'4', b'5'),
            (*b"F ", PairKind::Prismatic, b'5', b'0'),
        ] {
            let a = model.add_link(Name([a, b' ']));
            let b = model.add_link(Name([b, b' ']));
            model.add_pair(kind, Name(name), [a, b]).unwrap();
        }
        assert_eq!(model.formal_mobility(), 1);
        let mut app = Workbench::blank();
        app.perform(Action::New);
        app.structure = Some(crate::structure_form::StructureForm::from_model(&model));
        app.apply_structure();
        assert!(app.error.is_none(), "{:?}", app.error);
        assert!(
            matches!(&app.state, Some(DialogueState::Question { question, .. }) if question.path == FieldPath::Ground)
        );
        app.answer = "0".into();
        let ctx = egui::Context::default();
        let mut frame = eframe::Frame::_new_kittest();
        let raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1320.0, 840.0),
            )),
            ..Default::default()
        };
        let mut output = ctx.run_ui(raw.clone(), |ui| eframe::App::ui(&mut app, ui, &mut frame));
        output.textures_delta.clear();
        let mut enter = raw;
        enter.events.push(egui::Event::Key {
            key: egui::Key::Enter,
            physical_key: Some(egui::Key::Enter),
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        });
        let mut output = ctx.run_ui(enter, |ui| eframe::App::ui(&mut app, ui, &mut frame));
        output.textures_delta.clear();
        assert!(app.error.is_none(), "{:?}", app.error);
        assert!(
            matches!(&app.state, Some(DialogueState::Question { question, .. }) if question.path == FieldPath::Input(0)),
            "{:?}",
            app.state
        );
        app.submit("1");
        assert!(app.error.is_none(), "{:?}", app.error);
        let model = app.model().unwrap();
        assert_eq!(
            model.link(model.ground.unwrap()).unwrap().name,
            Name(*b"0 ")
        );
        assert_eq!(model.link(model.inputs[0]).unwrap().name, Name(*b"1 "));
    }
    #[test]
    fn a_zero_mobility_table_stays_editable_and_reports_counts_before_asking_for_ground() {
        let mut model = Mechanism::default();
        let a = model.add_link(Name(*b"0 "));
        let b = model.add_link(Name(*b"1 "));
        let c = model.add_link(Name(*b"2 "));
        for (name, ends) in [(*b"A ", [a, b]), (*b"B ", [b, c]), (*b"C ", [c, a])] {
            model
                .add_pair(PairKind::Revolute, Name(name), ends)
                .unwrap();
        }
        let mut app = Workbench::blank();
        app.perform(Action::New);
        app.structure = Some(crate::structure_form::StructureForm::from_model(&model));
        app.apply_structure();
        let error = app.error.as_ref().unwrap();
        assert!(error.contains("Звеньев: 3; пар: 3"), "{error}");
        assert!(error.contains("= 0") && error.contains("W > 0"), "{error}");
        assert!(!error.contains("Core(") && !error.contains("formal mobility"));
        assert!(app.structure.is_some());
        assert!(app.state.is_none() && app.draft.is_none() && app.document.is_none());
    }
    #[test]
    fn returning_from_ground_or_input_reopens_the_connections_table_and_preserves_the_original_journal()
     {
        let mut app = loaded(include_bytes!(
            "../../../../tests/fixtures/diada-v320-derived/dyad-01/SCENE.DI"
        ));
        let original = app.document.as_ref().unwrap().current_journal().to_bytes();
        let mut proposed = app.document.as_ref().unwrap().loaded().mechanism.clone();
        proposed.pairs[0].name = Name(*b"Q ");
        app.open_structure();
        app.structure = Some(crate::structure_form::StructureForm::from_model(&proposed));
        app.apply_structure();
        app.submit(" ");
        assert!(app.structure.is_some());
        assert!(app.error.is_none(), "{:?}", app.error);
        let returned = app.draft.as_ref().unwrap().journal().to_bytes();
        app.apply_structure();
        assert!(app.structure.is_none());
        assert!(
            matches!(&app.state, Some(DialogueState::Question { question, .. }) if question.path == FieldPath::Ground)
        );
        assert!(
            app.draft
                .as_ref()
                .unwrap()
                .journal()
                .to_bytes()
                .starts_with(&returned)
        );
        app.submit("0");
        let before_blank = app.draft.as_ref().unwrap().journal().to_bytes();
        app.submit(" ");
        assert!(app.structure.is_some());
        app.cancel_structure();
        assert!(app.structure.is_none());
        assert!(
            matches!(&app.state, Some(DialogueState::Question { question, .. }) if question.path == FieldPath::Input(0))
        );
        assert_eq!(
            app.draft.as_ref().unwrap().journal().to_bytes(),
            before_blank
        );
        assert_eq!(
            app.document.as_ref().unwrap().current_journal().to_bytes(),
            original
        );
    }
    #[test]
    fn changing_a_force_between_zero_and_nonzero_asks_or_skips_the_original_angle_without_shifting_moments()
     {
        let mut app = loaded(include_bytes!(
            "../../../../tests/fixtures/diada-v320-edge-runtime/force-zero-external-seven/SCENE.DI"
        ));
        let original = app.document.as_ref().unwrap().current_journal().to_bytes();
        let force = app
            .document
            .as_ref()
            .unwrap()
            .bindings()
            .iter()
            .find_map(|binding| {
                if let FieldPath::ForceMagnitude(point) = binding.path {
                    Some(point)
                } else {
                    None
                }
            })
            .unwrap();
        let magnitude = FieldPath::ForceMagnitude(force);
        let angle = FieldPath::ForceAngle(force);
        app.start_edit(magnitude.clone());
        app.submit("5");
        assert!(app.error.is_none(), "{:?}", app.error);
        assert!(
            matches!(&app.state, Some(DialogueState::Question { question, .. }) if question.path == angle)
        );
        app.restore_remaining();
        assert!(
            matches!(&app.state, Some(DialogueState::Question { question, .. }) if question.path == angle)
        );
        assert_eq!(
            app.document.as_ref().unwrap().current_journal().to_bytes(),
            original
        );
        app.submit("37.5");
        app.restore_remaining();
        assert!(app.error.is_none(), "{:?}", app.error);
        assert!(app.draft.is_none());
        assert_eq!(
            app.document
                .as_ref()
                .unwrap()
                .answer_text(&angle)
                .as_deref(),
            Some("37.5")
        );
        app.start_edit(magnitude.clone());
        app.submit("0");
        assert!(app.error.is_none(), "{:?}", app.error);
        app.restore_remaining();
        assert!(app.error.is_none(), "{:?}", app.error);
        assert!(app.draft.is_none());
        let document = app.document.as_ref().unwrap();
        assert!(document.answer_text(&angle).is_none());
        let reopened =
            EditableDialogue::open(&document.current_journal().to_bytes(), "SCENE", &TpNumbers)
                .unwrap();
        assert_eq!(document.loaded().mechanism, reopened.loaded().mechanism);
        let pair = reopened.loaded().mechanism.points[force.0].pair;
        assert_eq!(
            reopened.loaded().mechanism.pairs[pair.0].state.ry,
            Real48::ZERO
        );
    }
    #[test]
    fn unchanged_connections_table_preserves_the_entire_original_journal() {
        let mut app = loaded(SCENE);
        let original = app.document.as_ref().unwrap().current_journal().to_bytes();
        app.start_edit(FieldPath::PairName(0));
        assert!(app.structure.is_some());
        assert!(app.edit.is_none());
        app.apply_structure();
        assert!(app.structure.is_none());
        assert!(app.draft.is_none());
        assert!(!app.dirty);
        assert_eq!(
            app.document.as_ref().unwrap().current_journal().to_bytes(),
            original
        );
    }
    fn paint_structure_preview(app: &mut Workbench) {
        let ctx = egui::Context::default();
        let raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(840.0, 560.0),
            )),
            ..Default::default()
        };
        let mut output = ctx.run_ui(raw, |ui| app.canvas(ui));
        output.textures_delta.clear();
    }
    fn highlighted_structure_names(app: &Workbench) -> (Vec<[u8; 2]>, Vec<[u8; 2]>) {
        let model = app.structure.as_ref().unwrap().preview();
        let mut pairs = Vec::new();
        let mut links = Vec::new();
        for key in &app.view.highlighted {
            match key {
                ObjectKey::Pair(id) => pairs.push(model.pairs[*id].name.0),
                ObjectKey::Link(id) => links.push(model.links[*id].name.0),
                ObjectKey::Point(_) => panic!("a structural row cannot select an attached point"),
            }
        }
        pairs.sort();
        links.sort();
        (pairs, links)
    }
    #[test]
    fn a_selected_connections_row_emphasizes_its_pair_and_both_links_without_accepting_data() {
        let mut app = loaded(SCENE);
        let original = app.document.as_ref().unwrap().current_journal().to_bytes();
        let old_selection = Some(ObjectKey::Link(5));
        app.view.selected = old_selection;
        app.open_structure();
        for (row, pair, links) in [
            (1, *b"A ", vec![*b"1 ", *b"2 "]),
            (3, *b"C ", vec![*b"0 ", *b"3 "]),
        ] {
            app.structure.as_mut().unwrap().select_row(row);
            paint_structure_preview(&mut app);
            assert_eq!(highlighted_structure_names(&app), (vec![pair], links));
            assert_eq!(app.view.selected, old_selection);
            assert_eq!(
                app.document.as_ref().unwrap().current_journal().to_bytes(),
                original
            );
        }
        app.structure.as_mut().unwrap().select_row(usize::MAX);
        paint_structure_preview(&mut app);
        assert!(app.view.highlighted.is_empty());
        app.cancel_structure();
        assert!(!app.dirty);
        assert_eq!(
            app.document.as_ref().unwrap().current_journal().to_bytes(),
            original
        );
    }
    #[test]
    fn shared_hinge_occurrences_highlight_together_with_only_the_selected_rows_two_links() {
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
        let mut form = crate::structure_form::StructureForm::from_model(&model);
        form.select_row(1);
        let mut app = Workbench::blank();
        app.structure = Some(form);
        paint_structure_preview(&mut app);
        assert_eq!(
            highlighted_structure_names(&app),
            (vec![*b"A ", *b"A "], vec![*b"0 ", *b"2 "])
        );
    }
    fn press_shortcut(app: &mut Workbench, key: egui::Key) {
        let ctx = egui::Context::default();
        let raw = egui::RawInput {
            events: vec![egui::Event::Key {
                key,
                physical_key: Some(key),
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
            ..Default::default()
        };
        let mut output = ctx.run_ui(raw, |ui| app.shortcuts(ui.ctx()));
        output.textures_delta.clear();
    }
    #[test]
    fn escape_from_unsaved_modal_preserves_the_proposed_connections_table() {
        for action in [Action::New, Action::Open, Action::Close] {
            let mut app = loaded(SCENE);
            let original = app.document.as_ref().unwrap().current_journal().to_bytes();
            app.open_structure();
            let mut proposed = app.document.as_ref().unwrap().loaded().mechanism.clone();
            proposed.pairs[0].name = Name([b'Q', b' ']);
            app.structure = Some(crate::structure_form::StructureForm::from_model(&proposed));
            app.request(action);
            press_shortcut(&mut app, egui::Key::Escape);
            assert!(app.pending.is_none());
            assert_eq!(
                app.structure.as_ref().unwrap().preview().pairs[0].name,
                proposed.pairs[0].name
            );
            assert!(app.dirty);
            assert_eq!(
                app.document.as_ref().unwrap().current_journal().to_bytes(),
                original
            );
        }
    }
    #[test]
    fn f10_cannot_complete_a_hidden_draft_behind_connections_or_the_save_modal() {
        let (mut app, distance, _) = zero_optional_angle();
        let original = app.document.as_ref().unwrap().current_journal().to_bytes();
        app.start_edit(distance);
        app.submit("0.25");
        app.submit("45");
        assert!(app.draft.is_some());
        let remaining = app.restore_answers.clone();
        app.open_structure();
        press_shortcut(&mut app, egui::Key::F10);
        assert!(app.draft.is_some());
        assert_eq!(app.restore_answers, remaining);
        assert_eq!(
            app.document.as_ref().unwrap().current_journal().to_bytes(),
            original
        );
        app.cancel_structure();
        app.request(Action::Close);
        press_shortcut(&mut app, egui::Key::F10);
        assert!(app.pending.is_some());
        assert!(app.draft.is_some());
        assert_eq!(app.restore_answers, remaining);
        assert_eq!(
            app.document.as_ref().unwrap().current_journal().to_bytes(),
            original
        );
        press_shortcut(&mut app, egui::Key::Escape);
        app.restore_remaining();
        assert!(app.draft.is_none());
        assert_ne!(
            app.document.as_ref().unwrap().current_journal().to_bytes(),
            original
        );
    }
    #[test]
    fn changed_connections_table_keeps_accepted_data_until_completion_and_cancel_restores_it() {
        let mut app = loaded(SCENE);
        let original = app.document.as_ref().unwrap().current_journal().to_bytes();
        app.open_structure();
        let mut proposed = app.document.as_ref().unwrap().loaded().mechanism.clone();
        proposed.pairs[0].name = Name([b'Q', b' ']);
        app.structure = Some(crate::structure_form::StructureForm::from_model(&proposed));
        app.apply_structure();
        assert!(app.structure.is_none());
        assert!(app.draft.is_some());
        assert!(
            matches!(&app.state,Some(DialogueState::Question{question,..}) if question.path==FieldPath::Ground)
        );
        assert_eq!(
            app.document.as_ref().unwrap().current_journal().to_bytes(),
            original
        );
        assert!(!app.save(false));
        app.cancel_draft();
        assert!(!app.dirty);
        assert_eq!(
            app.document.as_ref().unwrap().current_journal().to_bytes(),
            original
        );
    }
    #[test]
    fn batch_connections_then_original_questions_produce_a_complete_reopenable_edited_mechanism() {
        let mut app = loaded(include_bytes!(
            "../../../../tests/fixtures/diada-v320-derived/dyad-01/SCENE.DI"
        ));
        let original = app.document.as_ref().unwrap().clone();
        app.open_structure();
        let mut proposed = original.loaded().mechanism.clone();
        proposed.pairs[0].name = Name([b'Q', b' ']);
        app.structure = Some(crate::structure_form::StructureForm::from_model(&proposed));
        app.apply_structure();
        let after = original
            .bindings()
            .iter()
            .position(|b| b.path == FieldPath::ConfirmPairs)
            .unwrap()
            + 1;
        for binding in &original.bindings()[after..] {
            assert!(
                matches!(&app.state,Some(DialogueState::Question{question,..})if question.path==binding.path),
                "{:?} {:?}",
                binding.path,
                app.state.as_ref().and_then(|s| {
                    if let DialogueState::Question { question, .. } = s {
                        Some(&question.path)
                    } else {
                        None
                    }
                })
            );
            app.submit(&binding.answer_text(original.current_journal()));
            assert!(app.error.is_none(), "{:?}", app.error);
        }
        assert!(app.draft.is_none());
        let doc = app.document.as_ref().unwrap();
        let reopened =
            EditableDialogue::open(&doc.current_journal().to_bytes(), "SCENE", &TpNumbers).unwrap();
        assert_eq!(doc.loaded().mechanism, reopened.loaded().mechanism);
        assert_eq!(doc.loaded().mechanism.pairs[0].name, Name([b'Q', b' ']));
        app.undo();
        assert_eq!(
            app.document.as_ref().unwrap().current_journal().to_bytes(),
            original.current_journal().to_bytes()
        );
    }
    #[test]
    fn accepted_numeric_edit_preserves_raw_units_and_undo_redo_restores_whole_document() {
        let mut app = loaded(SCENE);
        let path = app
            .document
            .as_ref()
            .unwrap()
            .bindings()
            .iter()
            .find(|b| matches!(b.path, FieldPath::GroundAngle(_)))
            .unwrap()
            .path
            .clone();
        let before = app.document.as_ref().unwrap().current_journal().to_bytes();
        app.start_edit(path.clone());
        app.submit("33.3333333");
        assert!(app.error.is_none(), "{:?}", app.error);
        assert_eq!(
            app.document.as_ref().unwrap().answer_text(&path).unwrap(),
            "33.3333333"
        );
        let changed = app.document.as_ref().unwrap().current_journal().to_bytes();
        assert_ne!(before, changed);
        app.undo();
        assert_eq!(
            app.document.as_ref().unwrap().current_journal().to_bytes(),
            before
        );
        app.redo();
        assert_eq!(
            app.document.as_ref().unwrap().current_journal().to_bytes(),
            changed
        );
        assert_eq!(
            app.fields.len(),
            app.document
                .as_ref()
                .unwrap()
                .loaded()
                .mechanism
                .inputs
                .len()
        );
    }
    fn zero_optional_angle() -> (Workbench, FieldPath, FieldPath) {
        let original = EditableDialogue::open(FORCE, "SCENE", &TpNumbers).unwrap();
        let distance = original
            .bindings()
            .iter()
            .find(|b| matches!(b.path, FieldPath::PointDistance(_)))
            .unwrap();
        let FieldPath::PointDistance(id) = distance.path else {
            unreachable!()
        };
        let angle = original
            .bindings()
            .iter()
            .find(|b| b.path == FieldPath::PointAngle(id))
            .unwrap();
        let mut bytes = Vec::new();
        for (index, line) in original.current_journal().lines().iter().enumerate() {
            if index + 1 == angle.start_line {
                continue;
            }
            bytes.extend_from_slice(if index + 1 == distance.start_line {
                b"0"
            } else {
                &line.bytes
            });
            bytes.extend_from_slice(b"\r\n");
        }
        (loaded(&bytes), distance.path.clone(), angle.path.clone())
    }
    #[test]
    fn dependent_edit_asks_new_angle_and_cancel_preserves_the_accepted_file_and_dirty_state() {
        let (mut app, distance, angle) = zero_optional_angle();
        let original = app.document.as_ref().unwrap().current_journal().to_bytes();
        app.start_edit(distance.clone());
        app.submit("0.25");
        assert!(app.draft.is_some());
        assert!(app.edit.is_none());
        assert!(
            matches!(&app.state,Some(DialogueState::Question{question,..}) if question.path==angle)
        );
        assert_eq!(
            app.document.as_ref().unwrap().current_journal().to_bytes(),
            original
        );
        app.restore_remaining();
        assert!(
            matches!(&app.state,Some(DialogueState::Question{question,..}) if question.path==angle),
            "new angle must never consume old next point distance"
        );
        app.redo();
        assert!(app.draft.is_some());
        app.cancel_draft();
        assert!(!app.dirty);
        assert!(app.draft.is_none());
        assert_eq!(
            app.document.as_ref().unwrap().current_journal().to_bytes(),
            original
        );
    }
    #[test]
    fn answering_a_new_dependent_field_then_f10_reuses_only_the_matching_remaining_answers() {
        let (mut app, distance, angle) = zero_optional_angle();
        app.start_edit(distance.clone());
        app.submit("0.25");
        app.submit("45");
        app.restore_remaining();
        assert!(app.error.is_none(), "{:?}", app.error);
        assert!(app.draft.is_none(), "{:?}", app.state);
        let doc = app.document.as_ref().unwrap();
        assert_eq!(doc.answer_text(&distance).unwrap(), "0.25");
        assert_eq!(doc.answer_text(&angle).unwrap(), "45");
        let bytes = doc.current_journal().to_bytes();
        let reopened = EditableDialogue::open(&bytes, "SCENE", &TpNumbers).unwrap();
        assert_eq!(doc.loaded().mechanism, reopened.loaded().mechanism);
        app.undo();
        assert_eq!(
            app.document
                .as_ref()
                .unwrap()
                .answer_text(&distance)
                .unwrap(),
            "0"
        );
    }
}
