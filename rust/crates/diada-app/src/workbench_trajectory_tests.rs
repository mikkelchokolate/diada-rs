//! Real app controls must display visited original motion dots independently
//! from accepted documents and calculation sessions.
use super::*;

const BASIC: &[u8] =
    include_bytes!("../../../../tests/fixtures/diada-v320-derived/dyad-01/SCENE.DI");

fn one_point_document() -> EditableDialogue {
    let original = EditableDialogue::open(BASIC, "TRACE", &TpNumbers).unwrap();
    let mut draft = original.begin_edit(&FieldPath::AdditionalPoints).unwrap();
    draft.next(&TpNumbers).unwrap();
    draft.answer("Y", &TpNumbers).unwrap();
    draft.answer("1", &TpNumbers).unwrap();
    draft.answer("T", &TpNumbers).unwrap();
    let mut state = draft.answer(" ", &TpNumbers).unwrap();
    for _ in 0..100 {
        match state {
            DialogueState::Complete(loaded) => {
                let document = EditableDialogue::from_loaded(loaded);
                assert_eq!(document.loaded().mechanism.points.len(), 1);
                return document;
            }
            DialogueState::Question {
                ref question,
                ref preview,
            } => {
                // Inserting T appends its attachment after the input and moves
                // the original dyad's arena index without changing that dyad.
                let source_path = if let FieldPath::Assembly(id) = question.path {
                    let current = &preview.groups[id.0];
                    original
                        .loaded()
                        .mechanism
                        .groups
                        .iter()
                        .position(|group| {
                            group.kind == current.kind
                                && group.link_a == current.link_a
                                && group.link_b == current.link_b
                        })
                        .map(|index| FieldPath::Assembly(GroupId(index)))
                        .unwrap()
                } else {
                    question.path.clone()
                };
                let answer = original
                    .bindings()
                    .iter()
                    .rev()
                    .find(|binding| binding.path == source_path)
                    .map(|binding| binding.answer_text(original.current_journal()))
                    .unwrap_or_else(|| match question.path {
                        FieldPath::PointDistance(_) => "0.25".into(),
                        FieldPath::PointAngle(_) => "45".into(),
                        _ => panic!("Unexpected new point question {:?}", question.path),
                    });
                state = draft.answer(&answer, &TpNumbers).unwrap();
            }
            _ => panic!("A valid added point cannot require undefined-memory review."),
        }
    }
    panic!("The completed one-point dialogue must be bounded.");
}

fn app_with_document(context: &egui::Context, document: EditableDialogue) -> Workbench {
    let mut app = Workbench::new(&eframe::CreationContext::_new_kittest(context.clone()));
    app.reset();
    app.fields = InputFields::for_model(&document.loaded().mechanism);
    app.document = Some(document);
    app
}

fn receive_motion(app: &mut Workbench) {
    app.motion();
    assert!(
        app.receiver.is_some(),
        "The actual motion action must launch calculation."
    );
    let start = Instant::now();
    while app.receiver.is_some() {
        app.receive();
        assert!(start.elapsed() < Duration::from_secs(10));
        std::thread::yield_now();
    }
    let series = app.series.as_ref().unwrap();
    assert_eq!(series.kind, SeriesKind::Motion);
    assert_eq!(series.frames.len(), 72);
    assert!(series.frames.iter().all(Result::is_ok));
}

fn paint(
    context: &egui::Context,
    app: &mut Workbench,
    events: Vec<egui::Event>,
) -> egui::FullOutput {
    let mut frame = eframe::Frame::_new_kittest();
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1320.0, 840.0),
        )),
        events,
        ..Default::default()
    };
    let mut output = context.run_ui(input, |ui| eframe::App::ui(app, ui, &mut frame));
    output.textures_delta.clear();
    output
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
    assert_eq!(centers.len(), 1, "One actual control must paint {value:?}.");
    centers[0]
}

fn has_text(output: &egui::FullOutput, value: &str) -> bool {
    output.shapes.iter().any(|paint| matches!(&paint.shape, egui::Shape::Text(text) if text.galley.text().contains(value)))
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

fn gold_dots(output: &egui::FullOutput) -> usize {
    output
        .shapes
        .iter()
        .filter(|paint| {
            matches!(&paint.shape, egui::Shape::Circle(dot)
        if dot.radius == 1.0 && dot.fill == egui::Color32::from_rgb(174, 145, 52)
        && paint.clip_rect.contains(dot.center))
        })
        .count()
}

fn expected_point(app: &Workbench, index: usize) -> egui::Pos2 {
    let model = &app.series.as_ref().unwrap().frames[index]
        .as_ref()
        .unwrap()
        .mechanism;
    let id = model.pair_by_name(Name(*b"T ")).unwrap();
    let [x, y] = model.pairs[id.0].state.display_xy();
    egui::pos2(x as f32, y as f32)
}

fn click_frame_ten(
    context: &egui::Context,
    app: &mut Workbench,
    output: &egui::FullOutput,
) -> egui::FullOutput {
    let status = egui::PanelState::load(context, egui::Id::new("status"))
        .unwrap()
        .outer_rect;
    let rail = output
        .shapes
        .iter()
        .filter_map(|paint| match &paint.shape {
            egui::Shape::Rect(rect)
                if rect.rect.width() >= 70.0
                    && rect.rect.height() <= 8.0
                    && status.contains(rect.rect.center()) =>
            {
                Some(rect.rect)
            }
            _ => None,
        })
        .max_by(|a, b| a.width().total_cmp(&b.width()))
        .expect("The actual frame slider has a rail.");
    let handle = output
        .shapes
        .iter()
        .filter_map(|paint| match &paint.shape {
            egui::Shape::Circle(circle)
                if circle.radius > 4.0
                    && (circle.center.y - rail.center().y).abs() < 1.0
                    && rail.expand(15.0).contains(circle.center) =>
            {
                Some(circle.center)
            }
            egui::Shape::Rect(rect)
                if rect.rect.height() > 8.0
                    && rect.rect.width() < 30.0
                    && (rect.rect.center().y - rail.center().y).abs() < 1.0
                    && rail.expand(15.0).contains(rect.rect.center()) =>
            {
                Some(rect.rect.center())
            }
            _ => None,
        })
        .min_by(|a, b| a.x.total_cmp(&b.x))
        .expect("The actual frame slider has a handle.");
    let inset = handle.x - rail.left();
    let position = egui::pos2(
        rail.left() + inset + (rail.width() - 2.0 * inset) * 10.0 / 71.0,
        rail.center().y,
    );
    let output = click(context, app, position);
    assert_eq!(
        app.current, 10,
        "Clicking the real slider must skip directly to frame10."
    );
    output
}

type Calculations = (
    Vec<Result<CalculationSnapshot, CoreError>>,
    Vec<Option<Mechanism>>,
    Vec<bool>,
    AnalysisConfig,
    ReportClock,
    SeriesKind,
);

#[derive(Debug, PartialEq)]
struct Session {
    document: Vec<u8>,
    model: Mechanism,
    calculations: Option<Calculations>,
    inputs: Vec<InputFields>,
    current: usize,
    playing: bool,
    tick: Instant,
    dirty: bool,
    history: (usize, usize, usize),
}
impl Session {
    fn read(app: &Workbench) -> Self {
        let document = app.document.as_ref().unwrap();
        Self {
            document: document.current_journal().to_bytes(),
            model: document.loaded().mechanism.clone(),
            calculations: app.series.as_ref().map(|series| {
                (
                    series.frames.clone(),
                    series.failed_previews.clone(),
                    series.kinematics_valid.clone(),
                    series.config.clone(),
                    series.clock,
                    series.kind,
                )
            }),
            inputs: app.fields.clone(),
            current: app.current,
            playing: app.playing,
            tick: app.tick,
            dirty: app.dirty,
            history: (app.undo.len(), app.redo.len(), app.draft_undo.len()),
        }
    }
    fn assert_unchanged(&self, app: &Workbench) {
        assert_eq!(
            Self::read(app),
            *self,
            "Trace controls cannot alter the calculation or accepted document."
        );
    }
}

#[test]
fn actual_motion_slider_records_only_visited_t_dots_and_new_motion_clears_them() {
    let context = egui::Context::default();
    let mut app = app_with_document(&context, one_point_document());
    receive_motion(&mut app);
    let initial = paint(&context, &mut app, vec![]);
    assert!(app.trajectories);
    assert_eq!(app.motion_trace.dots(), vec![expected_point(&app, 0)]);
    assert_eq!(gold_dots(&initial), 1);
    let visited = click_frame_ten(&context, &mut app, &initial);
    assert_eq!(
        app.motion_trace.dots(),
        vec![expected_point(&app, 0), expected_point(&app, 10)]
    );
    assert_eq!(gold_dots(&visited), 2);
    app.invalidate();
    assert!(app.motion_trace.dots().is_empty());
    assert!(app.series.is_none());
    receive_motion(&mut app);
    let restarted = paint(&context, &mut app, vec![]);
    assert_eq!(app.motion_trace.dots().len(), 1);
    click_frame_ten(&context, &mut app, &restarted);
    assert_eq!(app.motion_trace.dots().len(), 2);
    app.motion();
    assert!(app.motion_trace.dots().is_empty());
    assert!(app.series.is_none());
    app.invalidate();
    assert!(app.motion_trace.dots().is_empty());
    assert!(app.receiver.is_none());
}

#[test]
fn actual_trace_checkbox_hides_and_restores_the_same_history_without_restarting_playback() {
    let context = egui::Context::default();
    let mut app = app_with_document(&context, one_point_document());
    receive_motion(&mut app);
    app.tab = Tab::Plans;
    let initial = paint(&context, &mut app, vec![]);
    let visited = click_frame_ten(&context, &mut app, &initial);
    app.playing = true;
    app.tick = Instant::now() + Duration::from_secs(60);
    let saved = Session::read(&app);
    let dots = app.motion_trace.dots();
    let checkbox = text_center(&visited, "Траектории точек");
    let hidden = click(&context, &mut app, checkbox);
    assert!(!app.trajectories);
    assert_eq!(gold_dots(&hidden), 0);
    assert_eq!(app.motion_trace.dots(), dots);
    saved.assert_unchanged(&app);
    let checkbox = text_center(&hidden, "Траектории точек");
    let restored = click(&context, &mut app, checkbox);
    assert!(app.trajectories);
    assert_eq!(app.motion_trace.dots(), dots);
    assert_eq!(gold_dots(&restored), 2);
    saved.assert_unchanged(&app);
}

#[test]
fn hiding_m_keeps_the_actual_motion_dots_visible() {
    let context = egui::Context::default();
    let mut app = app_with_document(&context, one_point_document());
    receive_motion(&mut app);
    app.tab = Tab::Plans;
    let output = paint(&context, &mut app, vec![]);
    assert_eq!(gold_dots(&output), 1);
    let saved = Session::read(&app);
    let checkbox = text_center(&output, "M · механизм");
    let without_m = click(&context, &mut app, checkbox);
    assert!(!app.show_m && app.trajectories);
    assert_eq!(gold_dots(&without_m), 1);
    assert!(
        !without_m.shapes.iter().any(
            |paint| matches!(&paint.shape, egui::Shape::Circle(circle) if circle.radius == 4.5)
        ),
        "The trace-only M cell must hide the mechanism's hinges."
    );
    saved.assert_unchanged(&app);
}

#[test]
fn unavailable_trace_controls_explain_the_reason_and_cannot_change_any_session() {
    for mode in 0..3 {
        let context = egui::Context::default();
        let document = if mode == 2 {
            EditableDialogue::open(BASIC, "BASIC", &TpNumbers).unwrap()
        } else {
            one_point_document()
        };
        let mut app = app_with_document(&context, document);
        if mode == 1 {
            let model = app.document.as_ref().unwrap().loaded().mechanism.clone();
            app.series = Some(analysis::calculate(
                model,
                analysis::config(&app.fields, 12, false).unwrap(),
                SeriesKind::Analysis,
            ));
        } else if mode == 2 {
            receive_motion(&mut app);
        }
        app.tab = Tab::Plans;
        let output = paint(&context, &mut app, vec![]);
        assert!(!app.trajectories);
        assert!(app.motion_trace.dots().is_empty());
        assert!(has_text(
            &output,
            if mode == 2 {
                "Для траекторий задайте дополнительные точки"
            } else {
                "Траектории доступны в движении"
            }
        ));
        let saved = Session::read(&app);
        let checkbox = text_center(&output, "Траектории точек");
        paint(
            &context,
            &mut app,
            vec![egui::Event::PointerMoved(checkbox)],
        );
        click(&context, &mut app, checkbox);
        assert!(!app.trajectories);
        assert!(app.motion_trace.dots().is_empty());
        saved.assert_unchanged(&app);
    }
}
