//! Compact numeric presentation must not rewrite the accepted DI journal.
use super::*;

const SCENE: &[u8] = include_bytes!("../../../../tests/fixtures/diada-v320-derived/SCENE.DI");
const NEGATIVE_ZERO: &str = "-0.00000000000000000E+0000";
const DEGREES: &str = " 3.75000000000000000E+0001";

fn loaded(context: &egui::Context) -> (Workbench, FieldPath, FieldPath) {
    let original = EditableDialogue::open(SCENE, "SCENE", &TpNumbers).unwrap();
    let zero = original
        .bindings()
        .iter()
        .find(|binding| matches!(binding.path, FieldPath::GroundX(_)))
        .unwrap();
    let angle = original
        .bindings()
        .iter()
        .find(|binding| matches!(binding.path, FieldPath::GroundAngle(_)))
        .unwrap();
    let mut bytes = Vec::new();
    for (index, line) in original.current_journal().lines().iter().enumerate() {
        let raw = if index + 1 == zero.start_line {
            NEGATIVE_ZERO.as_bytes()
        } else if index + 1 == angle.start_line {
            DEGREES.as_bytes()
        } else {
            &line.bytes
        };
        bytes.extend_from_slice(raw);
        bytes.extend_from_slice(b"\r\n");
    }
    let document = EditableDialogue::open(&bytes, "SCENE", &TpNumbers).unwrap();
    let mut app = Workbench::new(&eframe::CreationContext::_new_kittest(context.clone()));
    app.reset();
    app.fields = InputFields::for_model(&document.loaded().mechanism);
    app.document = Some(document);
    (app, zero.path.clone(), angle.path.clone())
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
    assert_eq!(centers.len(), 1, "One real control must paint {value:?}.");
    centers[0]
}

fn click(context: &egui::Context, app: &mut Workbench, position: egui::Pos2) -> egui::FullOutput {
    for pressed in [true, false] {
        paint(
            context,
            app,
            vec![
                egui::Event::PointerMoved(position),
                egui::Event::PointerButton {
                    pos: position,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
    }
    paint(context, app, vec![])
}

fn enter(context: &egui::Context, app: &mut Workbench) {
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
}

fn type_answer(context: &egui::Context, app: &mut Workbench, value: &str) -> egui::FullOutput {
    paint(context, app, vec![]);
    let control = egui::Modifiers {
        ctrl: true,
        command: true,
        ..Default::default()
    };
    let mut events = vec![egui::Event::ModifiersChanged(control)];
    for pressed in [true, false] {
        events.push(egui::Event::Key {
            key: egui::Key::A,
            physical_key: Some(egui::Key::A),
            pressed,
            repeat: false,
            modifiers: control,
        });
    }
    paint(context, app, events);
    paint(
        context,
        app,
        vec![
            egui::Event::ModifiersChanged(egui::Modifiers::NONE),
            egui::Event::Text(value.into()),
        ],
    );
    assert_eq!(app.answer, value, "Typing must remain exactly as entered.");
    paint(context, app, vec![])
}

#[derive(Debug, PartialEq)]
struct Accepted {
    bytes: Vec<u8>,
    model: Mechanism,
    dirty: bool,
    undo: usize,
    redo: usize,
}

impl Accepted {
    fn read(app: &Workbench) -> Self {
        let document = app.document.as_ref().unwrap();
        Self {
            bytes: document.current_journal().to_bytes(),
            model: document.loaded().mechanism.clone(),
            dirty: app.dirty,
            undo: app.undo.len(),
            redo: app.redo.len(),
        }
    }
}

#[test]
fn compact_table_and_unchanged_enter_or_button_preserve_original_numeric_records_and_history() {
    let context = egui::Context::default();
    let (mut app, zero, angle) = loaded(&context);
    let accepted = Accepted::read(&app);
    paint(&context, &mut app, vec![]);
    let table = paint(&context, &mut app, vec![]);
    let table_text = texts(&table);
    assert!(table_text.iter().any(|text| text == "-0"));
    assert!(table_text.iter().any(|text| text == "37.5"));
    assert!(table_text.iter().all(|text| !text.contains("E+0000")));

    let label = fields::describe(&zero, app.model().unwrap()).0;
    let position = text_center(&table, &label);
    click(&context, &mut app, position);
    let edit = paint(&context, &mut app, vec![]);
    assert_eq!(app.answer, "-0");
    assert!(texts(&edit).iter().any(|text| text == "Сейчас: -0"));
    enter(&context, &mut app);
    assert!(app.edit.is_none());
    assert_eq!(Accepted::read(&app), accepted);

    let table = paint(&context, &mut app, vec![]);
    let label = fields::describe(&angle, app.model().unwrap()).0;
    let position = text_center(&table, &label);
    click(&context, &mut app, position);
    let edit = paint(&context, &mut app, vec![]);
    assert_eq!(
        app.answer, "37.5",
        "The editor must retain original degrees."
    );
    assert!(texts(&edit).iter().any(|text| text == "Сейчас: 37.5"));
    let position = text_center(&edit, "Принять");
    click(&context, &mut app, position);
    assert!(app.edit.is_none());
    assert_eq!(Accepted::read(&app), accepted);
    let document = app.document.as_ref().unwrap();
    assert_eq!(document.answer_text(&zero).unwrap(), NEGATIVE_ZERO);
    assert_eq!(document.answer_text(&angle).unwrap(), DEGREES);
}

#[test]
fn full_precision_degree_typing_and_actual_undo_redo_preserve_each_exact_di_journal() {
    let context = egui::Context::default();
    let (mut app, _, angle) = loaded(&context);
    let before = Accepted::read(&app).bytes;
    app.start_edit(angle.clone());
    paint(&context, &mut app, vec![]);
    const TYPED: &str = "33.333333333333333333";
    let proposed = type_answer(&context, &mut app, TYPED);
    let position = text_center(&proposed, "Принять");
    let accepted = click(&context, &mut app, position);
    assert!(app.error.is_none(), "{:?}", app.error);
    assert!(app.edit.is_none());
    let document = app.document.as_ref().unwrap();
    assert_eq!(document.answer_text(&angle).unwrap(), TYPED);
    let FieldPath::GroundAngle(pair) = angle else {
        unreachable!()
    };
    let expected = TpNumbers
        .degrees_to_radians(analysis::parse(TYPED).unwrap())
        .unwrap();
    assert_eq!(document.loaded().mechanism.pairs[pair.0].state.fi, expected);
    let changed = document.current_journal().to_bytes();
    assert_ne!(changed, before);
    let position = text_center(&accepted, "Отменить");
    let undone = click(&context, &mut app, position);
    assert_eq!(Accepted::read(&app).bytes, before);
    let position = text_center(&undone, "Повторить");
    click(&context, &mut app, position);
    assert_eq!(Accepted::read(&app).bytes, changed);
}

#[test]
fn incomplete_typing_cancel_and_rewound_draft_defaults_keep_original_spelling() {
    let context = egui::Context::default();
    let (mut app, zero, angle) = loaded(&context);
    let accepted = Accepted::read(&app);
    app.start_edit(zero.clone());
    paint(&context, &mut app, vec![]);
    for incomplete in ["-", "1e-"] {
        type_answer(&context, &mut app, incomplete);
        assert!(app.edit.is_some());
        assert!(app.error.is_none());
        assert_eq!(Accepted::read(&app), accepted);
    }
    let proposed = paint(&context, &mut app, vec![]);
    let position = text_center(&proposed, "Отменить изменение");
    click(&context, &mut app, position);
    assert!(app.edit.is_none());
    assert_eq!(Accepted::read(&app), accepted);

    let document = app.document.as_ref().unwrap();
    let mut draft = document.begin_edit(&angle).unwrap();
    app.restore_answers = document
        .bindings()
        .iter()
        .map(|binding| {
            (
                binding.path.clone(),
                binding.answer_text(document.current_journal()),
            )
        })
        .collect();
    let state = draft.next(&TpNumbers).unwrap();
    app.draft = Some(draft);
    app.accept(state);
    assert_eq!(app.answer, "37.5");
    assert_eq!(app.raw_answer(&app.answer), DEGREES);
    app.start_edit(zero.clone());
    assert_eq!(app.answer, "-0");
    assert_eq!(app.raw_answer(&app.answer), NEGATIVE_ZERO);
    paint(&context, &mut app, vec![]);
    enter(&context, &mut app);
    let draft = app.draft.as_ref().unwrap();
    let restored = draft
        .bindings()
        .iter()
        .rev()
        .find(|binding| binding.path == zero)
        .unwrap();
    assert_eq!(restored.answer_text(draft.journal()), NEGATIVE_ZERO);
    assert_eq!(
        app.document.as_ref().unwrap().current_journal().to_bytes(),
        accepted.bytes
    );
}
