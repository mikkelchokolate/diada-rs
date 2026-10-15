//! Playback must move the linkage against fixed supports on a stable canvas.
use super::*;

const FOUR_BAR: &[u8] =
    include_bytes!("../../../../tests/fixtures/diada-v320-derived/dyad-01/SCENE.DI");
const SLIDER_CRANK: &[u8] =
    include_bytes!("../../../../tests/fixtures/diada-v320-derived/dyad-02/SCENE.DI");

fn app(context: &egui::Context, bytes: &[u8]) -> Workbench {
    let mut app = Workbench::new(&eframe::CreationContext::_new_kittest(context.clone()));
    app.reset();
    let document = EditableDialogue::open(bytes, "FIXED SUPPORTS", &TpNumbers).unwrap();
    app.fields = InputFields::for_model(&document.loaded().mechanism);
    app.document = Some(document);
    app
}

fn paint(
    context: &egui::Context,
    app: &mut Workbench,
    events: Vec<egui::Event>,
) -> egui::FullOutput {
    let size = context
        .data_mut(|data| data.get_temp::<egui::Vec2>(egui::Id::new("motion_test_viewport")))
        .unwrap_or(egui::vec2(1320.0, 840.0));
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
        events,
        ..Default::default()
    };
    let mut frame = eframe::Frame::_new_kittest();
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

fn receive(context: &egui::Context, app: &mut Workbench) -> egui::FullOutput {
    let start = Instant::now();
    while app.receiver.is_some() {
        app.receive();
        assert!(start.elapsed() < Duration::from_secs(10));
        std::thread::yield_now();
    }
    assert!(
        app.series.is_some(),
        "An actual action must produce a series."
    );
    paint(context, app, vec![]);
    paint(context, app, vec![])
}

fn start(context: &egui::Context, app: &mut Workbench, kind: SeriesKind) -> egui::FullOutput {
    let label = match kind {
        SeriesKind::Analysis => "Стандартные 12",
        SeriesKind::Motion => "Движение · 72",
        SeriesKind::Graphs => "Графики · 0–360°",
    };
    paint(context, app, vec![]);
    let output = paint(context, app, vec![]);
    click(context, app, text_center(&output, label));
    let output = receive(context, app);
    assert_eq!(app.series.as_ref().unwrap().kind, kind);
    output
}

fn resize(context: &egui::Context, app: &mut Workbench, size: egui::Vec2) -> egui::FullOutput {
    context.data_mut(|data| data.insert_temp(egui::Id::new("motion_test_viewport"), size));
    paint(context, app, vec![]);
    paint(context, app, vec![])
}

fn hinges(output: &egui::FullOutput) -> Vec<egui::Pos2> {
    output
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
        .collect()
}

fn slider_center(output: &egui::FullOutput) -> egui::Pos2 {
    let centers: Vec<_> = output
        .shapes
        .iter()
        .filter_map(|paint| match &paint.shape {
            egui::Shape::Path(path) if path.closed && path.points.len() == 4 => {
                let bounds = egui::Rect::from_points(&path.points);
                ((bounds.width() - 34.8).abs() < 0.01
                    && (bounds.height() - 19.8).abs() < 0.01
                    && paint.clip_rect.contains(bounds.center()))
                .then_some(bounds.center())
            }
            _ => None,
        })
        .collect();
    assert_eq!(centers.len(), 1, "The actual slider must remain visible.");
    centers[0]
}

fn rail_and_hatches(output: &egui::FullOutput, slider: egui::Pos2) -> Vec<[egui::Pos2; 2]> {
    let rail: Vec<_> = output
        .shapes
        .iter()
        .filter_map(|paint| match &paint.shape {
            egui::Shape::LineSegment { points, stroke }
                if (stroke.width - 1.6).abs() < 0.001
                    && (points[0].y - slider.y - 12.0).abs() < 0.01
                    && (points[1].y - points[0].y).abs() < 0.01
                    && (points[1].x - points[0].x).abs() > 80.0 =>
            {
                Some(*points)
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        rail.len(),
        1,
        "The actual horizontal ground guide must be painted."
    );
    let rail = rail[0];
    let mut result = vec![rail];
    result.extend(output.shapes.iter().filter_map(|paint| match &paint.shape {
        egui::Shape::LineSegment { points, stroke }
            if (stroke.width - 1.6).abs() < 0.001
                && (points[0].y - rail[0].y).abs() < 0.01
                && (points[1].x - points[0].x + 4.5).abs() < 0.01
                && (points[1].y - points[0].y - 4.5).abs() < 0.01
                && points[0].x >= rail[0].x - 0.01
                && points[0].x <= rail[1].x + 0.01 =>
        {
            Some(*points)
        }
        _ => None,
    }));
    assert!(
        result.len() > 2,
        "The actual local support must include its hatching."
    );
    result
}

fn scrub_to(
    context: &egui::Context,
    app: &mut Workbench,
    output: &egui::FullOutput,
    index: usize,
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
    let point = if index == 0 {
        egui::pos2(rail.left(), rail.center().y)
    } else {
        // The first scrub starts at zero, exposing the actual end inset.
        assert_eq!(app.current, 0);
        let inset = handle.x - rail.left();
        let len = app.series.as_ref().unwrap().frames.len();
        egui::pos2(
            rail.left() + inset + (rail.width() - 2.0 * inset) * index as f32 / (len - 1) as f32,
            rail.center().y,
        )
    };
    let output = click(context, app, point);
    assert_eq!(
        app.current, index,
        "The real slider must select the requested frame."
    );
    output
}

fn same_position(actual: egui::Pos2, expected: egui::Pos2, message: &str) {
    assert!(
        actual.distance(expected) < 0.01,
        "{message}: before {expected:?}, after {actual:?}"
    );
}

#[derive(Debug, PartialEq)]
struct PaintedCamera {
    supports: [egui::Pos2; 2],
    scale_text: String,
    scale_bar: [egui::Pos2; 2],
}

fn painted_camera(output: &egui::FullOutput) -> PaintedCamera {
    let points = hinges(output);
    assert!(points.len() >= 2);
    let scales: Vec<_> = output
        .shapes
        .iter()
        .filter_map(|paint| match &paint.shape {
            egui::Shape::Text(text)
                if text
                    .galley
                    .text()
                    .strip_suffix(" м")
                    .is_some_and(|number| number.parse::<f32>().is_ok()) =>
            {
                Some(text)
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        scales.len(),
        1,
        "The real canvas must paint its metric scale."
    );
    let label = scales[0];
    let bars: Vec<_> = output
        .shapes
        .iter()
        .filter_map(|paint| match &paint.shape {
            egui::Shape::LineSegment { points, stroke }
                if (stroke.width - 1.0).abs() < 0.001
                    && (points[0].x - label.pos.x).abs() < 0.01
                    && (points[1].y - points[0].y).abs() < 0.01
                    && points[1].x > points[0].x + 5.0
                    && points[0].y > label.pos.y
                    && points[0].y < label.pos.y + label.galley.size().y + 10.0 =>
            {
                Some(*points)
            }
            _ => None,
        })
        .collect();
    assert_eq!(bars.len(), 1, "The real metric scale must include a ruler.");
    PaintedCamera {
        supports: [points[0], *points.last().unwrap()],
        scale_text: label.galley.text().into(),
        scale_bar: bars[0],
    }
}

#[test]
fn one_workbench_keeps_the_painted_camera_across_analysis_motion_and_graphs() {
    let context = egui::Context::default();
    let mut app = app(&context, FOUR_BAR);
    start(&context, &mut app, SeriesKind::Analysis);
    // Retain a user's camera adjustment as well as the original fit rectangle.
    app.view.zoom = 1.05;
    app.view.pan = egui::vec2(7.0, -5.0);
    let first = paint(&context, &mut app, vec![]);
    let expected = painted_camera(&first);
    let accepted = app.document.as_ref().unwrap().current_journal().to_bytes();
    for (kind, count) in [
        (SeriesKind::Motion, 72),
        (SeriesKind::Graphs, 73),
        (SeriesKind::Analysis, 12),
    ] {
        let initial = start(&context, &mut app, kind);
        assert_eq!(app.series.as_ref().unwrap().frames.len(), count);
        assert_eq!(
            painted_camera(&initial),
            expected,
            "{kind:?} changed the camera."
        );
        let moving_before = hinges(&initial)[1];
        let moved = scrub_to(&context, &mut app, &initial, count / 4);
        assert_eq!(
            painted_camera(&moved),
            expected,
            "{kind:?} scrubbing changed the camera."
        );
        assert!(hinges(&moved)[1].distance(moving_before) > 2.0);
        assert_eq!(app.view.zoom, 1.05);
        assert_eq!(app.view.pan, egui::vec2(7.0, -5.0));
        assert_eq!(
            app.document.as_ref().unwrap().current_journal().to_bytes(),
            accepted
        );
    }
}

#[test]
fn plans_keep_ground_supports_and_scale_when_the_trajectory_hint_changes_between_series() {
    for size in [egui::vec2(1320.0, 840.0), egui::vec2(840.0, 560.0)] {
        let context = egui::Context::default();
        let mut app = app(
            &context,
            include_bytes!("../../../../tests/fixtures/diada-v320-derived/SCENE.DI"),
        );
        assert!(
            !crate::preview::trajectory_points(&app.document.as_ref().unwrap().loaded().mechanism)
                .is_empty(),
            "The fixture must exercise the changing trajectory hints."
        );
        app.tab = Tab::Plans;
        app.show_m = true;
        app.show_v = false;
        app.show_a = false;
        start(&context, &mut app, SeriesKind::Analysis);
        let initial = resize(&context, &mut app, size);
        let camera = painted_camera(&initial);
        for kind in [SeriesKind::Motion, SeriesKind::Graphs, SeriesKind::Analysis] {
            let output = start(&context, &mut app, kind);
            assert_eq!(
                painted_camera(&output),
                camera,
                "{kind:?} in {size:?}: a changed trajectory hint moved O/H or the scale."
            );
            if kind == SeriesKind::Motion {
                assert!(app.trajectories);
                assert!(!app.motion_trace.dots().is_empty());
            }
        }
    }
}

#[test]
fn single_position_to_failing_motion_preserves_camera_until_fit_and_model_changes_reset_it() {
    let context = egui::Context::default();
    let mut app = app(&context, FOUR_BAR);
    let mut loaded = app.document.as_ref().unwrap().loaded().clone();
    let model = &mut loaded.mechanism;
    for (name, length) in [(b'1', "1"), (b'2', "1.5"), (b'3', "2")] {
        let id = model.link_by_name(Name([name, b' '])).unwrap();
        model.links[id.0].geometry.parameter = analysis::parse(length).unwrap();
    }
    let fixed = model.pair_by_name(Name(*b"C ")).unwrap();
    model.pairs[fixed.0].state.x = analysis::parse("4").unwrap();
    let crank = model.link_by_name(Name(*b"1 ")).unwrap();
    app.document = Some(EditableDialogue::from_loaded(loaded));
    app.positions = 1;
    paint(&context, &mut app, vec![]);
    let before = paint(&context, &mut app, vec![]);
    click(&context, &mut app, text_center(&before, "Рассчитать"));
    let initial = receive(&context, &mut app);
    assert_eq!(app.series.as_ref().unwrap().frames.len(), 1);
    assert!(app.series.as_ref().unwrap().frames[0].is_ok());
    let camera = painted_camera(&initial);
    let initial_bounds = app.view.fixed_bounds;
    let motion = start(&context, &mut app, SeriesKind::Motion);
    assert!(
        app.series
            .as_ref()
            .unwrap()
            .frames
            .iter()
            .any(Result::is_err)
    );
    assert_eq!(
        painted_camera(&motion),
        camera,
        "Adding failed frames cannot change canvas height or fit."
    );
    let failed = scrub_to(&context, &mut app, &motion, 36);
    assert_eq!(painted_camera(&failed), camera);
    assert_ne!(
        app.view.fit_bounds, initial_bounds,
        "The full motion has a different automatic extent."
    );
    let fitted = click(&context, &mut app, text_center(&failed, "Вписать"));
    assert_eq!(app.view.fixed_bounds, app.view.fit_bounds);
    assert_eq!(app.view.zoom, 1.0);
    assert_eq!(app.view.pan, egui::Vec2::ZERO);
    assert_ne!(
        painted_camera(&fitted),
        camera,
        "The actual Fit button must adopt the current motion extent."
    );

    app.start_edit(FieldPath::LinkParameter(crank));
    assert!(app.edit.is_some());
    app.submit("0.75");
    assert!(app.error.is_none(), "{:?}", app.error);
    assert!(app.edit.is_none());
    assert!(app.series.is_none());
    assert!(app.view.fixed_bounds.is_none());
    assert!(app.view.fit_bounds.is_none());
    start(&context, &mut app, SeriesKind::Analysis);
    assert!(app.view.fixed_bounds.is_some());
    app.load_path(
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../../tests/fixtures/diada-v320-derived/dyad-01/SCENE.DI"),
    );
    assert!(app.error.is_none(), "{:?}", app.error);
    assert!(app.series.is_none());
    assert!(app.view.fixed_bounds.is_none());
    assert!(app.view.fit_bounds.is_none());
    assert_eq!(app.view.zoom, 1.0);
    assert_eq!(app.view.pan, egui::Vec2::ZERO);
}

#[test]
fn actual_standard_motion_and_graph_controls_keep_both_fixed_hinges_in_place() {
    for size in [egui::vec2(1320.0, 840.0), egui::vec2(840.0, 560.0)] {
        for kind in [SeriesKind::Analysis, SeriesKind::Motion, SeriesKind::Graphs] {
            let context = egui::Context::default();
            let mut app = app(&context, FOUR_BAR);
            start(&context, &mut app, kind);
            let initial = resize(&context, &mut app, size);
            let before = hinges(&initial);
            assert_eq!(before.len(), 4, "O, A, B and C must all be painted.");
            let index = app.series.as_ref().unwrap().frames.len() / 4;
            let scrubbed = scrub_to(&context, &mut app, &initial, index);
            let after = hinges(&scrubbed);
            assert_eq!(after.len(), 4);
            same_position(
                after[0],
                before[0],
                &format!("{kind:?}: fixed O drifted on scrubbing"),
            );
            same_position(
                after[3],
                before[3],
                &format!("{kind:?}: fixed C drifted on scrubbing"),
            );
            assert!(
                after[1].distance(before[1]) > 2.0,
                "The crank must actually move."
            );
            let started = click(&context, &mut app, text_center(&scrubbed, "Пуск"));
            assert!(app.playing);
            text_center(&started, "Пауза");
            for _ in 0..3 {
                app.tick = Instant::now() - Duration::from_secs(1);
                let playing = paint(&context, &mut app, vec![]);
                let positions = hinges(&playing);
                same_position(
                    positions[0],
                    before[0],
                    &format!("{kind:?}: fixed O drifted during playback"),
                );
                same_position(
                    positions[3],
                    before[3],
                    &format!("{kind:?}: fixed C drifted during playback"),
                );
            }
            assert_eq!(app.current, index + 3);
            let playing = paint(&context, &mut app, vec![]);
            let paused = click(&context, &mut app, text_center(&playing, "Пауза"));
            assert!(!app.playing);
            let stepped = click(&context, &mut app, text_center(&paused, "▶"));
            assert_eq!(app.current, index + 4);
            let positions = hinges(&stepped);
            same_position(
                positions[0],
                before[0],
                "Fixed O drifted on the next-frame button",
            );
            same_position(
                positions[3],
                before[3],
                "Fixed C drifted on the next-frame button",
            );
        }
    }
}

#[test]
fn actual_slider_crank_playback_moves_the_grounded_slider_but_not_the_fixed_hinge() {
    for size in [egui::vec2(1320.0, 840.0), egui::vec2(840.0, 560.0)] {
        for kind in [SeriesKind::Analysis, SeriesKind::Motion, SeriesKind::Graphs] {
            let context = egui::Context::default();
            let mut app = app(&context, SLIDER_CRANK);
            start(&context, &mut app, kind);
            let initial = resize(&context, &mut app, size);
            let fixed = hinges(&initial)[0];
            let slider = slider_center(&initial);
            let guide = rail_and_hatches(&initial, slider);
            let index = app.series.as_ref().unwrap().frames.len() / 4;
            let scrubbed = scrub_to(&context, &mut app, &initial, index);
            same_position(
                hinges(&scrubbed)[0],
                fixed,
                &format!("{kind:?}: fixed O drifted"),
            );
            let moved = slider_center(&scrubbed);
            let moved_guide = rail_and_hatches(&scrubbed, moved);
            assert_eq!(guide.len(), moved_guide.len());
            assert!((guide[0][1].x - guide[0][0].x - 96.0).abs() < 0.01);
            for (before, after) in guide.iter().zip(&moved_guide) {
                for (a, b) in before.iter().zip(after) {
                    assert!(
                        ((*a - slider) - (*b - moved)).length() < 0.01,
                        "{kind:?}: the short support must follow the slider."
                    );
                }
            }
            assert!(
                moved.distance(slider) > 2.0,
                "A grounded P pair is a moving slider, not a fixed hinge."
            );
            assert!(
                (moved.y - slider.y).abs() < 0.01,
                "The horizontal guide must remain horizontal."
            );
        }
    }
}

#[test]
fn the_actual_m_checkbox_hides_ground_guides_while_preserving_the_motion_dots() {
    let context = egui::Context::default();
    let mut app = app(
        &context,
        include_bytes!("../../../../tests/fixtures/diada-v320-derived/SCENE.DI"),
    );
    start(&context, &mut app, SeriesKind::Motion);
    assert!(
        app.trajectories,
        "The fixture must exercise a trace-only canvas."
    );
    app.tab = Tab::Plans;
    app.show_v = false;
    app.show_a = false;
    paint(&context, &mut app, vec![]);
    let visible = paint(&context, &mut app, vec![]);
    let body_lines =
        |output: &egui::FullOutput| {
            output.shapes.iter().filter(|paint| matches!(
            &paint.shape,
            egui::Shape::LineSegment { stroke, .. } if (stroke.width - 1.6).abs() < 0.001
        )).count()
        };
    let dots =
        |output: &egui::FullOutput| {
            output.shapes.iter().filter(|paint| matches!(
            &paint.shape,
            egui::Shape::Circle(circle)
                if circle.radius == 1.0 && circle.fill == Color32::from_rgb(174, 145, 52)
        )).count()
        };
    assert!(
        body_lines(&visible) > 20,
        "The visible mechanism must include the ground rail hatches."
    );
    let visible_dots = dots(&visible);
    assert!(visible_dots > 0);
    let hidden = click(&context, &mut app, text_center(&visible, "M · механизм"));
    assert!(!app.show_m);
    assert!(hinges(&hidden).is_empty());
    assert_eq!(
        body_lines(&hidden),
        0,
        "Fixed guides cannot leak into the trace-only canvas."
    );
    assert_eq!(
        dots(&hidden),
        visible_dots,
        "Hiding M must keep the visited motion dots."
    );
    let restored = click(&context, &mut app, text_center(&hidden, "M · механизм"));
    assert!(app.show_m);
    assert_eq!(body_lines(&restored), body_lines(&visible));
    assert_eq!(dots(&restored), visible_dots);
}

#[test]
fn valid_failed_and_valid_series_frames_keep_both_supports_on_the_same_canvas() {
    for size in [egui::vec2(1320.0, 840.0), egui::vec2(840.0, 560.0)] {
        for kind in [SeriesKind::Analysis, SeriesKind::Motion, SeriesKind::Graphs] {
            let context = egui::Context::default();
            let mut app = app(&context, FOUR_BAR);
            let mut loaded = app.document.as_ref().unwrap().loaded().clone();
            let model = &mut loaded.mechanism;
            for (link, length) in [(b'1', "1"), (b'2', "1.5"), (b'3', "2")] {
                let id = model.link_by_name(Name([link, b' '])).unwrap();
                model.links[id.0].geometry.parameter = analysis::parse(length).unwrap();
            }
            let fixed = model.pair_by_name(Name(*b"C ")).unwrap();
            model.pairs[fixed.0].state.x = analysis::parse("4").unwrap();
            app.document = Some(EditableDialogue::from_loaded(loaded));
            start(&context, &mut app, kind);
            let initial = resize(&context, &mut app, size);
            let failing_index = match kind {
                SeriesKind::Analysis => 6,
                SeriesKind::Motion | SeriesKind::Graphs => 36,
            };
            assert!(app.series.as_ref().unwrap().frames[0].is_ok());
            assert!(app.series.as_ref().unwrap().frames[failing_index].is_err());
            let before = hinges(&initial);
            assert_eq!(before.len(), 4);
            let failed = scrub_to(&context, &mut app, &initial, failing_index);
            let partial = hinges(&failed);
            assert_eq!(
                partial.len(),
                3,
                "Only O, defined A, and fixed C remain visible."
            );
            same_position(
                partial[0],
                before[0],
                &format!("{kind:?}: O jumped on failure"),
            );
            same_position(
                partial[2],
                before[3],
                &format!("{kind:?}: C jumped on failure"),
            );
            let recovered = scrub_to(&context, &mut app, &failed, 0);
            let after = hinges(&recovered);
            same_position(
                after[0],
                before[0],
                &format!("{kind:?}: O jumped after recovery"),
            );
            same_position(
                after[3],
                before[3],
                &format!("{kind:?}: C jumped after recovery"),
            );
        }
    }
}
