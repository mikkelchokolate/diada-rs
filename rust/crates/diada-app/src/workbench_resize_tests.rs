//! Real app UI input tests; no desktop window or native automation is involved.
use super::*;

const SCENE: &[u8] = include_bytes!("../../../../tests/fixtures/diada-v320-derived/SCENE.DI");

fn loaded_app(context: &egui::Context) -> Workbench {
    // Keep the actual production theme/font setup, but discard command-line
    // arguments belonging to the test harness before opening the fixture.
    let mut app = Workbench::new(&eframe::CreationContext::_new_kittest(context.clone()));
    app.reset();
    let document = EditableDialogue::open(SCENE, "SCENE", &TpNumbers).unwrap();
    app.fields = InputFields::for_model(&document.loaded().mechanism);
    app.document = Some(document);
    app
}

fn paint_app(
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

fn panel_rect(context: &egui::Context, name: &str) -> egui::Rect {
    egui::PanelState::load(context, egui::Id::new(name))
        .expect("The actual app painted and stored this panel.")
        .outer_rect
}

#[derive(Clone, Copy)]
enum ResizeEdge {
    Right,
    Bottom,
}
impl ResizeEdge {
    fn handle(self, rect: egui::Rect) -> egui::Pos2 {
        match self {
            Self::Right => rect.left_center(),
            Self::Bottom => rect.center_top(),
        }
    }
    fn size(self, rect: egui::Rect) -> f32 {
        match self {
            Self::Right => rect.width(),
            Self::Bottom => rect.height(),
        }
    }
    fn correct_cursor(self, cursor: egui::CursorIcon) -> bool {
        match self {
            Self::Right => matches!(
                cursor,
                egui::CursorIcon::ResizeHorizontal
                    | egui::CursorIcon::ResizeWest
                    | egui::CursorIcon::ResizeEast
            ),
            Self::Bottom => matches!(
                cursor,
                egui::CursorIcon::ResizeVertical
                    | egui::CursorIcon::ResizeNorth
                    | egui::CursorIcon::ResizeSouth
            ),
        }
    }
}

struct AcceptedData {
    journal: Vec<u8>,
    dirty: bool,
    selected: Option<ObjectKey>,
    pan: egui::Vec2,
}
impl AcceptedData {
    fn read(app: &Workbench) -> Self {
        Self {
            journal: app.document.as_ref().unwrap().current_journal().to_bytes(),
            dirty: app.dirty,
            selected: app.view.selected,
            pan: app.view.pan,
        }
    }
    fn assert_unchanged(&self, app: &Workbench) {
        assert_eq!(
            app.document.as_ref().unwrap().current_journal().to_bytes(),
            self.journal,
            "Dragging a panel cannot rewrite accepted answers."
        );
        assert_eq!(app.dirty, self.dirty);
        assert_eq!(app.view.selected, self.selected);
        assert_eq!(
            app.view.pan, self.pan,
            "The panel drag cannot pan the canvas."
        );
        assert!(app.undo.is_empty() && app.redo.is_empty() && app.draft_undo.is_empty());
        assert!(app.state.is_none() && app.draft.is_none() && app.edit.is_none());
        assert!(app.pending.is_none());
    }
}

fn drag_and_check_retained_size(
    context: &egui::Context,
    app: &mut Workbench,
    name: &str,
    edge: ResizeEdge,
) {
    paint_app(context, app, vec![]);
    paint_app(context, app, vec![]);
    let before = panel_rect(context, name);
    let handle = edge.handle(before);
    let hovered = paint_app(context, app, vec![egui::Event::PointerMoved(handle)]);
    assert!(
        edge.correct_cursor(hovered.platform_output.cursor_icon),
        "The rendered {name} edge must be the actual resize handle: {:?}",
        hovered.platform_output.cursor_icon
    );
    let saved = AcceptedData::read(app);
    paint_app(
        context,
        app,
        vec![egui::Event::PointerButton {
            pos: handle,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        }],
    );
    let delta = match edge {
        ResizeEdge::Right => egui::vec2(-120.0, 0.0),
        ResizeEdge::Bottom => egui::vec2(0.0, -120.0),
    };
    let moved = handle + delta;
    // A held pointer move crosses the drag threshold; a second frame gives the
    // resize widget's previous-frame response to the actual panel allocator.
    paint_app(context, app, vec![egui::Event::PointerMoved(moved)]);
    paint_app(context, app, vec![egui::Event::PointerMoved(moved)]);
    paint_app(
        context,
        app,
        vec![egui::Event::PointerButton {
            pos: moved,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        }],
    );
    let released_size = edge.size(panel_rect(context, name));
    assert!(
        released_size - edge.size(before) >= 100.0,
        "{name} snapped back on release: before {}, after {released_size}",
        edge.size(before)
    );
    for _ in 0..3 {
        paint_app(context, app, vec![]);
        let settled_size = edge.size(panel_rect(context, name));
        assert!(
            (settled_size - released_size).abs() < 1.0,
            "{name} changed after idle layout: {released_size} -> {settled_size}"
        );
    }
    saved.assert_unchanged(app);
}

#[test]
fn actual_question_panel_drag_keeps_its_width_after_release() {
    let context = egui::Context::default();
    let mut app = loaded_app(&context);
    drag_and_check_retained_size(&context, &mut app, "question", ResizeEdge::Right);
    assert!(app.structure.is_none());
}

#[test]
fn actual_parameters_panel_drag_keeps_its_height_after_release() {
    let context = egui::Context::default();
    let mut app = loaded_app(&context);
    drag_and_check_retained_size(&context, &mut app, "parameters_panel", ResizeEdge::Bottom);
    assert!(app.structure.is_none());
}

#[test]
fn actual_connections_panel_drag_keeps_width_and_does_not_accept_a_proposal() {
    let context = egui::Context::default();
    let mut app = loaded_app(&context);
    app.open_structure();
    drag_and_check_retained_size(&context, &mut app, "structure_editor", ResizeEdge::Right);
    let form = app.structure.as_ref().unwrap();
    assert!(form.same_structure(&app.document.as_ref().unwrap().loaded().mechanism));
    assert!(form.selected_preview_objects().is_none());
}
