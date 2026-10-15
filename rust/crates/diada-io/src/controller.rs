use crate::replay::replay_attempt;
use crate::{
    AnswerValue, DialogueNumbers, FieldPath, Journal, JournalWriter, LegacyName, LoadedDialogue,
    QuestionKind, ReplayError, ReplayQuestion, encode_cp866, load_dialogue,
};
use diada_core::Mechanism;

/// An accepted answer in the original input units. Numeric text is retained,
/// rather than reconstructed from rounded internal radians.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RawAnswer {
    Character(u8),
    Name(LegacyName),
    YesNo(bool),
    Number(Vec<u8>),
}
impl RawAnswer {
    pub fn from_text<C: DialogueNumbers>(
        kind: QuestionKind,
        text: &str,
        codec: &C,
    ) -> Result<Self, ReplayError> {
        let invalid = |message: &str| ReplayError::Unsupported {
            line: 0,
            message: message.into(),
        };
        let bytes = encode_cp866(text).map_err(|error| invalid(&error.to_string()))?;
        match kind {
            QuestionKind::PairCommand => {
                if bytes.len() == 1 {
                    Ok(Self::Character(bytes[0]))
                } else {
                    Err(invalid("enter exactly one pair command character"))
                }
            }
            QuestionKind::Name => LegacyName::from_text(text)
                .map(Self::Name)
                .map_err(|_| invalid("a legacy name has at most two ASCII characters")),
            QuestionKind::YesNo => match bytes.as_slice() {
                b"Y" | b"y" => Ok(Self::YesNo(true)),
                b"N" | b"n" => Ok(Self::YesNo(false)),
                _ => Err(invalid("enter Y or N")),
            },
            QuestionKind::Number => {
                codec
                    .parse_real(&bytes)
                    .map_err(|message| ReplayError::Numeric { line: 0, message })?;
                Ok(Self::Number(bytes))
            }
        }
    }
}

fn answer_journal(answer: &RawAnswer, kind: QuestionKind) -> Result<Journal, ReplayError> {
    let mut writer = JournalWriter::new();
    match (answer, kind) {
        (RawAnswer::Character(value), QuestionKind::PairCommand) => writer.write_character(*value),
        (RawAnswer::Name(value), QuestionKind::Name) => {
            if value.0.iter().any(|c| !(32..=127).contains(c)) {
                return Err(ReplayError::Unsupported {
                    line: 0,
                    message: "legacy names require one or two original ASCII characters".into(),
                });
            }
            writer.write_name(*value);
        }
        (RawAnswer::YesNo(value), QuestionKind::YesNo) => writer.write_yes_no(*value),
        (RawAnswer::Number(value), QuestionKind::Number) => {
            if value.contains(&b'\r') || value.contains(&b'\n') || value.is_empty() {
                return Err(ReplayError::Unsupported {
                    line: 0,
                    message: "a numeric answer must occupy one nonempty dialogue line".into(),
                });
            }
            writer.write_line(value);
        }
        _ => {
            return Err(ReplayError::Unsupported {
                line: 0,
                message: "answer type does not match the dialogue question".into(),
            });
        }
    }
    Ok(writer.into_journal())
}
fn replace_lines(journal: &Journal, start: usize, end: usize, replacement: &Journal) -> Journal {
    let mut writer = Vec::new();
    for (index, line) in journal.lines().iter().enumerate() {
        if index == start {
            writer.extend_from_slice(&replacement.to_bytes());
        }
        if index < start || index >= end {
            writer.extend_from_slice(&Journal::from_lines(vec![line.clone()]).to_bytes());
        }
    }
    if start == journal.lines().len() {
        writer.extend_from_slice(&replacement.to_bytes());
    }
    Journal::from_bytes(&writer)
}
fn kind(value: &AnswerValue) -> QuestionKind {
    match value {
        AnswerValue::Character(_) => QuestionKind::PairCommand,
        AnswerValue::Name(_) => QuestionKind::Name,
        AnswerValue::YesNo(_) => QuestionKind::YesNo,
        AnswerValue::Number(_) => QuestionKind::Number,
    }
}

#[derive(Clone, Debug)]
pub enum DialogueState {
    Question {
        question: ReplayQuestion,
        preview: Mechanism,
    },
    ReviewRequired {
        path: FieldPath,
        preview: Mechanism,
        bindings: Vec<crate::FieldBinding>,
    },
    Complete(LoadedDialogue),
}

/// Drives the very same journal replay used for file loading. A preview is an
/// independent model; submitting an answer never changes an accepted document.
#[derive(Clone, Debug)]
pub struct DialogueController {
    journal: Journal,
    title: String,
    pending: Option<ReplayQuestion>,
    preview: Option<Mechanism>,
    current_override: Option<(FieldPath, AnswerValue)>,
    bindings: Vec<crate::FieldBinding>,
}
impl DialogueController {
    pub fn new(title: impl Into<String>) -> Self {
        Self::resume(Journal::from_bytes(&[]), title)
    }
    pub fn resume(journal: Journal, title: impl Into<String>) -> Self {
        Self {
            journal,
            title: title.into(),
            pending: None,
            preview: None,
            current_override: None,
            bindings: Vec::new(),
        }
    }
    pub fn journal(&self) -> &Journal {
        &self.journal
    }
    pub fn current_journal(&self) -> &Journal {
        &self.journal
    }
    pub fn question(&self) -> Option<&ReplayQuestion> {
        self.pending.as_ref()
    }
    pub fn preview(&self) -> Option<&Mechanism> {
        self.preview.as_ref()
    }
    pub fn bindings(&self) -> &[crate::FieldBinding] {
        &self.bindings
    }
    pub fn rewind<C: DialogueNumbers>(
        &mut self,
        path: &FieldPath,
        codec: &C,
    ) -> Result<DialogueState, ReplayError> {
        let binding = self
            .bindings
            .iter()
            .rev()
            .find(|binding| &binding.path == path)
            .cloned()
            .ok_or_else(|| ReplayError::Unsupported {
                line: 0,
                message: format!("no accepted draft answer is bound to {path:?}"),
            })?;
        let prefix = Journal::from_lines(self.journal.lines()[..binding.start_line - 1].to_vec());
        let mut draft = Self::resume(prefix, self.title.clone());
        draft.current_override = Some((binding.path, binding.value));
        let state = draft.next(codec)?;
        *self = draft;
        Ok(state)
    }
    pub fn next<C: DialogueNumbers>(&mut self, codec: &C) -> Result<DialogueState, ReplayError> {
        // Markers are transport records, not user questions. The explicit bound
        // makes malformed or cyclic replay fail without hanging the interface.
        for _ in 0..65536 {
            let (preview, bindings, result) =
                replay_attempt(&self.journal, self.title.clone(), codec);
            self.bindings = bindings.clone();
            match result {
                Ok(()) => {
                    self.pending = None;
                    self.preview = Some(preview.clone());
                    return Ok(DialogueState::Complete(LoadedDialogue {
                        mechanism: preview,
                        bindings,
                        journal: self.journal.clone(),
                    }));
                }
                Err(ReplayError::NeedMarker(marker)) => {
                    let mut writer = JournalWriter::new();
                    writer
                        .write_text(&marker)
                        .map_err(|error| ReplayError::Unsupported {
                            line: self.journal.lines().len() + 1,
                            message: error.to_string(),
                        })?;
                    self.journal = replace_lines(
                        &self.journal,
                        self.journal.lines().len(),
                        self.journal.lines().len(),
                        &writer.into_journal(),
                    );
                }
                Err(ReplayError::NeedAnswer(mut question)) => {
                    if let Some((path, value)) = &self.current_override
                        && *path == question.path
                    {
                        question.current = Some(value.clone());
                    }
                    self.pending = Some(question.clone());
                    self.preview = Some(preview.clone());
                    return Ok(DialogueState::Question { question, preview });
                }
                Err(ReplayError::ReviewRequired(path)) => {
                    self.pending = None;
                    self.preview = Some(preview.clone());
                    return Ok(DialogueState::ReviewRequired {
                        path,
                        preview,
                        bindings,
                    });
                }
                Err(ReplayError::RestartStructure { keep_lines }) => {
                    let mut writer = JournalWriter::new();
                    writer
                        .write_text("_Возврат_к_началу_ввода_пар")
                        .map_err(|error| ReplayError::Unsupported {
                            line: keep_lines + 1,
                            message: error.to_string(),
                        })?;
                    self.journal = replace_lines(
                        &self.journal,
                        keep_lines,
                        self.journal.lines().len(),
                        &writer.into_journal(),
                    );
                }
                Err(error) => return Err(error),
            }
        }
        Err(ReplayError::Unsupported {
            line: self.journal.lines().len() + 1,
            message: "dialogue marker generation did not converge".into(),
        })
    }
    pub fn submit<C: DialogueNumbers>(
        &mut self,
        answer: RawAnswer,
        codec: &C,
    ) -> Result<DialogueState, ReplayError> {
        let question = self
            .pending
            .clone()
            .ok_or_else(|| ReplayError::Unsupported {
                line: 0,
                message: "request the current question before submitting an answer".into(),
            })?;
        let replacement = answer_journal(&answer, question.kind)?;
        let candidate = replace_lines(
            &self.journal,
            question.answer_start_line - 1,
            self.journal.lines().len(),
            &replacement,
        );
        let mut draft = Self::resume(candidate, self.title.clone());
        let state = draft.next(codec)?;
        *self = draft;
        Ok(state)
    }
    pub fn answer<C: DialogueNumbers>(
        &mut self,
        text: &str,
        codec: &C,
    ) -> Result<DialogueState, ReplayError> {
        let kind = self
            .pending
            .as_ref()
            .ok_or_else(|| ReplayError::Unsupported {
                line: 0,
                message: "request the current question before submitting an answer".into(),
            })?
            .kind;
        self.submit(RawAnswer::from_text(kind, text, codec)?, codec)
    }
}

/// Only replay-validated edits become accepted. Save uses the accepted raw
/// answer journal, including unchanged numeric spellings and input units.
#[derive(Clone, Debug)]
pub struct EditableDialogue {
    accepted: LoadedDialogue,
}
impl EditableDialogue {
    pub fn load<C: DialogueNumbers>(
        journal: &Journal,
        title: impl Into<String>,
        codec: &C,
    ) -> Result<Self, ReplayError> {
        Ok(Self {
            accepted: load_dialogue(journal, title, codec)?,
        })
    }
    pub fn open<C: DialogueNumbers>(
        bytes: &[u8],
        title: impl Into<String>,
        codec: &C,
    ) -> Result<Self, ReplayError> {
        Self::load(&Journal::from_bytes(bytes), title, codec)
    }
    pub fn from_loaded(accepted: LoadedDialogue) -> Self {
        Self { accepted }
    }
    pub fn accepted(&self) -> &LoadedDialogue {
        &self.accepted
    }
    pub fn loaded(&self) -> &LoadedDialogue {
        &self.accepted
    }
    pub fn bindings(&self) -> &[crate::FieldBinding] {
        &self.accepted.bindings
    }
    pub fn answer_text(&self, path: &FieldPath) -> Option<String> {
        self.accepted
            .bindings
            .iter()
            .rev()
            .find(|binding| &binding.path == path)
            .map(|binding| binding.answer_text(&self.accepted.journal))
    }
    pub fn current_journal(&self) -> &Journal {
        &self.accepted.journal
    }
    pub fn save(
        &self,
        path: impl AsRef<std::path::Path>,
    ) -> Result<crate::SaveOutcome, crate::SaveError> {
        crate::SaveTransaction::begin(path)?.commit_current(&self.accepted.journal)
    }
    pub fn journal_for_save(&self) -> &Journal {
        &self.accepted.journal
    }
    pub fn edit<C: DialogueNumbers>(
        &mut self,
        path: &FieldPath,
        answer: RawAnswer,
        codec: &C,
    ) -> Result<&LoadedDialogue, ReplayError> {
        let binding = self
            .accepted
            .bindings
            .iter()
            .rev()
            .find(|binding| &binding.path == path)
            .ok_or_else(|| ReplayError::Unsupported {
                line: 0,
                message: format!("no accepted raw answer is bound to {path:?}"),
            })?;
        let replacement = answer_journal(&answer, kind(&binding.value))?;
        let candidate = replace_lines(
            &self.accepted.journal,
            binding.start_line - 1,
            binding.end_line,
            &replacement,
        );
        let loaded = load_dialogue(&candidate, self.accepted.mechanism.title.clone(), codec)?;
        // Optional questions can consume a neighbouring valid numeric record
        // and still finish replay. Ask the dependent questions in a separate
        // controller rather than accepting that shifted interpretation.
        if !self
            .accepted
            .bindings
            .iter()
            .map(|binding| &binding.path)
            .eq(loaded.bindings.iter().map(|binding| &binding.path))
        {
            return Err(ReplayError::DependentQuestionsChanged(path.clone()));
        }
        self.accepted = loaded;
        Ok(&self.accepted)
    }
    /// Starts a structural change as a separate draft. Dependent answers are
    /// asked again because changing topology can change their meanings/order.
    pub fn begin_edit(&self, path: &FieldPath) -> Result<DialogueController, ReplayError> {
        let binding = self
            .accepted
            .bindings
            .iter()
            .rev()
            .find(|binding| &binding.path == path)
            .ok_or_else(|| ReplayError::Unsupported {
                line: 0,
                message: format!("no accepted raw answer is bound to {path:?}"),
            })?;
        let prefix =
            Journal::from_lines(self.accepted.journal.lines()[..binding.start_line - 1].to_vec());
        let mut draft = DialogueController::resume(prefix, self.accepted.mechanism.title.clone());
        draft.current_override = Some((binding.path.clone(), binding.value.clone()));
        Ok(draft)
    }
    pub fn edit_text<C: DialogueNumbers>(
        &mut self,
        path: &FieldPath,
        text: &str,
        codec: &C,
    ) -> Result<&LoadedDialogue, ReplayError> {
        let binding = self
            .accepted
            .bindings
            .iter()
            .rev()
            .find(|binding| &binding.path == path)
            .ok_or_else(|| ReplayError::Unsupported {
                line: 0,
                message: format!("no accepted raw answer is bound to {path:?}"),
            })?;
        self.edit(
            path,
            RawAnswer::from_text(kind(&binding.value), text, codec)?,
            codec,
        )
    }
    pub fn accept_complete(&mut self, complete: LoadedDialogue) {
        self.accepted = complete;
    }
}
