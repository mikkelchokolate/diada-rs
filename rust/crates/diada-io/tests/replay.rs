use diada_core::{
    AnalysisConfig, AnalysisMode, CalculationSnapshot, InputPosition, InputSeries, Name, PairId,
    Real48,
};
use diada_io::*;

const INPUT: &[u8] = include_bytes!("../../../../tests/fixtures/diada-v320-derived/SCENE.DI");
const OUTPUT: &[u8] = include_bytes!("../../../../tests/fixtures/diada-v320-derived/SCENE.DIF");

#[test]
fn complete_original_auto_series_is_generated_byte_for_byte_from_replayed_data() {
    assert_series(INPUT, OUTPUT, false);
}
fn assert_series(input: &[u8], output: &[u8], forces: bool) {
    let codec = TpNumbers;
    let document = EditableDialogue::open(input, "SCENE", &codec).unwrap();
    let mut model = document.loaded().mechanism.clone();
    let rate = codec.parse_real(b"1").unwrap();
    let mut snapshots = Vec::new();
    for index in 0..12 {
        let inputs = vec![InputPosition {
            position: codec
                .parse_real((index * 30).to_string().as_bytes())
                .unwrap(),
            other_position: diada_core::Real48::ZERO,
            rate,
            rate_derivative: diada_core::Real48::ZERO,
        }];
        model.calculate_position(&inputs).unwrap();
        if forces {
            model.calculate_forces().unwrap();
        }
        snapshots.push(CalculationSnapshot {
            position_index: index + 1,
            inputs,
            mechanism: model.clone(),
        });
    }
    let header = ReportHeader {
        mechanism_name: "SCENE".into(),
        source: InputSource::File,
        clock: ReportClock {
            year: 1999,
            month: 3,
            day: 10,
            hour: 10,
            minute: 38,
            second: 0,
        },
    };
    let report = position_reports(&snapshots, forces).unwrap();
    let generated = write_dif(&header, &report, &ReportSelection::default(), &codec).unwrap();
    assert_eq!(
        generated, output,
        "the entire original report, including CRLF/CP866/header/block boundaries/numeric rows, must match"
    );
}
#[test]
fn all_five_dyad_types_match_whole_original_auto_reports() {
    let cases: [(&[u8], &[u8]); 5] = [
        (
            include_bytes!("../../../../tests/fixtures/diada-v320-derived/dyad-01/SCENE.DI"),
            include_bytes!("../../../../tests/fixtures/diada-v320-derived/dyad-01/SCENE.DIF"),
        ),
        (
            include_bytes!("../../../../tests/fixtures/diada-v320-derived/dyad-02/SCENE.DI"),
            include_bytes!("../../../../tests/fixtures/diada-v320-derived/dyad-02/SCENE.DIF"),
        ),
        (
            include_bytes!("../../../../tests/fixtures/diada-v320-derived/dyad-03/SCENE.DI"),
            include_bytes!("../../../../tests/fixtures/diada-v320-derived/dyad-03/SCENE.DIF"),
        ),
        (
            include_bytes!("../../../../tests/fixtures/diada-v320-derived/dyad-04/SCENE.DI"),
            include_bytes!("../../../../tests/fixtures/diada-v320-derived/dyad-04/SCENE.DIF"),
        ),
        (
            include_bytes!("../../../../tests/fixtures/diada-v320-derived/dyad-05/SCENE.DI"),
            include_bytes!("../../../../tests/fixtures/diada-v320-derived/dyad-05/SCENE.DIF"),
        ),
    ];
    for (input, output) in cases {
        assert_series(input, output, false);
    }
}
#[test]
fn all_five_force_dyad_types_match_complete_original_reports_with_approved_empty_cache_fix() {
    let cases: [(&[u8], &[u8]); 5] = [
        (
            include_bytes!("../../../../tests/fixtures/diada-v320-derived/force-fixed-01/SCENE.DI"),
            include_bytes!(
                "../../../../tests/fixtures/diada-v320-derived/force-fixed-01/SCENE.DIF"
            ),
        ),
        (
            include_bytes!("../../../../tests/fixtures/diada-v320-derived/force-fixed-02/SCENE.DI"),
            include_bytes!(
                "../../../../tests/fixtures/diada-v320-derived/force-fixed-02/SCENE.DIF"
            ),
        ),
        (
            include_bytes!("../../../../tests/fixtures/diada-v320-derived/force-fixed-03/SCENE.DI"),
            include_bytes!(
                "../../../../tests/fixtures/diada-v320-derived/force-fixed-03/SCENE.DIF"
            ),
        ),
        (
            include_bytes!("../../../../tests/fixtures/diada-v320-derived/force-fixed-04/SCENE.DI"),
            include_bytes!(
                "../../../../tests/fixtures/diada-v320-derived/force-fixed-04/SCENE.DIF"
            ),
        ),
        (
            include_bytes!("../../../../tests/fixtures/diada-v320-derived/force-fixed-05/SCENE.DI"),
            include_bytes!(
                "../../../../tests/fixtures/diada-v320-derived/force-fixed-05/SCENE.DIF"
            ),
        ),
    ];
    for (input, output) in cases {
        assert_series(input, output, true);
    }
}

#[test]
fn two_sequential_force_dyads_reuse_reactions_and_match_the_complete_corrected_original_report() {
    assert_series(
        include_bytes!("../../../../tests/fixtures/diada-v320-derived/force-two-dyads-01/SCENE.DI"),
        include_bytes!(
            "../../../../tests/fixtures/diada-v320-derived/force-two-dyads-01/SCENE.DIF"
        ),
        true,
    );
}

#[test]
fn type_five_child_force_dyad_updates_scratch_before_parent_cached_reaction_and_matches_original() {
    assert_series(
        include_bytes!(
            "../../../../tests/fixtures/diada-v320-derived/force-child5-cache-01/SCENE.DI"
        ),
        include_bytes!(
            "../../../../tests/fixtures/diada-v320-derived/force-child5-cache-01/SCENE.DIF"
        ),
        true,
    );
}

#[test]
fn an_optional_angle_edit_cannot_silently_reassign_the_next_points_valid_numeric_answers() {
    let input =
        include_bytes!("../../../../tests/fixtures/diada-v320-derived/force-fixed-02/SCENE.DI");
    let original = EditableDialogue::open(input, "SCENE", &TpNumbers).unwrap();
    let distance = original
        .bindings()
        .iter()
        .find(|binding| matches!(binding.path, FieldPath::PointDistance(_)))
        .unwrap();
    let FieldPath::PointDistance(point) = distance.path else {
        unreachable!()
    };
    let angle = original
        .bindings()
        .iter()
        .find(|binding| binding.path == FieldPath::PointAngle(point))
        .unwrap();
    // A zero first distance has no first angle. The next point's accepted
    // distance/angle are .5/0 in the measured source dialogue.
    let mut zero = Vec::new();
    for (index, line) in original.current_journal().lines().iter().enumerate() {
        if index + 1 == angle.start_line {
            continue;
        }
        zero.extend_from_slice(if index + 1 == distance.start_line {
            b"0"
        } else {
            &line.bytes
        });
        zero.extend_from_slice(b"\r\n");
    }
    let mut document = EditableDialogue::open(&zero, "SCENE", &TpNumbers).unwrap();
    let accepted = document.loaded().mechanism.clone();
    let current = document
        .bindings()
        .iter()
        .find(|binding| binding.path == distance.path)
        .unwrap();
    let mut shifted = Vec::new();
    for (index, line) in document.current_journal().lines().iter().enumerate() {
        shifted.extend_from_slice(if index + 1 == current.start_line {
            b"0.25"
        } else {
            &line.bytes
        });
        shifted.extend_from_slice(b"\r\n");
    }
    // Raw replay alone succeeds: .5 becomes the new first angle, and 0
    // becomes the next distance, suppressing that point's angle in turn.
    let shifted_model = EditableDialogue::open(&shifted, "SCENE", &TpNumbers).unwrap();
    assert_ne!(shifted_model.loaded().mechanism.points, accepted.points);
    assert!(matches!(
        document.edit_text(&distance.path, "0.25", &TpNumbers),
        Err(ReplayError::DependentQuestionsChanged(_))
    ));
    assert_eq!(document.current_journal().to_bytes(), zero);
    assert_eq!(document.loaded().mechanism, accepted);
    let mut draft = document.begin_edit(&distance.path).unwrap();
    draft.next(&TpNumbers).unwrap();
    let DialogueState::Question { question, .. } = draft.answer("0.25", &TpNumbers).unwrap() else {
        panic!("the newly required angle must be asked")
    };
    assert_eq!(question.path, FieldPath::PointAngle(point));
}
#[test]
fn original_error_and_correction_history_replays_losslessly_and_generates_the_same_report() {
    let input = include_bytes!("../../../../tests/fixtures/diada-v320-derived/history-01/SCENE.DI");
    let output =
        include_bytes!("../../../../tests/fixtures/diada-v320-derived/history-01/SCENE.DIF");
    let document = EditableDialogue::open(input, "SCENE", &TpNumbers).unwrap();
    assert_eq!(document.current_journal().to_bytes(), input);
    let ground: Vec<_> = document
        .bindings()
        .iter()
        .filter(|binding| binding.path == FieldPath::Ground)
        .collect();
    assert_eq!(ground.len(), 2);
    assert_eq!(ground[0].value, AnswerValue::Name(LegacyName(*b"ZZ")));
    assert_eq!(ground[1].value, AnswerValue::Name(LegacyName(*b"0 ")));
    assert_series(input, output, false);
}

#[test]
fn original_mass_name_retry_blank_mass_rename_and_blank_additional_point_end_are_replayed_exactly()
{
    let input =
        include_bytes!("../../../../tests/fixtures/diada-v320-derived/history-force-01/SCENE.DI");
    let output =
        include_bytes!("../../../../tests/fixtures/diada-v320-derived/history-force-01/SCENE.DIF");
    let document = EditableDialogue::open(input, "SCENE", &TpNumbers).unwrap();
    assert_eq!(document.current_journal().to_bytes(), input);
    let model = &document.loaded().mechanism;
    let owner = model.link_by_name(Name(*b"2 ")).unwrap();
    let center = model.links[owner.0].mass.as_ref().unwrap().center.unwrap();
    assert_eq!(model.pairs[model.points[center.0].pair.0].name, Name::BLANK);
    assert_eq!(
        document
            .bindings()
            .iter()
            .filter(|binding| binding.path == FieldPath::MassCenter(owner))
            .count(),
        3
    );
    assert_series(input, output, true);
}

#[test]
fn declining_group_confirmation_rewinds_the_draft_to_structure_entry_without_changing_accepted_data()
 {
    let document = EditableDialogue::open(INPUT, "SCENE", &TpNumbers).unwrap();
    let saved = document.current_journal().to_bytes();
    let mut draft = document.begin_edit(&FieldPath::ConfirmGroups).unwrap();
    draft.next(&TpNumbers).unwrap();
    let DialogueState::Question { question, .. } = draft.answer("N", &TpNumbers).unwrap() else {
        panic!("structure entry resumes")
    };
    assert!(matches!(question.path, FieldPath::PairCommand(_)));
    assert!(
        decode_cp866(&draft.current_journal().to_bytes())
            .contains("_Возврат_к_началу_ввода_пар\r\n_Начало_описания_пар\r\n")
    );
    assert_eq!(document.current_journal().to_bytes(), saved);
}

#[test]
fn a_new_controller_consumes_answers_and_creates_a_complete_reopenable_legacy_dialogue() {
    let input =
        include_bytes!("../../../../tests/fixtures/diada-v320-derived/force-fixed-02/SCENE.DI");
    let existing = EditableDialogue::open(input, "SCENE", &TpNumbers).unwrap();
    let mut controller = DialogueController::new("SCENE");
    let mut state = controller.next(&TpNumbers).unwrap();
    for binding in existing.bindings() {
        let DialogueState::Question { question, .. } = &state else {
            panic!("new input completed before all original answers")
        };
        assert_eq!(question.path, binding.path);
        state = controller
            .answer(&binding.answer_text(existing.current_journal()), &TpNumbers)
            .unwrap();
    }
    let DialogueState::Complete(loaded) = state else {
        panic!("all original answers should complete new input")
    };
    assert_eq!(loaded.mechanism, existing.loaded().mechanism);
    assert_eq!(loaded.journal.to_bytes(), input);
    let reopened = EditableDialogue::open(&loaded.journal.to_bytes(), "SCENE", &TpNumbers).unwrap();
    assert_eq!(reopened.loaded().mechanism, loaded.mechanism);
}

#[test]
fn invalid_new_numeric_answer_does_not_change_the_draft_or_accepted_previous_answers() {
    let existing = EditableDialogue::open(INPUT, "SCENE", &TpNumbers).unwrap();
    let path = existing
        .bindings()
        .iter()
        .find(|binding| matches!(binding.path, FieldPath::LinkParameter(_)))
        .unwrap()
        .path
        .clone();
    let mut draft = existing.begin_edit(&path).unwrap();
    draft.next(&TpNumbers).unwrap();
    let prefix = draft.current_journal().to_bytes();
    let bindings = draft.bindings().to_vec();
    assert!(draft.answer("not-a-number", &TpNumbers).is_err());
    assert_eq!(draft.current_journal().to_bytes(), prefix);
    assert_eq!(draft.bindings(), bindings);
    assert_eq!(draft.question().unwrap().path, path);
    assert_eq!(existing.current_journal().to_bytes(), INPUT);
}

fn assert_custom_series(
    input: &[u8],
    output: &[u8],
    positions: usize,
    values: &[(&str, &str, &str, &str, &str)],
    forces: bool,
    selection: ReportSelection,
) {
    let document = EditableDialogue::open(input, "SCENE", &TpNumbers).unwrap();
    let model = &document.loaded().mechanism;
    let number = |text: &str| TpNumbers.parse_real(text.as_bytes()).unwrap();
    let inputs = values
        .iter()
        .map(|(name, start, step, rate, eps)| InputSeries {
            link: model
                .link_by_name(Name(LegacyName::from_text(name).unwrap().0))
                .unwrap(),
            start: number(start),
            other_start: Real48::ZERO,
            rate: number(rate),
            rate_derivative: number(eps),
            step: number(step),
        })
        .collect();
    let config = AnalysisConfig {
        positions,
        inputs,
        mode: if forces {
            AnalysisMode::Kinetostatics
        } else {
            AnalysisMode::Kinematics
        },
    };
    let snapshots = model.sample_kinematics(&config).unwrap();
    let header = ReportHeader {
        mechanism_name: "SCENE".into(),
        source: InputSource::File,
        clock: ReportClock {
            year: 1999,
            month: 3,
            day: 10,
            hour: 10,
            minute: 38,
            second: 0,
        },
    };
    let generated = write_dif(
        &header,
        &position_reports(&snapshots, forces).unwrap(),
        &selection,
        &TpNumbers,
    )
    .unwrap();
    assert_eq!(
        generated, output,
        "whole original custom-series report must match"
    );
}

#[test]
fn original_custom_series_cover_incomplete_blocks_negative_steps_rates_and_acceleration() {
    assert_custom_series(
        include_bytes!("../../../../tests/fixtures/diada-v320-derived/series-one-b/SCENE.DI"),
        include_bytes!("../../../../tests/fixtures/diada-v320-derived/series-one-b/SCENE.DIF"),
        1,
        &[("1", "137.5", "0", "-2.25", "1.125")],
        false,
        ReportSelection::default(),
    );
    assert_custom_series(
        include_bytes!("../../../../tests/fixtures/diada-v320-derived/series-seven-b/SCENE.DI"),
        include_bytes!("../../../../tests/fixtures/diada-v320-derived/series-seven-b/SCENE.DIF"),
        7,
        &[("1", "137.5", "-17.5", "-2.25", "1.125")],
        false,
        ReportSelection::default(),
    );
    assert_custom_series(
        include_bytes!("../../../../tests/fixtures/diada-v320-derived/series-thirteen-b/SCENE.DI"),
        include_bytes!("../../../../tests/fixtures/diada-v320-derived/series-thirteen-b/SCENE.DIF"),
        13,
        &[("1", "-179.75", "31.25", "0.875", "-0.375")],
        false,
        ReportSelection::default(),
    );
    assert_custom_series(
        include_bytes!("../../../../tests/fixtures/diada-v320-derived/series-force-one-b/SCENE.DI"),
        include_bytes!(
            "../../../../tests/fixtures/diada-v320-derived/series-force-one-b/SCENE.DIF"
        ),
        1,
        &[("1", "137.5", "0", "-2.25", "1.125")],
        true,
        ReportSelection::default(),
    );
}

#[test]
fn original_user_selected_report_objects_keep_the_unfiltered_first_leading_link() {
    let selection = ReportSelection {
        links: Some(vec![LegacyName(*b"2 ")]),
        points: Some(vec![LegacyName(*b"B ")]),
        reactions: None,
    };
    assert_custom_series(
        include_bytes!("../../../../tests/fixtures/diada-v320-derived/series-filtered-b/SCENE.DI"),
        include_bytes!("../../../../tests/fixtures/diada-v320-derived/series-filtered-b/SCENE.DIF"),
        7,
        &[("1", "137.5", "-17.5", "-2.25", "1.125")],
        false,
        selection,
    );
}

#[test]
fn two_independent_inputs_match_original_whole_analysis_with_distinct_series() {
    assert_custom_series(
        include_bytes!("../../../../tests/fixtures/diada-v320-derived/two-inputs-b/SCENE.DI"),
        include_bytes!("../../../../tests/fixtures/diada-v320-derived/two-inputs-b/SCENE.DIF"),
        7,
        &[
            ("1", "15", "-12.5", "1.25", "-0.5"),
            ("2", "70", "8.75", "-0.75", "0.25"),
        ],
        false,
        ReportSelection::default(),
    );
}

#[test]
fn edited_angle_retains_accepted_input_units_and_survives_save_reopen_exactly() {
    let codec = TpNumbers;
    let mut document = EditableDialogue::open(INPUT, "SCENE", &codec).unwrap();
    let path = FieldPath::GroundAngle(PairId(3));
    document.edit_text(&path, "12.3456789", &codec).unwrap();
    let accepted = document.loaded().mechanism.clone();
    let bytes = document.current_journal().to_bytes();
    let rebound = document
        .bindings()
        .iter()
        .rev()
        .find(|binding| binding.path == path)
        .unwrap();
    assert_eq!(
        document.current_journal().lines()[rebound.start_line - 1].bytes,
        b"12.3456789"
    );
    let reopened = EditableDialogue::open(&bytes, "SCENE", &codec).unwrap();
    assert_eq!(reopened.loaded().mechanism, accepted);
    assert_eq!(reopened.current_journal().to_bytes(), bytes);
    for _ in 0..8 {
        document = EditableDialogue::open(&document.current_journal().to_bytes(), "SCENE", &codec)
            .unwrap();
    }
    assert_eq!(document.loaded().mechanism, accepted);
    assert_eq!(document.current_journal().to_bytes(), bytes);
}

#[test]
fn failed_edit_preserves_the_accepted_model_and_complete_journal() {
    let codec = TpNumbers;
    let mut document = EditableDialogue::open(INPUT, "SCENE", &codec).unwrap();
    let accepted = document.loaded().mechanism.clone();
    let bytes = document.current_journal().to_bytes();
    let path = document
        .bindings()
        .iter()
        .find_map(|binding| match binding.path {
            FieldPath::LinkParameter(id) => Some(FieldPath::LinkParameter(id)),
            _ => None,
        })
        .unwrap();
    assert!(document.edit_text(&path, "invalid", &codec).is_err());
    assert_eq!(document.loaded().mechanism, accepted);
    assert_eq!(document.current_journal().to_bytes(), bytes);
}

#[test]
fn incomplete_journal_stops_at_a_typed_question_and_completion_is_an_independent_preview() {
    let codec = TpNumbers;
    let complete = EditableDialogue::open(INPUT, "SCENE", &codec).unwrap();
    let final_line = complete.bindings().last().unwrap().start_line - 1;
    let mut prefix = Vec::new();
    for line in &complete.current_journal().lines()[..final_line] {
        prefix.extend_from_slice(&line.bytes);
        prefix.extend_from_slice(b"\r\n");
    }
    let mut controller = DialogueController::resume(Journal::from_bytes(&prefix), "SCENE");
    let state = controller.next(&codec).unwrap();
    let DialogueState::Question { question, preview } = state else {
        panic!("confirmation is outstanding")
    };
    assert_eq!(question.path, FieldPath::ConfirmDimensions);
    assert_eq!(question.kind, QuestionKind::YesNo);
    assert_eq!(preview, complete.loaded().mechanism);
    let DialogueState::Complete(loaded) = controller.answer("Y", &codec).unwrap() else {
        panic!("all input is now complete")
    };
    assert_eq!(loaded.mechanism, complete.loaded().mechanism);
    let mut expected = INPUT.to_vec();
    let last_answer = expected.len() - 3;
    expected[last_answer] = b'Y';
    assert_eq!(loaded.journal.to_bytes(), expected);
}

#[test]
fn physical_save_uses_current_accepted_answers_and_backup_keeps_previous_dialogue() {
    let codec = TpNumbers;
    let mut document = EditableDialogue::open(INPUT, "SCENE", &codec).unwrap();
    document
        .edit_text(&FieldPath::GroundAngle(PairId(3)), "33.3333333", &codec)
        .unwrap();
    let directory =
        std::env::temp_dir().join(format!("diada-current-replay-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join("SCENE.DI");
    std::fs::write(&path, INPUT).unwrap();
    let transaction = SaveTransaction::begin(&path).unwrap();
    transaction
        .commit_current(document.current_journal())
        .unwrap();
    let reopened = EditableDialogue::open(&std::fs::read(&path).unwrap(), "SCENE", &codec).unwrap();
    assert_eq!(reopened.loaded().mechanism, document.loaded().mechanism);
    assert_eq!(
        std::fs::read(directory.join("Diada.bak")).unwrap(),
        legacy_backup_copy(INPUT)
    );
    assert!(!path.with_extension("STR").exists());
    assert!(!path.with_extension("ST1").exists());
    std::fs::remove_dir_all(directory).unwrap();
}
