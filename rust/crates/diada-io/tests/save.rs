use diada_io::{Journal, SaveTransaction};
use std::{
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

fn directory(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "diada-io-{name}-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&path).unwrap();
    path
}

#[test]
fn save_replaces_the_current_journal_creates_backup_and_cleans_fragments() {
    let directory = directory("save");
    let target = directory.join("NEW.DI");
    fs::write(&target, b"old\n").unwrap();
    let transaction = SaveTransaction::begin(&target).unwrap();
    let current = Journal::from_bytes(b"current\r\n");
    let outcome = transaction.commit_current(&current).unwrap();
    assert_eq!(fs::read(&target).unwrap(), b"current\r\n");
    assert_eq!(fs::read(outcome.backup.unwrap()).unwrap(), b"old\r\n");
    assert!(!directory.join("NEW.STR").exists());
    assert!(!directory.join("NEW.ST1").exists());
    transaction.rollback().unwrap();
    assert_eq!(fs::read(&target).unwrap(), b"old\n");
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn new_dialogue_rollback_removes_only_the_created_target() {
    let directory = directory("new");
    let target = directory.join("new.di");
    let transaction = SaveTransaction::begin(&target).unwrap();
    assert!(
        transaction
            .commit_current(&Journal::from_bytes(b"new\r\n"))
            .unwrap()
            .backup
            .is_none()
    );
    transaction.rollback().unwrap();
    assert!(!target.exists());
    assert!(SaveTransaction::begin(directory.join("report.DIF")).is_err());
    fs::remove_dir_all(directory).unwrap();
}
