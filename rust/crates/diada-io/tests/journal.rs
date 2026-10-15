use diada_io::{
    Journal, JournalWriter, LegacyName, decode_cp866, encode_cp866, legacy_backup_copy,
};

#[test]
fn all_cp866_bytes_round_trip_including_controls_and_drawing_characters() {
    let bytes: Vec<_> = (0..=255).collect();
    assert_eq!(encode_cp866(&decode_cp866(&bytes)).unwrap(), bytes);
    assert_eq!(
        encode_cp866("Ёё┌─┐").unwrap(),
        [0xf0, 0xf1, 0xda, 0xc4, 0xbf]
    );
    assert!(encode_cp866("🦀").is_err());
}

#[test]
fn archival_journal_is_lossless_and_its_initial_answers_replay() {
    let original = include_bytes!(
        "../../../../tests/fixtures/diada-v320/mechanisms/crank-slider-5link-dialog-backup.DI"
    );
    let journal = Journal::from_bytes(original);
    assert_eq!(journal.to_bytes(), original);
    let mut cursor = journal.cursor();
    cursor.expect_marker("_Начало_описания_пар").unwrap();
    assert_eq!(cursor.read_character().unwrap(), b'r');
    assert_eq!(cursor.read_name().unwrap(), LegacyName([b'0', b' ']));
    assert_eq!(cursor.read_name().unwrap(), LegacyName([b'1', b' ']));
    assert_eq!(cursor.read_name().unwrap(), LegacyName([b'O', b' ']));
}

#[test]
fn name_writer_handles_one_and_two_character_tokens_and_legacy_case_rules() {
    let mut writer = JournalWriter::new();
    writer.write_name(LegacyName([b'A', b' ']));
    writer.write_name(LegacyName([b'S', b'2']));
    writer.write_name(LegacyName::BLANK);
    let journal = writer.into_journal();
    let mut cursor = journal.cursor();
    assert_eq!(cursor.read_name().unwrap(), LegacyName([b'A', b' ']));
    assert_eq!(cursor.read_name().unwrap(), LegacyName([b'S', b'2']));
    assert!(cursor.read_name().unwrap().is_blank());
    let weird = Journal::from_bytes(b"{\r\n\r\n");
    assert_eq!(
        weird.cursor().read_name().unwrap(),
        LegacyName([b'[', b' '])
    );
}

#[test]
fn backspace_retries_and_end_of_file_are_explicit() {
    let journal = Journal::from_bytes(b"A\r\n\x08\r\nB\r\n2\r\n");
    let mut cursor = journal.cursor();
    assert_eq!(cursor.read_name().unwrap(), LegacyName([b'B', b'2']));
    assert!(
        cursor
            .read_name()
            .unwrap_err()
            .message
            .contains("keyboard continuation")
    );
    assert!(
        Journal::from_bytes(b"\x03\r\n")
            .cursor()
            .read_character()
            .is_err()
    );
}

#[test]
fn mixed_input_endings_are_retained_but_legacy_backup_is_dos_text() {
    let input = b"one\ntwo\rthree\r\nfour";
    assert_eq!(Journal::from_bytes(input).to_bytes(), input);
    assert_eq!(
        legacy_backup_copy(input),
        b"one\r\ntwo\r\nthree\r\nfour\r\n"
    );
    let mut long = vec![b'A'; 300];
    long.extend_from_slice(b"\r\n");
    assert_eq!(legacy_backup_copy(&long).len(), 257);
}
