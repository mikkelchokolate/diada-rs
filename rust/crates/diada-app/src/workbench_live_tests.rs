//! Real app input and paint checks for disposable question previews.
use super::*;

const SCENE: &[u8] = include_bytes!("../../../../tests/fixtures/diada-v320-derived/SCENE.DI");

fn themed_app(context: &egui::Context) -> Workbench {
    let mut app = Workbench::new(&eframe::CreationContext::_new_kittest(context.clone()));
    app.reset();
    app
}

fn loaded_app(context: &egui::Context) -> Workbench {
    let mut app = themed_app(context);
    let document = EditableDialogue::open(SCENE, "SCENE", &TpNumbers).unwrap();
    app.fields = InputFields::for_model(&document.loaded().mechanism);
    app.document = Some(document);
    app
}

fn draft_at(context: &egui::Context, path: FieldPath) -> Workbench {
    let original = EditableDialogue::open(SCENE, "SCENE", &TpNumbers).unwrap();
    let mut draft = original.begin_edit(&path).unwrap();
    let state = draft.next(&TpNumbers).unwrap();
    assert!(matches!(&state, DialogueState::Question { question, .. } if question.path == path));
    let mut app = themed_app(context);
    app.draft = Some(draft);
    app.accept(state);
    app
}

fn paint(
    context: &egui::Context,
    app: &mut Workbench,
    events: Vec<egui::Event>,
) -> egui::FullOutput {
    let raw = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1320.0, 840.0),
        )),
        events,
        ..Default::default()
    };
    let mut frame = eframe::Frame::_new_kittest();
    let mut output = context.run_ui(raw, |ui| eframe::App::ui(app, ui, &mut frame));
    output.textures_delta.clear();
    output
}

fn texts(output: &egui::FullOutput) -> Vec<String> {
    output
        .shapes
        .iter()
        .filter_map(|paint| match &paint.shape {
            egui::Shape::Text(text) => Some(text.galley.text().to_owned()),
            _ => None,
        })
        .collect()
}

fn text_center(output: &egui::FullOutput, value: &str) -> egui::Pos2 {
    let centers: Vec<_> = output
        .shapes
        .iter()
        .filter_map(|paint| match &paint.shape {
            egui::Shape::Text(text) if text.galley.text() == value => {
                Some(text.pos + text.galley.size() * 0.5)
            }
            _ => None,
        })
        .collect();
    assert_eq!(centers.len(), 1, "One real widget must paint {value:?}.");
    centers[0]
}

fn hinge_centers(output: &egui::FullOutput) -> Vec<egui::Pos2> {
    let centers: Vec<_> = output
        .shapes
        .iter()
        .filter_map(|paint| match &paint.shape {
            egui::Shape::Circle(circle) if (circle.radius - 4.5).abs() < 0.001 => {
                Some(circle.center)
            }
            _ => None,
        })
        .collect();
    assert!(
        centers.len() >= 3,
        "The actual canvas must paint mechanism hinges."
    );
    centers
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
    paint(context, app, vec![])
}

fn replace_focused_answer(
    context: &egui::Context,
    app: &mut Workbench,
    value: &str,
) -> egui::FullOutput {
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
            egui::Event::Text(value.to_owned()),
        ],
    );
    assert_eq!(
        app.answer, value,
        "The real focused text field must receive typing."
    );
    paint(context, app, vec![])
}

#[derive(Debug, PartialEq)]
struct AcceptedData {
    document: Option<Vec<u8>>,
    draft: Option<Vec<u8>>,
    document_model: Option<Mechanism>,
    draft_model: Option<Mechanism>,
    dirty: bool,
    undo: usize,
    redo: usize,
    draft_undo: usize,
}

impl AcceptedData {
    fn read(app: &Workbench) -> Self {
        Self {
            document: app
                .document
                .as_ref()
                .map(|d| d.current_journal().to_bytes()),
            draft: app.draft.as_ref().map(|d| d.journal().to_bytes()),
            document_model: app.document.as_ref().map(|d| d.loaded().mechanism.clone()),
            draft_model: app.draft.as_ref().and_then(|d| d.preview().cloned()),
            dirty: app.dirty,
            undo: app.undo.len(),
            redo: app.redo.len(),
            draft_undo: app.draft_undo.len(),
        }
    }
    fn assert_unchanged(&self, app: &Workbench) {
        assert_eq!(
            Self::read(app),
            *self,
            "A preview cannot accept answers or dirty history."
        );
    }
}

#[test]
fn actual_draft_typing_moves_body_before_accept_and_retains_it_at_the_next_question() {
    let context = egui::Context::default();
    let mut app = draft_at(&context, FieldPath::LinkParameter(LinkId(1)));
    paint(&context, &mut app, vec![]);
    let before = paint(&context, &mut app, vec![]);
    let accepted = AcceptedData::read(&app);
    let proposed = replace_focused_answer(&context, &mut app, "0.8");
    assert_ne!(hinge_centers(&before), hinge_centers(&proposed));
    accepted.assert_unchanged(&app);
    let preview = app.draft_projection().unwrap().0.display_model;
    let a = preview.pair_by_name(Name(*b"A ")).unwrap();
    let proposed_a = preview.pairs[a.0].state;
    let button = text_center(&proposed, "Принять");
    let next_paint = click(&context, &mut app, button);
    assert!(
        matches!(&app.state, Some(DialogueState::Question { question, .. }) if question.path == FieldPath::LinkParameter(LinkId(2)))
    );
    assert_eq!(app.draft_undo.len(), 1);
    assert!(app.dirty);
    let retained = app.draft_projection().unwrap().0.display_model;
    assert_eq!(retained.pairs[a.0].state.x, proposed_a.x);
    assert_eq!(retained.pairs[a.0].state.y, proposed_a.y);
    // O and H are fixed ground hinges; the ratio is independent of automatic
    // fitting and the next question's different dimension annotations.
    let ground_ratio = |output: &egui::FullOutput| {
        let centers = hinge_centers(output);
        centers[0].distance(centers[1]) / centers[0].distance(*centers.last().unwrap())
    };
    assert!((ground_ratio(&proposed) - ground_ratio(&next_paint)).abs() < 0.0001);
    assert_eq!(
        app.draft
            .as_ref()
            .unwrap()
            .bindings()
            .iter()
            .filter(|b| b.path == FieldPath::LinkParameter(LinkId(1)))
            .count(),
        1
    );
}

#[test]
fn actual_yes_no_hover_changes_only_the_temporary_canvas_and_click_accepts_once() {
    let context = egui::Context::default();
    let mut app = draft_at(&context, FieldPath::LinkOnGuide(LinkId(3)));
    paint(&context, &mut app, vec![]);
    let baseline = paint(&context, &mut app, vec![]);
    let yes = text_center(&baseline, "Да");
    let no = text_center(&baseline, "Нет");
    let accepted = AcceptedData::read(&app);
    let hovered_yes = paint(&context, &mut app, vec![egui::Event::PointerMoved(yes)]);
    assert_eq!(app.hover_answer, Some("Y"));
    assert!(
        texts(&hovered_yes)
            .iter()
            .any(|t| t.starts_with("После ответа «Да»:"))
    );
    accepted.assert_unchanged(&app);
    let hovered_no = paint(&context, &mut app, vec![egui::Event::PointerMoved(no)]);
    assert_eq!(app.hover_answer, Some("N"));
    assert!(
        texts(&hovered_no)
            .iter()
            .any(|t| t.starts_with("После ответа «Нет»:"))
    );
    assert_ne!(hinge_centers(&hovered_yes), hinge_centers(&hovered_no));
    accepted.assert_unchanged(&app);
    let outside = paint(
        &context,
        &mut app,
        vec![egui::Event::PointerMoved(egui::pos2(5.0, 5.0))],
    );
    assert_eq!(app.hover_answer, None);
    assert_eq!(hinge_centers(&baseline), hinge_centers(&outside));
    assert!(
        !texts(&outside)
            .iter()
            .any(|t| t.starts_with("После ответа «"))
    );
    accepted.assert_unchanged(&app);
    click(&context, &mut app, yes);
    assert_eq!(app.draft_undo.len(), 1);
    assert_eq!(
        app.draft
            .as_ref()
            .unwrap()
            .bindings()
            .iter()
            .filter(|b| b.path == FieldPath::LinkOnGuide(LinkId(3)))
            .count(),
        1
    );
    assert!(
        matches!(&app.state, Some(DialogueState::Question { question, .. }) if question.path == FieldPath::LinkOnGuide(LinkId(4)))
    );
}

#[test]
fn actual_complete_document_edit_paints_old_to_new_dimensions_and_discard_keeps_raw_units() {
    for (angle, proposed, old_display) in [(false, "1.55", "1.5"), (true, "85", "89.999987770454")]
    {
        let context = egui::Context::default();
        let mut app = loaded_app(&context);
        let path = if angle {
            app.document
                .as_ref()
                .unwrap()
                .bindings()
                .iter()
                .find_map(|b| matches!(b.path, FieldPath::PointAngle(_)).then(|| b.path.clone()))
                .unwrap()
        } else {
            FieldPath::LinkParameter(LinkId(2))
        };
        let accepted = AcceptedData::read(&app);
        app.start_edit(path);
        let unit = app.edit.as_ref().unwrap().question.units.unwrap();
        assert_eq!(unit, if angle { "град" } else { "м" });
        paint(&context, &mut app, vec![]);
        let before = paint(&context, &mut app, vec![]);
        let candidate = replace_focused_answer(&context, &mut app, proposed);
        let comparison = format!("{old_display} → {proposed} {unit}");
        assert!(
            texts(&candidate).iter().any(|t| t.contains(&comparison)),
            "Actual canvas must show original input units: {comparison:?}"
        );
        assert_ne!(
            hinge_centers(&before),
            hinge_centers(&candidate),
            "Length/angle typing must move the actual body, not only its caption."
        );
        accepted.assert_unchanged(&app);
        let cancel = text_center(&candidate, "Отменить изменение");
        let discarded = click(&context, &mut app, cancel);
        assert!(app.edit.is_none());
        accepted.assert_unchanged(&app);
        assert_eq!(hinge_centers(&before), hinge_centers(&discarded));
    }
}

#[test]
fn the_actual_question_names_b_link_three_and_c_after_primary_pair_reordering() {
    let context = egui::Context::default();
    let mut app = draft_at(&context, FieldPath::LinkOnGuide(LinkId(3)));
    let link = &app.model().unwrap().links[3];
    assert_eq!(
        app.model().unwrap().pairs[link.pairs[0].unwrap().0].kind,
        PairKind::Prismatic
    );
    let output = paint(&context, &mut app, vec![]);
    assert!(
        texts(&output)
            .iter()
            .any(|t| t == "Шарнир B звена 3 лежит на направляющей ползуна C?")
    );
}
