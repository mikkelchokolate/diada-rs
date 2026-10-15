//! Original 3.20 motion-dot storage, kept outside calculated mechanism data.
//!
//! Pinned OVR SHA256 0663b42d70d2978af337ceef41651aea13a82ce164c242be284be24bf2ba25d7,
//! U01 file+0x5D4E: CS1F74..1FA6 initializes 72 XY slots per T/M/F point;
//! CS526A..5280 stores only slot[N14]; CS578F plots existing slots with PutPixel.
//! Passes reuse those slots. A new pass replaces the current slot and retains
//! the other dots, rather than predicting future dots or connecting them.
use diada_core::{Mechanism, PairId};
use eframe::egui::Pos2;

const SLOTS: usize = 72;

#[derive(Default)]
pub struct MotionTrace {
    points: Vec<(PairId, [Option<Pos2>; SLOTS])>,
}

impl MotionTrace {
    pub fn record(&mut self, model: &Mechanism, frame_index: usize) {
        self.record_points(crate::preview::trajectory_points(model), frame_index);
    }

    fn record_points(
        &mut self,
        points: impl IntoIterator<Item = (PairId, Pos2)>,
        frame_index: usize,
    ) {
        let slot = frame_index % SLOTS;
        // Missing or malformed current geometry cannot certify an old value
        // from a previous pass for the cell being visited now.
        for (_, cells) in &mut self.points {
            cells[slot] = None;
        }
        for (id, position) in points {
            if !position.x.is_finite() || !position.y.is_finite() {
                continue;
            }
            if let Some((_, cells)) = self.points.iter_mut().find(|(key, _)| *key == id) {
                cells[slot] = Some(position);
            } else {
                let mut cells = [None; SLOTS];
                cells[slot] = Some(position);
                self.points.push((id, cells));
            }
        }
    }

    pub fn dots(&self) -> Vec<Pos2> {
        self.points
            .iter()
            .flat_map(|(_, cells)| cells.iter().flatten().copied())
            .collect()
    }

    pub fn clear(&mut self) {
        self.points.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use diada_core::{
        AnalysisConfig, AnalysisMode, CalculationSnapshot, InputSeries, Name, Real48,
    };
    use diada_io::{DialogueNumbers, EditableDialogue, TpNumbers};

    const SCENE: &[u8] = include_bytes!("../../../../tests/fixtures/diada-v320-derived/SCENE.DI");
    const FORCE: &[u8] =
        include_bytes!("../../../../tests/fixtures/diada-v320-derived/force-fixed-02/SCENE.DI");

    fn frames(input: &[u8]) -> Vec<CalculationSnapshot> {
        let document = EditableDialogue::open(input, "SCENE", &TpNumbers).unwrap();
        let model = &document.loaded().mechanism;
        let number = |text: &str| TpNumbers.parse_real(text.as_bytes()).unwrap();
        let config = AnalysisConfig {
            positions: 12,
            inputs: model
                .inputs
                .iter()
                .map(|link| InputSeries {
                    link: *link,
                    start: Real48::ZERO,
                    other_start: number("30"),
                    step: number("30"),
                    rate: number("1"),
                    rate_derivative: Real48::ZERO,
                })
                .collect(),
            mode: AnalysisMode::Kinematics,
        };
        model.sample_kinematics(&config).unwrap()
    }

    #[test]
    fn ordinary_hinges_and_sliders_of_all_five_dyads_do_not_become_original_trace_points() {
        let fixtures: [&[u8]; 5] = [
            include_bytes!("../../../../tests/fixtures/diada-v320-derived/dyad-01/SCENE.DI"),
            include_bytes!("../../../../tests/fixtures/diada-v320-derived/dyad-02/SCENE.DI"),
            include_bytes!("../../../../tests/fixtures/diada-v320-derived/dyad-03/SCENE.DI"),
            include_bytes!("../../../../tests/fixtures/diada-v320-derived/dyad-04/SCENE.DI"),
            include_bytes!("../../../../tests/fixtures/diada-v320-derived/dyad-05/SCENE.DI"),
        ];
        for input in fixtures {
            let frames = frames(input);
            let mut trace = MotionTrace::default();
            for (index, frame) in frames.iter().enumerate() {
                assert!(crate::preview::trajectory_points(&frame.mechanism).is_empty());
                trace.record(&frame.mechanism, index);
            }
            assert!(trace.dots().is_empty());
        }
    }

    #[test]
    fn original_t_m_f_dots_follow_the_same_auxiliary_coordinates_as_the_visible_scene() {
        for (input, expected) in [(SCENE, 2), (FORCE, 4)] {
            for frame in frames(input) {
                let points = crate::preview::trajectory_points(&frame.mechanism);
                assert_eq!(points.len(), expected);
                let scene = crate::drawing::Scene::mechanism(&frame.mechanism);
                for (id, position) in points {
                    assert_eq!(scene.nodes[id.0].position, position);
                    assert!(matches!(
                        frame.mechanism.pairs[id.0].kind,
                        diada_core::PairKind::Point
                            | diada_core::PairKind::MassCenter
                            | diada_core::PairKind::ForcePoint
                    ));
                }
            }
        }
    }

    #[test]
    fn first_frame_records_one_dot_per_point_without_predicting_future_frames() {
        let frames = frames(SCENE);
        let mut trace = MotionTrace::default();
        trace.record(&frames[0].mechanism, 0);
        let expected = crate::preview::trajectory_points(&frames[0].mechanism)
            .into_iter()
            .map(|(_, position)| position)
            .collect::<Vec<_>>();
        assert_eq!(trace.dots(), expected);
        assert!(
            trace
                .points
                .iter()
                .all(|(_, cells)| cells.iter().flatten().count() == 1)
        );
        trace.record(&frames[10].mechanism, 10);
        assert_eq!(trace.dots().len(), expected.len() * 2);
        assert!(
            trace
                .points
                .iter()
                .all(|(_, cells)| cells[1..10].iter().all(Option::is_none))
        );
    }

    #[test]
    fn repeat_and_reverse_pass_replace_the_same_72_slots_and_keep_other_visited_dots() {
        let frames = frames(SCENE);
        let mut trace = MotionTrace::default();
        trace.record(&frames[0].mechanism, 0);
        trace.record(&frames[1].mechanism, 1);
        let retained = trace
            .points
            .iter()
            .map(|(_, cells)| cells[1])
            .collect::<Vec<_>>();
        trace.record(&frames[11].mechanism, 72);
        assert_eq!(trace.dots().len(), 4);
        assert_eq!(
            trace
                .points
                .iter()
                .map(|(_, cells)| cells[1])
                .collect::<Vec<_>>(),
            retained
        );
        for (id, position) in crate::preview::trajectory_points(&frames[11].mechanism) {
            assert_eq!(
                trace.points.iter().find(|(key, _)| *key == id).unwrap().1[0],
                Some(position)
            );
        }
        trace.record(&frames[0].mechanism, 144);
        assert_eq!(trace.dots().len(), 4);
    }

    #[test]
    fn equally_named_mass_centres_keep_their_distinct_pair_identity() {
        let mut model = frames(FORCE).remove(0).mechanism;
        let centres = model
            .pairs
            .iter_mut()
            .filter(|pair| pair.kind == diada_core::PairKind::MassCenter)
            .map(|pair| {
                pair.name = Name::BLANK;
            })
            .count();
        assert_eq!(centres, 3);
        let mut trace = MotionTrace::default();
        trace.record(&model, 0);
        assert_eq!(trace.points.len(), 4);
        assert_eq!(trace.dots().len(), 4);
    }

    #[test]
    fn fixed_attachments_before_input_and_malformed_owner_or_pair_records_are_not_trusted() {
        let model = frames(SCENE).remove(0).mechanism;
        let first = model
            .groups
            .iter()
            .position(|group| group.kind == diada_core::GroupKind::Input)
            .unwrap();
        let attachment = model
            .groups
            .iter()
            .find(|group| {
                group.kind == diada_core::GroupKind::Attached
                    && group
                        .more
                        .is_some_and(|id| model.pairs[id.0].kind == diada_core::PairKind::Point)
            })
            .unwrap()
            .clone();
        let expected = crate::preview::trajectory_points(&model);
        let mut before = model.clone();
        before.groups.insert(first, attachment.clone());
        assert_eq!(crate::preview::trajectory_points(&before), expected);
        let mut fixed = model.clone();
        let ground = fixed.ground.unwrap();
        let mut fixed_group = attachment.clone();
        fixed_group.link_b = Some(ground);
        fixed.groups.push(fixed_group);
        assert_eq!(crate::preview::trajectory_points(&fixed), expected);
        let mut malformed = model.clone();
        let mut missing = attachment.clone();
        missing.more = Some(PairId(usize::MAX));
        malformed.groups = vec![model.groups[first].clone(), missing];
        assert!(crate::preview::trajectory_points(&malformed).is_empty());
        malformed.groups[1] = attachment;
        malformed.groups[1].link_b = Some(diada_core::LinkId(usize::MAX));
        assert!(crate::preview::trajectory_points(&malformed).is_empty());
        malformed.groups.clear();
        assert!(crate::preview::trajectory_points(&malformed).is_empty());
    }

    #[test]
    fn nonfinite_current_coordinates_do_not_reuse_the_previous_pass_cell() {
        let mut trace = MotionTrace::default();
        let id = PairId(9);
        trace.record_points([(id, Pos2::new(1.0, 2.0))], 0);
        trace.record_points([(id, Pos2::new(3.0, 4.0))], 1);
        trace.record_points([(id, Pos2::new(f32::INFINITY, 5.0))], 72);
        assert_eq!(trace.dots(), vec![Pos2::new(3.0, 4.0)]);
        trace.record_points([(id, Pos2::new(1.0, f32::NAN))], 73);
        assert!(trace.dots().is_empty());
        trace.record_points([(id, Pos2::new(5.0, 6.0))], 74);
        trace.clear();
        assert!(trace.dots().is_empty());
    }
}
