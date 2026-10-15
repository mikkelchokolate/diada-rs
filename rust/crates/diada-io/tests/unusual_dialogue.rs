//! Controller edge cases checked against measured original journals and the
//! recovered U09 structure dialogue. New structure cases exercise dialogue
//! semantics; they do not claim a newly measured numerical DOS oracle.
use diada_core::{CoreError, Mechanism, Name, PairKind};
use diada_io::*;

const HISTORY: &[u8] =
    include_bytes!("../../../../tests/fixtures/diada-v320-derived/history-01/SCENE.DI");
const FORCE_HISTORY: &[u8] =
    include_bytes!("../../../../tests/fixtures/diada-v320-derived/history-force-01/SCENE.DI");
const DYAD: &[u8] =
    include_bytes!("../../../../tests/fixtures/diada-v320-derived/dyad-01/SCENE.DI");
type Connection = (u8, &'static str, &'static str, &'static str);
const USER_CONNECTIONS: &[Connection] = &[
    (b'R', "0", "1", "O"),
    (b'R', "1", "2", "A"),
    (b'R', "2", "3", "B"),
    (b'P', "3", "0", "C"),
    (b'R', "2", "4", "D"),
    (b'R', "4", "5", "E"),
    (b'P', "5", "0", "F"),
];

fn pending(controller: &DialogueController, expected: FieldPath) -> &Mechanism {
    assert_eq!(controller.question().unwrap().path, expected);
    controller.preview().unwrap()
}

fn connections(rows: &[Connection]) -> DialogueController {
    let mut writer = JournalWriter::new();
    writer.write_text("_Начало_описания_пар").unwrap();
    for &(kind, a, b, name) in rows {
        writer.write_character(kind);
        for text in [a, b, name] {
            writer.write_name(LegacyName::from_text(text).unwrap());
        }
    }
    writer.write_character(b' ');
    writer.write_text("_Конец_описания_пар").unwrap();
    writer.write_yes_no(true);
    let mut controller = DialogueController::resume(writer.into_journal(), "SCENE");
    controller.next(&TpNumbers).unwrap();
    pending(&controller, FieldPath::Ground);
    controller
}

fn finish_original(
    controller: &mut DialogueController,
    original: &EditableDialogue,
    start: usize,
) -> LoadedDialogue {
    let mut finished = None;
    for binding in &original.bindings()[start..] {
        assert_eq!(controller.question().unwrap().path, binding.path);
        let state = controller
            .answer(&binding.answer_text(original.current_journal()), &TpNumbers)
            .unwrap();
        match state {
            DialogueState::Complete(loaded) => {
                assert!(finished.is_none());
                finished = Some(loaded);
            }
            DialogueState::Question { question, preview } => {
                assert_eq!(controller.question().unwrap(), &question);
                assert_eq!(controller.preview().unwrap(), &preview);
            }
            DialogueState::ReviewRequired { .. } => {
                panic!("measured original answers must finish without invented questions")
            }
        }
    }
    finished.expect("all measured original answers must complete the dialogue")
}

#[test]
fn the_original_rejection_and_rename_histories_can_be_entered_interactively_without_losing_records()
{
    // Provenance: history-01's unmodified DOS session measured unknown ground
    // and input, a rejected group, rejected confirmation and zero-length retry.
    // history-force-01 additionally measured duplicate mass names, an unknown
    // rename owner and a mass-centre rename to blank (approved cache correction
    // affects subsequent force calculation, not these original U09 questions).
    for bytes in [HISTORY, FORCE_HISTORY] {
        let original = EditableDialogue::open(bytes, "SCENE", &TpNumbers).unwrap();
        let mut controller = DialogueController::new("SCENE");
        controller.next(&TpNumbers).unwrap();
        let completed = finish_original(&mut controller, &original, 0);
        assert_eq!(completed.journal.to_bytes(), bytes);
        assert_eq!(completed.mechanism, original.loaded().mechanism);
        assert_eq!(completed.bindings, original.bindings());
    }
}

#[test]
fn the_users_seven_connections_accept_ground_zero_and_retry_invalid_input_names() {
    // U09 ConfirmPairs uses 3*(NLink-1)-2*NPair. These exact user rows have
    // six links and seven physical pair occurrences, hence W=1.
    let mut controller = connections(USER_CONNECTIONS);
    let before = pending(&controller, FieldPath::Ground).clone();
    assert_eq!((before.links.len(), before.pairs.len()), (6, 7));
    assert_eq!(before.formal_mobility(), 1);

    controller.answer("ZZ", &TpNumbers).unwrap();
    assert_eq!(pending(&controller, FieldPath::Ground), &before);
    controller.answer("0", &TpNumbers).unwrap();
    pending(&controller, FieldPath::Input(0));
    // U09:4Bxx retries unknown names, the ground itself, and a leading link
    // having no ground pair. Link 2 has A/B/D; ground 0 has O/C/F.
    for invalid in ["ZZ", "0", "2"] {
        controller.answer(invalid, &TpNumbers).unwrap();
        let preview = pending(&controller, FieldPath::Input(0));
        assert_eq!(preview.links, before.links);
        assert_eq!(preview.pairs, before.pairs);
    }
    controller.answer("1", &TpNumbers).unwrap();
    let accepted = pending(&controller, FieldPath::ForceMode);
    assert_eq!(
        accepted.links[accepted.ground.unwrap().0].name,
        Name(*b"0 ")
    );
    assert_eq!(accepted.inputs.len(), 1);
    assert_eq!(accepted.links[accepted.inputs[0].0].name, Name(*b"1 "));
    let inputs: Vec<_> = controller
        .bindings()
        .iter()
        .filter(|binding| binding.path == FieldPath::Input(0))
        .map(|binding| binding.answer_text(controller.journal()))
        .collect();
    assert_eq!(inputs, ["ZZ", "0", "2", "1"]);
}

#[test]
fn a_blank_ground_or_input_returns_to_pair_entry_without_deleting_the_existing_connections() {
    // Both U09 SelectGround and SelectInputLinks call ReturnToPairs on blank.
    // The outer pair phase resumes with Initialize=False and existing links.
    for cancel_at_input in [false, true] {
        let mut controller = connections(USER_CONNECTIONS);
        let original = controller.preview().unwrap().clone();
        if cancel_at_input {
            controller.answer("0", &TpNumbers).unwrap();
            pending(&controller, FieldPath::Input(0));
        }
        let prefix = controller.journal().to_bytes();
        controller.answer(" ", &TpNumbers).unwrap();
        assert_eq!(pending(&controller, FieldPath::PairCommand(7)), &original);
        assert!(controller.journal().to_bytes().starts_with(&prefix));
        controller.answer(" ", &TpNumbers).unwrap();
        pending(&controller, FieldPath::ConfirmPairs);
        controller.answer("Y", &TpNumbers).unwrap();
        pending(&controller, FieldPath::Ground);
        controller.answer("0", &TpNumbers).unwrap();
        controller.answer("1", &TpNumbers).unwrap();
        let resumed = pending(&controller, FieldPath::ForceMode);
        assert_eq!(resumed.pairs, original.pairs);
        assert_eq!(resumed.formal_mobility(), 1);
    }
}

#[test]
fn rejecting_a_measured_dyad_row_preserves_the_leading_groups_until_the_correct_row_is_supplied() {
    let original = EditableDialogue::open(HISTORY, "SCENE", &TpNumbers).unwrap();
    let first_group = original
        .bindings()
        .iter()
        .position(|binding| binding.path == FieldPath::GroupLink(0, 0))
        .unwrap();
    let binding = &original.bindings()[first_group];
    let prefix: Vec<_> = original.current_journal().lines()[..binding.start_line - 1]
        .iter()
        .flat_map(|line| {
            let mut bytes = line.bytes.clone();
            bytes.extend_from_slice(match line.ending {
                LineEnding::Crlf => b"\r\n",
                LineEnding::Cr => b"\r",
                LineEnding::Lf => b"\n",
                LineEnding::None => b"",
            });
            bytes
        })
        .collect();
    let journal = Journal::from_bytes(&prefix);
    let mut controller = DialogueController::resume(journal, "SCENE");
    controller.next(&TpNumbers).unwrap();
    let leading = pending(&controller, FieldPath::GroupLink(0, 0)).clone();
    assert_eq!(leading.groups.len(), 2);
    controller.answer("0", &TpNumbers).unwrap();
    controller.answer("3", &TpNumbers).unwrap();
    assert_eq!(pending(&controller, FieldPath::GroupLink(0, 0)), &leading);
    controller.answer("2", &TpNumbers).unwrap();
    controller.answer("3", &TpNumbers).unwrap();
    assert_eq!(
        pending(&controller, FieldPath::ConfirmGroups).groups.len(),
        3
    );
    assert_eq!(
        controller
            .bindings()
            .iter()
            .filter(|binding| matches!(binding.path, FieldPath::GroupLink(0, _)))
            .map(|binding| binding.answer_text(controller.journal()))
            .collect::<Vec<_>>(),
        ["0", "3", "2", "3"]
    );
}

#[test]
fn repeatedly_rewinding_completed_original_histories_keeps_raw_answers_and_current_model() {
    for bytes in [HISTORY, FORCE_HISTORY] {
        let original = EditableDialogue::open(bytes, "SCENE", &TpNumbers).unwrap();
        let mut controller =
            DialogueController::resume(original.current_journal().clone(), "SCENE");
        controller.next(&TpNumbers).unwrap();
        let numeric = original
            .bindings()
            .iter()
            .find(|binding| matches!(binding.path, FieldPath::LinkParameter(_)))
            .unwrap()
            .path
            .clone();
        for path in [
            FieldPath::Ground,
            numeric.clone(),
            FieldPath::Input(0),
            FieldPath::GroupLink(0, 0),
            numeric,
        ] {
            let index = original
                .bindings()
                .iter()
                .rposition(|binding| binding.path == path)
                .unwrap();
            controller.rewind(&path, &TpNumbers).unwrap();
            assert_eq!(
                controller.question().unwrap().current,
                Some(original.bindings()[index].value.clone())
            );
            let completed = finish_original(&mut controller, &original, index);
            assert_eq!(completed.journal.to_bytes(), bytes);
            assert_eq!(completed.mechanism, original.loaded().mechanism);
            assert_eq!(completed.bindings, original.bindings());
        }
    }
}

#[test]
fn legacy_keyboard_names_are_normalized_once_while_the_controller_keeps_the_raw_spelling() {
    // ENGINE ReadToken subtracts 32 for every accepted byte above 'Z'; '['
    // becomes ';', whereas '{' becomes '['. It is not Unicode uppercasing.
    for (raw, expected) in [
        ("aB", *b"AB"),
        ("q", *b"Q "),
        ("[", *b"; "),
        ("{", *b"[ "),
        ("~", *b"^ "),
    ] {
        let mut controller = DialogueController::new("NAMES");
        controller.next(&TpNumbers).unwrap();
        for answer in ["r", "0", "1", raw] {
            controller.answer(answer, &TpNumbers).unwrap();
        }
        let model = pending(&controller, FieldPath::PairCommand(1));
        assert_eq!(model.pairs[0].kind, PairKind::Revolute);
        assert_eq!(model.pairs[0].name, Name(expected));
        let binding = controller.bindings().last().unwrap();
        assert_eq!(binding.answer_text(controller.journal()), raw);
        assert_eq!(binding.value, AnswerValue::Name(LegacyName(expected)));
        let prefix = controller.journal().to_bytes();
        let mut reopened = DialogueController::resume(Journal::from_bytes(&prefix), "NAMES");
        reopened.next(&TpNumbers).unwrap();
        assert_eq!(reopened.preview(), controller.preview());
        assert_eq!(reopened.journal().to_bytes(), prefix);
    }
}

#[test]
fn a_pair_rename_rejection_keeps_the_existing_connection_and_confirmation_replaces_it() {
    // U09's pair-entry phase asks before a different name replaces an existing
    // connection. A declined rename logs the rejection and returns to entry.
    for (answer, expected) in [("N", *b"O "), ("Y", *b"Q ")] {
        let mut controller = DialogueController::new("RENAME");
        controller.next(&TpNumbers).unwrap();
        for text in ["R", "0", "1", "O", "R", "1", "0", "Q"] {
            controller.answer(text, &TpNumbers).unwrap();
        }
        pending(&controller, FieldPath::RenamePairConfirm(1));
        controller.answer(answer, &TpNumbers).unwrap();
        let model = pending(&controller, FieldPath::PairCommand(2));
        assert_eq!((model.links.len(), model.pairs.len()), (2, 1));
        assert_eq!(model.pairs[0].name, Name(expected));
        assert_eq!(model.formal_mobility(), 1);
        assert!(
            controller
                .bindings()
                .iter()
                .any(|binding| binding.path == FieldPath::RenamePairConfirm(1))
        );
        let mut reopened = DialogueController::resume(controller.journal().clone(), "RENAME");
        reopened.next(&TpNumbers).unwrap();
        assert_eq!(reopened.preview(), controller.preview());
        assert_eq!(
            reopened.journal().to_bytes(),
            controller.journal().to_bytes()
        );
    }
}

#[test]
fn nonpositive_mobility_is_an_explicit_transactional_error_without_losing_the_pending_ground() {
    // W<=0 leads outside the defined supported calculation paths. The agreed
    // Rust behavior is an explicit error rather than original memory garbage.
    let cases: &[(&[Connection], isize)] = &[
        (
            &[
                (b'R', "0", "1", "O"),
                (b'R', "1", "2", "A"),
                (b'R', "2", "0", "B"),
            ],
            0,
        ),
        (
            &[
                (b'R', "0", "1", "O"),
                (b'R', "1", "2", "A"),
                (b'R', "2", "3", "B"),
                (b'R', "3", "0", "C"),
                (b'R', "0", "2", "D"),
            ],
            -1,
        ),
    ];
    for &(rows, mobility) in cases {
        let mut controller = connections(rows);
        assert_eq!(controller.preview().unwrap().formal_mobility(), mobility);
        let journal = controller.journal().to_bytes();
        let model = controller.preview().unwrap().clone();
        let question = controller.question().unwrap().clone();
        let bindings = controller.bindings().to_vec();
        for _ in 0..3 {
            assert!(matches!(
                controller.answer("0", &TpNumbers),
                Err(ReplayError::Core(CoreError::InvalidStructure(
                    "formal mobility must be positive"
                )))
            ));
            assert_eq!(controller.journal().to_bytes(), journal);
            assert_eq!(controller.preview().unwrap(), &model);
            assert_eq!(controller.question().unwrap(), &question);
            assert_eq!(controller.bindings(), bindings);
        }
    }
}

#[test]
fn measured_histories_survive_physical_save_and_reopen_with_rejected_answers_and_blank_renames() {
    let directory = std::env::temp_dir().join(format!(
        "diada-unusual-journal-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&directory).unwrap();
    for (index, bytes) in [HISTORY, FORCE_HISTORY, DYAD].into_iter().enumerate() {
        let document = EditableDialogue::open(bytes, "SCENE", &TpNumbers).unwrap();
        let path = directory.join(format!("HISTORY{index}.DI"));
        assert!(document.save(&path).unwrap().backup.is_none());
        let saved = std::fs::read(&path).unwrap();
        assert_eq!(saved, bytes);
        let reopened = EditableDialogue::open(&saved, "SCENE", &TpNumbers).unwrap();
        assert_eq!(reopened.loaded().mechanism, document.loaded().mechanism);
        assert_eq!(reopened.bindings(), document.bindings());
        // Mass/force/additional points are retained but do not count as planar
        // R/P constraints; both measured histories still have formal W=1.
        assert_eq!(reopened.loaded().mechanism.formal_mobility(), 1);
        assert!(!path.with_extension("STR").exists());
        assert!(!path.with_extension("ST1").exists());
    }
    let actual_directory = std::fs::canonicalize(&directory).unwrap();
    assert_eq!(
        actual_directory.parent(),
        Some(
            std::fs::canonicalize(std::env::temp_dir())
                .unwrap()
                .as_path()
        )
    );
    std::fs::remove_dir_all(actual_directory).unwrap();
}
