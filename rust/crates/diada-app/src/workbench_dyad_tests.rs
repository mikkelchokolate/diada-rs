//! The real question controls explain rejected dyad answers without changing
//! the original controller's order, accepted geometry, or answer journal.
use super::*;

const BASIC: &[u8] =
    include_bytes!("../../../../tests/fixtures/diada-v320-derived/dyad-01/SCENE.DI");

fn user_controller() -> DialogueController {
    let mut model = Mechanism {
        title: "DYAD FEEDBACK".into(),
        ..Default::default()
    };
    for (pair, kind, a, b) in [
        (b'O', PairKind::Revolute, b'0', b'1'),
        (b'A', PairKind::Revolute, b'1', b'2'),
        (b'B', PairKind::Revolute, b'2', b'3'),
        (b'C', PairKind::Prismatic, b'3', b'0'),
        (b'D', PairKind::Revolute, b'2', b'4'),
        (b'E', PairKind::Revolute, b'4', b'5'),
        (b'F', PairKind::Prismatic, b'5', b'0'),
    ] {
        let a = model.add_link(Name([a, b' ']));
        let b = model.add_link(Name([b, b' ']));
        model.add_pair(kind, Name([pair, b' ']), [a, b]).unwrap();
    }
    let (mut controller, state) = crate::structure_form::StructureForm::from_model(&model)
        .commit()
        .unwrap();
    assert!(
        matches!(state, DialogueState::Question { question, .. } if question.path == FieldPath::Ground)
    );
    for answer in ["0", "1", "N", "N"] {
        controller.answer(answer, &TpNumbers).unwrap();
    }
    assert_eq!(
        controller.question().unwrap().path,
        FieldPath::GroupLink(0, 0)
    );
    controller
}

fn app_with_draft(context: &egui::Context, mut controller: DialogueController) -> Workbench {
    let mut app = Workbench::new(&eframe::CreationContext::_new_kittest(context.clone()));
    app.reset();
    let state = controller.next(&TpNumbers).unwrap();
    app.draft = Some(controller);
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
        .filter_map(|shape| match &shape.shape {
            egui::Shape::Text(text) => Some(text.galley.text().to_owned()),
            _ => None,
        })
        .collect()
}

fn text_center(output: &egui::FullOutput, value: &str) -> egui::Pos2 {
    let points: Vec<_> = output
        .shapes
        .iter()
        .filter_map(|shape| match &shape.shape {
            egui::Shape::Text(text) if text.galley.text() == value => {
                Some(text.pos + text.galley.size() * 0.5)
            }
            _ => None,
        })
        .collect();
    assert_eq!(points.len(), 1, "One actual control must paint {value:?}.");
    points[0]
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

fn type_answer(context: &egui::Context, app: &mut Workbench, value: &str) -> egui::FullOutput {
    // A preceding mouse click may own focus; each next question requests the
    // answer edit's focus exactly as it does in the native app.
    paint(context, app, vec![]);
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
            egui::Event::Text(value.into()),
        ],
    );
    assert_eq!(app.answer, value, "The focused edit must receive typing.");
    paint(context, app, vec![])
}

fn enter(context: &egui::Context, app: &mut Workbench) -> egui::FullOutput {
    for pressed in [true, false] {
        paint(
            context,
            app,
            vec![egui::Event::Key {
                key: egui::Key::Enter,
                physical_key: Some(egui::Key::Enter),
                pressed,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
    }
    paint(context, app, vec![])
}

fn question_is(app: &Workbench, expected: FieldPath) {
    assert!(
        matches!(&app.state, Some(DialogueState::Question { question, .. }) if question.path == expected),
        "Expected {expected:?}, actual state {:?}.",
        app.state
    );
}

fn agrees_with_controller(app: &Workbench, controller: &DialogueController) {
    let actual = app.draft.as_ref().unwrap();
    assert_eq!(actual.journal().to_bytes(), controller.journal().to_bytes());
    assert_eq!(actual.preview(), controller.preview());
    assert_eq!(actual.question(), controller.question());
    assert!(app.document.is_none());
}

#[test]
fn keyboard_rejection_explains_unknown_d_and_mouse_retry_preserves_original_order_and_journal() {
    let context = egui::Context::default();
    let mut controller = user_controller();
    let mut app = app_with_draft(&context, controller.clone());
    paint(&context, &mut app, vec![]);
    assert!(app.error.is_none());
    type_answer(&context, &mut app, "4");
    enter(&context, &mut app);
    controller.answer("4", &TpNumbers).unwrap();
    agrees_with_controller(&app, &controller);
    question_is(&app, FieldPath::GroupLink(0, 1));
    assert!(app.error.is_none());
    let accepted_journal = controller.journal().to_bytes();
    let proposed = type_answer(&context, &mut app, "5");
    assert!(app.error.is_none(), "Typing is not an answer submission.");
    assert!(
        texts(&proposed)
            .iter()
            .all(|text| !text.contains("Порядок анализа задан неверно"))
    );
    agrees_with_controller(&app, &controller);
    assert_eq!(
        app.draft.as_ref().unwrap().journal().to_bytes(),
        accepted_journal
    );
    let rejected = enter(&context, &mut app);
    controller.answer("5", &TpNumbers).unwrap();
    agrees_with_controller(&app, &controller);
    question_is(&app, FieldPath::GroupLink(0, 0));
    let reason = app.error.as_ref().unwrap();
    assert!(reason.contains("Порядок анализа задан неверно"), "{reason}");
    assert!(reason.contains("пары D:"), "{reason}");
    assert!(texts(&rejected).iter().any(|text| text == reason));
    assert!(
        app.draft
            .as_ref()
            .unwrap()
            .preview()
            .unwrap()
            .groups
            .iter()
            .all(|group| group.kind != GroupKind::Dyad)
    );

    // Retrying through the real mouse button must clear the stale reason and
    // accept the same 2-3, then 4-5 sequence as the unchanged controller.
    for (answer, expected) in [
        ("2", FieldPath::GroupLink(0, 1)),
        ("3", FieldPath::GroupLink(1, 0)),
        ("4", FieldPath::GroupLink(1, 1)),
        ("5", FieldPath::ConfirmGroups),
    ] {
        let proposed = type_answer(&context, &mut app, answer);
        let button = text_center(&proposed, "Принять");
        let output = click(&context, &mut app, button);
        controller.answer(answer, &TpNumbers).unwrap();
        agrees_with_controller(&app, &controller);
        question_is(&app, expected);
        assert!(app.error.is_none(), "{:?}", app.error);
        assert!(
            texts(&output)
                .iter()
                .all(|text| !text.contains("Порядок анализа задан неверно"))
        );
    }
    assert_eq!(
        app.draft_undo.len(),
        6,
        "Each real answer is accepted once."
    );
}

#[test]
fn mouse_retries_name_roles_and_duplicates_only_show_the_submitted_reason() {
    let context = egui::Context::default();
    let mut controller = user_controller();
    let mut app = app_with_draft(&context, controller.clone());
    paint(&context, &mut app, vec![]);
    for (first, second, expected) in [
        ("Z", "3", "Звено Z не найдено"),
        ("2", "Z", "Звено Z не найдено"),
        ("1", "2", "Звено 1 — ведущее"),
        ("2", "0", "Звено 0 — стойка"),
        ("2", "2", "Звено 2 указано дважды"),
        ("2", "5", "ровно одной общей кинематической парой"),
    ] {
        question_is(&app, FieldPath::GroupLink(0, 0));
        type_answer(&context, &mut app, first);
        enter(&context, &mut app);
        controller.answer(first, &TpNumbers).unwrap();
        agrees_with_controller(&app, &controller);
        question_is(&app, FieldPath::GroupLink(0, 1));
        assert!(
            app.error.is_none(),
            "A first link alone cannot reject a dyad."
        );
        let proposed = type_answer(&context, &mut app, second);
        assert!(
            app.error.is_none(),
            "No warning before submitting the second link."
        );
        agrees_with_controller(&app, &controller);
        let button = text_center(&proposed, "Принять");
        let output = click(&context, &mut app, button);
        controller.answer(second, &TpNumbers).unwrap();
        agrees_with_controller(&app, &controller);
        question_is(&app, FieldPath::GroupLink(0, 0));
        let reason = app.error.as_ref().unwrap();
        assert!(reason.contains(expected), "{reason}");
        assert!(texts(&output).iter().any(|text| text == reason));
    }
    assert_eq!(app.draft_undo.len(), 12);
}

#[test]
fn blank_at_either_dyad_question_returns_to_connections_without_a_rejection() {
    for first in [None, Some("4")] {
        let context = egui::Context::default();
        let mut controller = user_controller();
        let mut app = app_with_draft(&context, controller.clone());
        paint(&context, &mut app, vec![]);
        if let Some(first) = first {
            type_answer(&context, &mut app, first);
            enter(&context, &mut app);
            controller.answer(first, &TpNumbers).unwrap();
        }
        let output = paint(&context, &mut app, vec![]);
        let finish = text_center(&output, "Пробел / закончить");
        let output = click(&context, &mut app, finish);
        controller.answer(" ", &TpNumbers).unwrap();
        agrees_with_controller(&app, &controller);
        assert!(app.structure.is_some());
        assert!(app.error.is_none());
        assert!(
            texts(&output)
                .iter()
                .all(|text| !text.contains("не принята как очередная диада")
                    && !text.contains("Порядок анализа задан неверно"))
        );
    }
}

#[test]
fn rejecting_an_edited_dyad_keeps_the_completed_document_unchanged_and_cancels_cleanly() {
    let context = egui::Context::default();
    let document = EditableDialogue::open(BASIC, "DYAD EDIT", &TpNumbers).unwrap();
    let original_journal = document.current_journal().to_bytes();
    let original_model = document.loaded().mechanism.clone();
    let original_fields = InputFields::for_model(&original_model);
    let path = FieldPath::GroupLink(0, 1);
    let mut controller = document.begin_edit(&path).unwrap();
    controller.next(&TpNumbers).unwrap();
    let input_name = original_model.links[original_model.inputs[0].0]
        .name
        .to_string();
    let input_name = input_name.trim_end().to_owned();
    let mut app = Workbench::new(&eframe::CreationContext::_new_kittest(context.clone()));
    app.reset();
    app.fields = original_fields;
    app.document = Some(document);
    app.start_edit(path);
    assert!(app.edit.is_some());
    paint(&context, &mut app, vec![]);
    type_answer(&context, &mut app, &input_name);
    let output = enter(&context, &mut app);
    controller.answer(&input_name, &TpNumbers).unwrap();
    question_is(&app, FieldPath::GroupLink(0, 0));
    assert!(app.edit.is_none());
    assert!(app.draft.is_some());
    let actual = app.draft.as_ref().unwrap();
    assert_eq!(actual.journal().to_bytes(), controller.journal().to_bytes());
    assert_eq!(actual.preview(), controller.preview());
    let reason = app.error.as_ref().unwrap();
    assert!(reason.contains("— ведущее"), "{reason}");
    assert!(texts(&output).iter().any(|text| text == reason));
    assert!(app.dirty);
    assert_eq!(
        app.document.as_ref().unwrap().current_journal().to_bytes(),
        original_journal
    );
    assert_eq!(
        app.document.as_ref().unwrap().loaded().mechanism,
        original_model
    );
    let cancel = text_center(&output, "Отменить изменение структуры");
    click(&context, &mut app, cancel);
    assert!(app.draft.is_none());
    assert!(app.state.is_none());
    assert!(app.error.is_none());
    assert!(!app.dirty);
    assert_eq!(
        app.document.as_ref().unwrap().current_journal().to_bytes(),
        original_journal
    );
    assert_eq!(
        app.document.as_ref().unwrap().loaded().mechanism,
        original_model
    );
    assert!(app.undo.is_empty());
    assert!(app.redo.is_empty());
}
