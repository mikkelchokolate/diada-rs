//! Historical DIADA dialogue and report files.
//!
//! `.DI` is an executable dialogue journal, not a new project format. Raw
//! journals are retained losslessly; saving an edited model emits a fresh,
//! complete journal so its next replay reflects the current values.

mod codec;
mod controller;
mod encoding;
mod journal;
mod replay;
mod report;
mod save;
mod snapshots;

pub use codec::TpNumbers;
pub use controller::{DialogueController, DialogueState, EditableDialogue, RawAnswer};
pub use encoding::{EncodingError, decode_cp866, encode_cp866};
pub use journal::{
    Journal, JournalError, JournalLine, JournalWriter, LegacyCursor, LegacyName, LineEnding,
};
pub use replay::{
    AnswerValue, DialogueNumbers, FieldBinding, FieldPath, LoadedDialogue, QuestionKind,
    ReplayError, ReplayQuestion, legacy_link_order, load_dialogue,
};
pub use report::{
    BalanceMoment, InputSource, LinkReport, NumberFormatter, PointReport, PositionReport,
    ReactionApplication, ReactionReport, ReportClock, ReportError, ReportHeader, ReportIdentity,
    ReportNumber, ReportSelection, write_dif, write_dif_with_identity,
};
pub use save::{SaveError, SaveOutcome, SaveTransaction, legacy_backup_copy};
pub use snapshots::{position_report, position_reports};
