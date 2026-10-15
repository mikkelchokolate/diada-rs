//! Clearing a visual selection must never accept or rewrite mechanism data.
use super::*;

const SCENE: &[u8] = include_bytes!("../../../../tests/fixtures/diada-v320-derived/SCENE.DI");

fn loaded(context: &egui::Context) -> Workbench {
    let mut app = Workbench::new(&eframe::CreationContext::_new_kittest(context.clone()));
    app.reset();
    let document = EditableDialogue::open(SCENE, "SELECTION", &TpNumbers).unwrap();
    app.fields = InputFields::for_model(&document.loaded().mechanism);
    app.document = Some(document);
    app
}

fn paint(
    context: &egui::Context,
    app: &mut Workbench,
    events: Vec<egui::Event>,
) -> egui::FullOutput {
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1320.0, 840.0),
        )),
        events,
        ..Default::default()
    };
    let mut frame = eframe::Frame::_new_kittest();
    let mut output = context.run_ui(input, |ui| eframe::App::ui(app, ui, &mut frame));
    output.textures_delta.clear();
    output
}

fn click(context: &egui::Context, app: &mut Workbench, point: egui::Pos2) -> egui::FullOutput {
    for pressed in [true, false] {
        paint(
            context,
            app,
            vec![
                egui::Event::PointerMoved(point),
                egui::Event::PointerButton {
                    pos: point,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
    }
    paint(context, app, vec![]);
    paint(context, app, vec![])
}

fn escape(context: &egui::Context, app: &mut Workbench) {
    for pressed in [true, false] {
        paint(
            context,
            app,
            vec![egui::Event::Key {
                key: egui::Key::Escape,
                physical_key: Some(egui::Key::Escape),
                pressed,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
    }
    paint(context, app, vec![]);
}

fn type_answer(context: &egui::Context, app: &mut Workbench, answer: &str) {
    let control = egui::Modifiers {
        ctrl: true,
        command: true,
        ..Default::default()
    };
    paint(
        context,
        app,
        vec![
            egui::Event::ModifiersChanged(control),
            egui::Event::Key {
                key: egui::Key::A,
                physical_key: Some(egui::Key::A),
                pressed: true,
                repeat: false,
                modifiers: control,
            },
            egui::Event::Key {
                key: egui::Key::A,
                physical_key: Some(egui::Key::A),
                pressed: false,
                repeat: false,
                modifiers: control,
            },
        ],
    );
    paint(
        context,
        app,
        vec![
            egui::Event::ModifiersChanged(egui::Modifiers::NONE),
            egui::Event::Text(answer.into()),
        ],
    );
    paint(context, app, vec![]);
    assert_eq!(app.answer, answer);
}

fn text_center(output: &egui::FullOutput, label: &str) -> egui::Pos2 {
    let positions: Vec<_> = output
        .shapes
        .iter()
        .filter_map(|paint| match &paint.shape {
            egui::Shape::Text(text) if text.galley.text() == label => {
                Some(text.pos + text.galley.size() * 0.5)
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        positions.len(),
        1,
        "One actual table row must display {label:?}"
    );
    positions[0]
}

fn hinges(output: &egui::FullOutput) -> Vec<egui::Pos2> {
    let positions: Vec<_> = output
        .shapes
        .iter()
        .filter_map(|paint| match &paint.shape {
            egui::Shape::Circle(circle)
                if (circle.radius - 4.5).abs() < 0.001
                    && paint.clip_rect.contains(circle.center) =>
            {
                Some(circle.center)
            }
            _ => None,
        })
        .collect();
    assert!(positions.len() >= 3, "The actual mechanism must be painted");
    positions
}

fn canvas_blank(output: &egui::FullOutput) -> egui::Pos2 {
    let canvas = output
        .shapes
        .iter()
        .filter_map(|paint| match &paint.shape {
            egui::Shape::Rect(shape)
                if shape.rect.width() > 500.0
                    && shape.rect.height() > 200.0
                    && shape.corner_radius == egui::CornerRadius::same(4) =>
            {
                Some(shape.rect)
            }
            _ => None,
        })
        .max_by(|a, b| a.area().total_cmp(&b.area()))
        .expect("The actual rounded mechanism canvas must exist");
    canvas.left_top() + egui::vec2(15.0, 15.0)
}

fn table_blank(context: &egui::Context) -> egui::Pos2 {
    let panel = egui::PanelState::load(context, egui::Id::new("parameters_panel"))
        .expect("Actual parameters panel")
        .outer_rect;
    panel.right_bottom() - egui::vec2(20.0, 20.0)
}

#[derive(Debug, PartialEq)]
struct Accepted {
    document: Option<Vec<u8>>,
    model: Option<Mechanism>,
    draft: Option<Vec<u8>>,
    draft_model: Option<Mechanism>,
    dirty: bool,
    undo: Vec<Vec<u8>>,
    redo: Vec<Vec<u8>>,
    draft_undo: usize,
    fields: Vec<InputFields>,
}
impl Accepted {
    fn read(app: &Workbench) -> Self {
        Self {
            document: app
                .document
                .as_ref()
                .map(|d| d.current_journal().to_bytes()),
            model: app.document.as_ref().map(|d| d.loaded().mechanism.clone()),
            draft: app.draft.as_ref().map(|d| d.journal().to_bytes()),
            draft_model: app.draft.as_ref().and_then(|d| d.preview().cloned()),
            dirty: app.dirty,
            undo: app
                .undo
                .iter()
                .map(|d| d.current_journal().to_bytes())
                .collect(),
            redo: app
                .redo
                .iter()
                .map(|d| d.current_journal().to_bytes())
                .collect(),
            draft_undo: app.draft_undo.len(),
            fields: app.fields.clone(),
        }
    }
    fn assert_unchanged(&self, app: &Workbench) {
        assert_eq!(
            Self::read(app),
            *self,
            "Unselect cannot accept data or create an undo entry"
        );
    }
}

#[test]
fn actual_canvas_pair_and_link_selection_clear_on_empty_click_or_escape() {
    for pair in [false, true] {
        for use_escape in [false, true] {
            let context = egui::Context::default();
            let mut app = loaded(&context);
            paint(&context, &mut app, vec![]);
            let before = paint(&context, &mut app, vec![]);
            let accepted = Accepted::read(&app);
            let points = hinges(&before);
            let at = if pair {
                points[0]
            } else {
                points[0].lerp(points[1], 0.5)
            };
            let selected = click(&context, &mut app, at);
            assert_eq!(
                app.view.selected,
                Some(if pair {
                    ObjectKey::Pair(0)
                } else {
                    ObjectKey::Link(1)
                })
            );
            accepted.assert_unchanged(&app);
            if use_escape {
                escape(&context, &mut app);
            } else {
                click(&context, &mut app, canvas_blank(&selected));
            }
            assert_eq!(app.view.selected, None);
            assert!(app.edit.is_none());
            accepted.assert_unchanged(&app);
        }
    }
}

#[test]
fn selecting_a_dimension_then_clearing_discards_only_the_unaccepted_editor() {
    for modified in [false, true] {
        for gesture in 0..3 {
            let context = egui::Context::default();
            let mut app = loaded(&context);
            paint(&context, &mut app, vec![]);
            let before = paint(&context, &mut app, vec![]);
            let accepted = Accepted::read(&app);
            let path = FieldPath::GroundX(PairId(0));
            let label = fields::describe(&path, app.model().unwrap()).0;
            click(&context, &mut app, text_center(&before, &label));
            assert!(app.edit.as_ref().is_some_and(|edit| edit.path == path));
            assert_eq!(app.view.selected, Some(ObjectKey::Pair(0)));
            if modified {
                type_answer(&context, &mut app, "0.125");
            }
            accepted.assert_unchanged(&app);
            match gesture {
                0 => {
                    let output = paint(&context, &mut app, vec![]);
                    click(&context, &mut app, canvas_blank(&output));
                }
                1 => {
                    click(&context, &mut app, table_blank(&context));
                }
                _ => escape(&context, &mut app),
            }
            assert!(
                app.edit.is_none(),
                "Gesture {gesture} must cancel the unaccepted dimension editor"
            );
            assert_eq!(app.view.selected, None);
            assert!(app.view.highlighted.is_empty());
            accepted.assert_unchanged(&app);
        }
    }
}

#[test]
fn structure_table_pair_selection_can_be_cleared_without_editing_structure() {
    for use_escape in [false, true] {
        let context = egui::Context::default();
        let mut app = loaded(&context);
        app.section = "Структура";
        paint(&context, &mut app, vec![]);
        let before = paint(&context, &mut app, vec![]);
        let panel = egui::PanelState::load(&context, egui::Id::new("parameters_panel"))
            .unwrap()
            .outer_rect;
        let pair_name = before
            .shapes
            .iter()
            .find_map(|paint| match &paint.shape {
                egui::Shape::Text(text) if text.galley.text() == "O" => {
                    let point = text.pos + text.galley.size() * 0.5;
                    panel.contains(point).then_some(point)
                }
                _ => None,
            })
            .expect("Pair O must have a real structure table row");
        let accepted = Accepted::read(&app);
        click(&context, &mut app, pair_name);
        assert_eq!(app.view.selected, Some(ObjectKey::Pair(0)));
        assert!(app.edit.is_none());
        assert!(app.structure.is_none());
        if use_escape {
            escape(&context, &mut app);
        } else {
            click(&context, &mut app, table_blank(&context));
        }
        assert_eq!(app.view.selected, None);
        assert!(app.structure.is_none());
        accepted.assert_unchanged(&app);
    }
}

#[test]
fn dragging_empty_canvas_pans_without_clearing_the_selected_object() {
    let context = egui::Context::default();
    let mut app = loaded(&context);
    paint(&context, &mut app, vec![]);
    let before = paint(&context, &mut app, vec![]);
    let selected = click(&context, &mut app, hinges(&before)[0]);
    let accepted = Accepted::read(&app);
    let start = canvas_blank(&selected);
    let pan = app.view.pan;
    paint(
        &context,
        &mut app,
        vec![
            egui::Event::PointerMoved(start),
            egui::Event::PointerButton {
                pos: start,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            },
        ],
    );
    let end = start + egui::vec2(35.0, 25.0);
    paint(&context, &mut app, vec![egui::Event::PointerMoved(end)]);
    paint(
        &context,
        &mut app,
        vec![egui::Event::PointerButton {
            pos: end,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        }],
    );
    paint(&context, &mut app, vec![]);
    assert_ne!(app.view.pan, pan);
    assert_eq!(
        app.view.selected,
        Some(ObjectKey::Pair(0)),
        "A pan gesture is not a blank click"
    );
    accepted.assert_unchanged(&app);
}

#[test]
fn clearing_manual_selection_preserves_the_required_draft_question_and_answers() {
    for use_escape in [false, true] {
        let context = egui::Context::default();
        let mut app = loaded(&context);
        let path = FieldPath::LinkParameter(LinkId(2));
        let mut draft = app.document.as_ref().unwrap().begin_edit(&path).unwrap();
        let state = draft.next(&TpNumbers).unwrap();
        app.document = None;
        app.draft = Some(draft);
        app.accept(state);
        paint(&context, &mut app, vec![]);
        let before = paint(&context, &mut app, vec![]);
        let accepted = Accepted::read(&app);
        let selected = click(&context, &mut app, hinges(&before)[0]);
        assert_eq!(app.view.selected, Some(ObjectKey::Pair(0)));
        if use_escape {
            escape(&context, &mut app);
        } else {
            click(&context, &mut app, canvas_blank(&selected));
        }
        assert_eq!(app.view.selected, None);
        assert!(
            matches!(&app.state, Some(DialogueState::Question { question, .. }) if question.path == path)
        );
        assert!(app.draft.is_some());
        accepted.assert_unchanged(&app);
    }
}
