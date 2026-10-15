use crate::{Journal, JournalError, LegacyCursor, LegacyName};
use diada_core::*;
use std::fmt;

pub trait DialogueNumbers {
    fn parse_real(&self, bytes: &[u8]) -> Result<Real48, String>;
    fn scientific_real(&self, value: Real48) -> Result<Vec<u8>, String>;
    fn degrees_to_radians(&self, value: Real48) -> Result<Real48, String>;
    fn radians_to_degrees(&self, value: Real48) -> Result<Real48, String>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FieldPath {
    PairCommand(usize),
    PairLink(usize, usize),
    PairName(usize),
    RenamePairConfirm(usize),
    ConfirmPairs,
    Ground,
    Input(usize),
    ForceMode,
    MassCenter(LinkId),
    MassRenameOwner(usize),
    ConfirmMassCenters,
    ForceOwner(usize),
    ForceName(usize),
    ConfirmForcePoints,
    AdditionalPoints,
    PointOwner(usize),
    PointName(usize),
    GroupLink(usize, usize),
    ConfirmGroups,
    GroundX(PairId),
    GroundY(PairId),
    GroundAngle(PairId),
    LinkOnGuide(LinkId),
    LinkParameter(LinkId),
    SignedOffset(LinkId),
    Assembly(GroupId),
    InputGuideAngle(LinkId),
    PointOnGuide(PointId),
    PointDistance(PointId),
    PointAngle(PointId),
    Mass(LinkId),
    Inertia(LinkId),
    ConfirmMassValues,
    MassEditOwner(usize),
    ForceMagnitude(PointId),
    ForceAngle(PointId),
    Moment(LinkId),
    ConfirmDimensions,
    ConfirmForces,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QuestionKind {
    PairCommand,
    Name,
    YesNo,
    Number,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AnswerValue {
    Character(u8),
    Name(LegacyName),
    YesNo(bool),
    Number(Real48),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FieldBinding {
    pub path: FieldPath,
    pub start_line: usize,
    pub end_line: usize,
    pub value: AnswerValue,
}
impl FieldBinding {
    /// Text for an editor in original input units. Numeric spelling is read
    /// from its accepted record, so an untouched angle is never reconverted.
    pub fn answer_text(&self, journal: &Journal) -> String {
        match self.value {
            AnswerValue::Number(_) => journal
                .lines()
                .get(self.start_line - 1)
                .map(|line| line.text())
                .unwrap_or_default(),
            AnswerValue::Name(_) => {
                let lines = &journal.lines()
                    [self.start_line.saturating_sub(1)..self.end_line.min(journal.lines().len())];
                let mut chars = lines
                    .iter()
                    .map(|line| line.bytes.first().copied().unwrap_or(b'\r'));
                loop {
                    let Some(first) = chars
                        .by_ref()
                        .find(|value| *value == b' ' || (32..=127).contains(value))
                    else {
                        return String::new();
                    };
                    if first == b' ' {
                        return String::new();
                    }
                    let Some(second) = chars
                        .by_ref()
                        .find(|value| *value == 8 || *value == b'\r' || (32..=127).contains(value))
                    else {
                        return crate::decode_cp866(&[first]);
                    };
                    if second == 8 {
                        continue;
                    }
                    let spelling = [first, second];
                    return crate::decode_cp866(
                        &spelling[..if second == b'\r' || second == b' ' {
                            1
                        } else {
                            2
                        }],
                    );
                }
            }
            AnswerValue::YesNo(value) => {
                if value {
                    "Y".into()
                } else {
                    "N".into()
                }
            }
            AnswerValue::Character(value) => crate::decode_cp866(&[value]),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReplayQuestion {
    pub path: FieldPath,
    pub kind: QuestionKind,
    pub text: String,
    pub units: Option<&'static str>,
    pub objects: Vec<LegacyName>,
    pub current: Option<AnswerValue>,
    pub answer_start_line: usize,
}
#[derive(Clone, Debug)]
pub struct LoadedDialogue {
    pub mechanism: Mechanism,
    pub bindings: Vec<FieldBinding>,
    pub journal: Journal,
}
#[derive(Clone, Debug)]
pub enum ReplayError {
    Journal(JournalError),
    Core(CoreError),
    Numeric { line: usize, message: String },
    NeedMarker(String),
    NeedAnswer(ReplayQuestion),
    ReviewRequired(FieldPath),
    DependentQuestionsChanged(FieldPath),
    RestartStructure { keep_lines: usize },
    Unsupported { line: usize, message: String },
}
impl fmt::Display for ReplayError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for ReplayError {}
impl From<JournalError> for ReplayError {
    fn from(e: JournalError) -> Self {
        Self::Journal(e)
    }
}
impl From<CoreError> for ReplayError {
    fn from(e: CoreError) -> Self {
        Self::Core(e)
    }
}

pub fn legacy_link_order(model: &Mechanism) -> Vec<LinkId> {
    let mut order: Vec<_> = (0..model.links.len()).map(LinkId).collect();
    for (target, id) in model
        .ground
        .into_iter()
        .chain(model.inputs.iter().copied())
        .enumerate()
    {
        if let Some(position) = order.iter().position(|value| *value == id) {
            order.swap(target, position);
        }
    }
    order
}

struct Reader<'a, C> {
    cursor: LegacyCursor<'a>,
    codec: &'a C,
    bindings: Vec<FieldBinding>,
}
impl<C: DialogueNumbers> Reader<'_, C> {
    fn marker(&mut self, marker: &str) -> Result<(), ReplayError> {
        self.cursor.skip_empty_lines();
        if self.cursor.peek().is_none() {
            return Err(ReplayError::NeedMarker(marker.to_owned()));
        }
        self.cursor.expect_marker(marker)?;
        Ok(())
    }
    fn question(
        &self,
        path: FieldPath,
        kind: QuestionKind,
        text: impl Into<String>,
        units: Option<&'static str>,
        objects: &[Name],
        current: Option<AnswerValue>,
    ) -> ReplayQuestion {
        ReplayQuestion {
            path,
            kind,
            text: text.into(),
            units,
            objects: objects.iter().map(|n| LegacyName(n.0)).collect(),
            current,
            answer_start_line: self.cursor.position() + 1,
        }
    }
    fn ask(&self, question: &ReplayQuestion) -> Result<usize, ReplayError> {
        if self.cursor.peek().is_none() {
            Err(ReplayError::NeedAnswer(question.clone()))
        } else {
            Ok(self.cursor.position() + 1)
        }
    }
    fn bind(&mut self, q: ReplayQuestion, start: usize, value: AnswerValue) {
        self.bindings.push(FieldBinding {
            path: q.path,
            start_line: start,
            end_line: self.cursor.position(),
            value,
        });
    }
    fn character(&mut self, path: FieldPath, text: &str) -> Result<u8, ReplayError> {
        let q = self.question(path, QuestionKind::PairCommand, text, None, &[], None);
        let start = self.ask(&q)?;
        let value = self.cursor.read_character()?;
        self.bind(q, start, AnswerValue::Character(value));
        Ok(value)
    }
    fn name(
        &mut self,
        path: FieldPath,
        text: impl Into<String>,
        objects: &[Name],
    ) -> Result<Name, ReplayError> {
        // Single-character names leave an extra empty ReadLn record in the
        // archival stream. Do not turn that record into a spurious question.
        self.cursor.skip_empty_lines();
        let q = self.question(path, QuestionKind::Name, text, None, objects, None);
        let start = self.ask(&q)?;
        let value = match self.cursor.read_name() {
            Ok(value) => value,
            Err(error) if error.message.starts_with("unexpected end") => {
                return Err(ReplayError::NeedAnswer(q));
            }
            Err(error) => return Err(error.into()),
        };
        self.bind(q, start, AnswerValue::Name(value));
        Ok(Name(value.0))
    }
    fn yes(
        &mut self,
        path: FieldPath,
        text: impl Into<String>,
        objects: &[Name],
    ) -> Result<bool, ReplayError> {
        self.cursor.skip_empty_lines();
        let q = self.question(path, QuestionKind::YesNo, text, None, objects, None);
        let start = self.ask(&q)?;
        let value = match self.cursor.read_yes_no() {
            Ok(value) => value,
            Err(error) if error.message.starts_with("unexpected end") => {
                return Err(ReplayError::NeedAnswer(q));
            }
            Err(error) => return Err(error.into()),
        };
        self.bind(q, start, AnswerValue::YesNo(value));
        Ok(value)
    }
    fn real(
        &mut self,
        path: FieldPath,
        text: impl Into<String>,
        units: &'static str,
        objects: &[Name],
        current: Real48,
    ) -> Result<Real48, ReplayError> {
        let q = self.question(
            path,
            QuestionKind::Number,
            text,
            Some(units),
            objects,
            Some(AnswerValue::Number(current)),
        );
        let start = self.ask(&q)?;
        let raw = self.cursor.read_line()?;
        // Original keyboard errors are retried before logging an accepted
        // number. File Val errors do not retry the next record as this field;
        // their undefined/stale continuation is an explicit load error.
        let value = self
            .codec
            .parse_real(&raw[..raw.len().min(255)])
            .map_err(|message| ReplayError::Numeric {
                line: start,
                message,
            })?;
        self.bind(q, start, AnswerValue::Number(value));
        Ok(value)
    }
    fn radian(
        &mut self,
        path: FieldPath,
        text: impl Into<String>,
        objects: &[Name],
        current: Real48,
    ) -> Result<Real48, ReplayError> {
        let degrees =
            self.codec
                .radians_to_degrees(current)
                .map_err(|message| ReplayError::Numeric {
                    line: self.cursor.position() + 1,
                    message,
                })?;
        let input = self.real(path, text, "град", objects, degrees)?;
        self.codec
            .degrees_to_radians(input)
            .map_err(|message| ReplayError::Numeric {
                line: self.cursor.position(),
                message,
            })
    }
}

pub fn load_dialogue<C: DialogueNumbers>(
    journal: &Journal,
    title: impl Into<String>,
    codec: &C,
) -> Result<LoadedDialogue, ReplayError> {
    let (model, bindings, result) = replay_attempt(journal, title.into(), codec);
    result?;
    Ok(LoadedDialogue {
        mechanism: model,
        bindings,
        journal: journal.clone(),
    })
}
pub(crate) fn replay_attempt<C: DialogueNumbers>(
    journal: &Journal,
    title: String,
    codec: &C,
) -> (Mechanism, Vec<FieldBinding>, Result<(), ReplayError>) {
    let mut reader = Reader {
        cursor: journal.cursor(),
        codec,
        bindings: Vec::new(),
    };
    let mut model = Mechanism {
        title,
        ..Mechanism::default()
    };
    let result = (|| {
        read_structure(&mut reader, &mut model)?;
        read_dimensions(&mut reader, &mut model)?;
        if model.force_analysis {
            read_forces(&mut reader, &mut model)?;
        }
        Ok(())
    })();
    (model, reader.bindings, result)
}

fn read_structure<C: DialogueNumbers>(
    r: &mut Reader<'_, C>,
    m: &mut Mechanism,
) -> Result<(), ReplayError> {
    let mut command_index = 0;
    let mut initial_marker = true;
    'pairs: loop {
        if initial_marker {
            r.marker("_Начало_описания_пар")?;
            initial_marker = false;
        }
        loop {
            let command=r.character(FieldPath::PairCommand(command_index),"Тип пары: R — вращательная, P — поступательная, E — удалить по имени; пробел — завершить")?.to_ascii_uppercase();
            if command == b' ' {
                break;
            }
            if command == b'\r' {
                continue;
            }
            if command == b'E' {
                let name = r.name(
                    FieldPath::PairName(command_index),
                    "Имя удаляемой пары",
                    &[],
                )?;
                if m.pair_by_name(name).is_some() {
                    m.delete_pairs_named(name)?;
                }
                command_index += 1;
                continue;
            }
            if command != b'R' && command != b'P' {
                continue;
            }
            let a = r.name(
                FieldPath::PairLink(command_index, 0),
                "Первое звено пары",
                &[],
            )?;
            if a == Name::BLANK {
                break;
            }
            let b = r.name(
                FieldPath::PairLink(command_index, 1),
                "Второе звено пары",
                &[a],
            )?;
            if b == Name::BLANK {
                break;
            }
            let name = r.name(
                FieldPath::PairName(command_index),
                "Имя кинематической пары",
                &[a, b],
            )?;
            if name == Name::BLANK {
                break;
            }
            command_index += 1;
            if a == b {
                continue;
            }
            let kind = PairKind::from_code(command)?;
            if m.pairs.iter().any(|pair| {
                pair.name == name && pair.kind != kind && {
                    let ends = [m.links[pair.links[0].0].name, m.links[pair.links[1].0].name];
                    ends != [a, b] && ends != [b, a]
                }
            }) {
                continue;
            }
            let mut rejected = false;
            while let Some(old_name) = m
                .pairs
                .iter()
                .find(|pair| {
                    pair.name != name
                        && matches!(pair.kind, PairKind::Revolute | PairKind::Prismatic)
                        && {
                            let ends =
                                [m.links[pair.links[0].0].name, m.links[pair.links[1].0].name];
                            ends == [a, b] || ends == [b, a]
                        }
                })
                .map(|pair| pair.name)
            {
                if !r.yes(
                    FieldPath::RenamePairConfirm(command_index - 1),
                    "Звенья уже соединены другой парой. Подтвердить переименование?",
                    &[old_name, name, a, b],
                )? {
                    rejected = true;
                    break;
                }
                m.delete_pairs_named(old_name)?;
            }
            if rejected {
                continue;
            }
            let aa = m.add_link(a);
            let bb = m.add_link(b);
            if let Some(existing) = m
                .pairs
                .iter_mut()
                .find(|p| p.name == name && (p.links == [aa, bb] || p.links == [bb, aa]))
            {
                existing.kind = kind;
                existing.links = [aa, bb];
            } else {
                m.add_pair(kind, name, [aa, bb])?;
            }
        }
        r.marker("_Конец_описания_пар")?;
        if !r.yes(
            FieldPath::ConfirmPairs,
            "Кинематические пары перечислены правильно?",
            &[],
        )? {
            continue;
        }
        let (ground_name, ground) = loop {
            r.marker("_Ввод_стойки")?;
            let name = r.name(FieldPath::Ground, "Имя звена стойки", &[])?;
            if name == Name::BLANK {
                continue 'pairs;
            }
            if let Some(id) = m.link_by_name(name) {
                break (name, id);
            }
        };
        r.marker("_Ввод_начального_звена")?;
        let mobility = m.formal_mobility();
        if mobility < 1 {
            return Err(CoreError::InvalidStructure("formal mobility must be positive").into());
        }
        'input_retry: loop {
            let mut inputs = Vec::new();
            for i in 0..mobility as usize {
                let name = r.name(FieldPath::Input(i), "Имя ведущего звена", &[ground_name])?;
                if name == Name::BLANK {
                    continue 'pairs;
                }
                let Some(id) = m.link_by_name(name) else {
                    continue 'input_retry;
                };
                if id == ground || inputs.contains(&id) {
                    continue 'input_retry;
                }
                inputs.push(id);
            }
            match m.set_ground_and_inputs(ground, inputs) {
                Ok(()) => break,
                Err(CoreError::InvalidStructure("input link must connect to ground")) => continue,
                Err(error) => return Err(error.into()),
            }
        }
        r.marker("_Ввод_дополнительных_точек")?;
        if !m.force_analysis {
            m.force_analysis = r.yes(
                FieldPath::ForceMode,
                "Будет проводиться силовой анализ?",
                &[],
            )?;
        }
        if m.force_analysis {
            read_mass_and_force_names(r, m)?;
        }
        if r.yes(
            FieldPath::AdditionalPoints,
            "Есть дополнительные точки для исследования?",
            &[],
        )? {
            let mut index = 0;
            'additional: loop {
                let owner_name = r.name(
                    FieldPath::PointOwner(index),
                    "Звено дополнительной точки; пробел — закончить",
                    &[],
                )?;
                if owner_name == Name::BLANK {
                    break;
                }
                let Some(owner) = m.link_by_name(owner_name) else {
                    index += 1;
                    continue;
                };
                loop {
                    let name = r.name(
                        FieldPath::PointName(index),
                        "Имя дополнительной точки",
                        &[owner_name],
                    )?;
                    if name == Name::BLANK {
                        break 'additional;
                    }
                    if m.pair_by_name(name).is_none() {
                        m.add_point(owner, PairKind::Point, name)?;
                        break;
                    }
                }
                index += 1;
            }
        }
        r.cursor.skip_empty_lines();
        if r.cursor
            .peek()
            .is_some_and(|line| line.text() == "_Возврат_к_началу_ввода_пар")
        {
            r.marker("_Возврат_к_началу_ввода_пар")?;
            initial_marker = true;
            continue;
        }
        let structure_restart_line = r.cursor.position();
        r.marker("_Конец_ввода_пар,_звеньев_и_точек")?;
        r.marker("_Начало_разбивки_механизма_на_группы")?;
        m.prepare_initial_groups()?;
        let needed = (m.links.len() - m.inputs.len() - 1) / 2;
        for i in 0..needed {
            loop {
                let a = r.name(
                    FieldPath::GroupLink(i, 0),
                    "Первое звено очередной диады",
                    &[],
                )?;
                if a == Name::BLANK {
                    initial_marker = true;
                    continue 'pairs;
                }
                let b = r.name(
                    FieldPath::GroupLink(i, 1),
                    "Второе звено очередной диады",
                    &[a],
                )?;
                if b == Name::BLANK {
                    initial_marker = true;
                    continue 'pairs;
                }
                let (Some(a), Some(b)) = (m.link_by_name(a), m.link_by_name(b)) else {
                    continue;
                };
                let mut candidate = m.clone();
                match candidate.append_dyad(a, b) {
                    Ok(_) => {
                        *m = candidate;
                        break;
                    }
                    Err(CoreError::InvalidStructure("three prismatic pairs in a dyad")) => {
                        return Err(
                            CoreError::InvalidStructure("three prismatic pairs in a dyad").into(),
                        );
                    }
                    Err(CoreError::InvalidStructure(_)) => continue,
                    Err(error) => return Err(error.into()),
                }
            }
        }
        if !r.yes(
            FieldPath::ConfirmGroups,
            "Порядок структурных групп задан правильно?",
            &[],
        )? {
            return Err(ReplayError::RestartStructure {
                keep_lines: structure_restart_line,
            });
        }
        r.marker("_Конец_описания_структуры_механизма")?;
        return Ok(());
    }
}

fn read_mass_and_force_names<C: DialogueNumbers>(
    r: &mut Reader<'_, C>,
    m: &mut Mechanism,
) -> Result<(), ReplayError> {
    for owner in legacy_link_order(m).into_iter().skip(1) {
        let link_name = m.links[owner.0].name;
        let name = loop {
            let name = r.name(
                FieldPath::MassCenter(owner),
                "Имя центра масс; пробел — звено без массы",
                &[link_name],
            )?;
            if name == Name::BLANK || m.pair_by_name(name).is_none() {
                break name;
            }
        };
        if name != Name::BLANK {
            let id = m.add_point(owner, PairKind::MassCenter, name)?;
            m.links[owner.0].mass = Some(MassProperties {
                mass: Real48::ZERO,
                inertia: Real48::ZERO,
                center: Some(id),
            });
        }
    }
    if !r.yes(
        FieldPath::ConfirmMassCenters,
        "Центры масс обозначены правильно?",
        &[],
    )? {
        let mut index = 0;
        loop {
            let owner_name = r.name(
                FieldPath::MassRenameOwner(index),
                "Имя звена для изменения центра масс; пробел — закончить",
                &[],
            )?;
            if owner_name == Name::BLANK {
                break;
            }
            let Some(owner) = m.link_by_name(owner_name) else {
                index += 1;
                continue;
            };
            let name = loop {
                let name = r.name(
                    FieldPath::MassCenter(owner),
                    "Новое имя центра масс",
                    &[owner_name],
                )?;
                // U09:528B checks the nonblank owner name again. A blank point
                // name therefore legitimately reaches the existing reference
                // rename (or a newly allocated mass record), without a nil read.
                if m.pair_by_name(name).is_none() {
                    break name;
                }
            };
            if let Some(center) = m.links[owner.0]
                .mass
                .as_ref()
                .and_then(|properties| properties.center)
            {
                m.pairs[m.points[center.0].pair.0].name = name;
            } else {
                let id = m.add_point(owner, PairKind::MassCenter, name)?;
                m.links[owner.0].mass = Some(MassProperties {
                    mass: Real48::ZERO,
                    inertia: Real48::ZERO,
                    center: Some(id),
                });
            }
            index += 1;
        }
    }
    let mut index = 0;
    loop {
        let owner_name = r.name(
            FieldPath::ForceOwner(index),
            "Звено приложения силы; пробел — закончить",
            &[],
        )?;
        if owner_name == Name::BLANK {
            if r.yes(
                FieldPath::ConfirmForcePoints,
                "Все точки приложения сил перечислены?",
                &[],
            )? {
                break;
            }
            index += 1;
            continue;
        }
        let Some(owner) = m.link_by_name(owner_name) else {
            index += 1;
            continue;
        };
        loop {
            let name = r.name(
                FieldPath::ForceName(index),
                "Имя точки приложения силы",
                &[owner_name],
            )?;
            if name == Name::BLANK {
                break;
            }
            if m.pair_by_name(name).is_none() {
                m.add_point(owner, PairKind::ForcePoint, name)?;
                break;
            }
        }
        index += 1;
    }
    Ok(())
}

fn read_dimensions<C: DialogueNumbers>(
    r: &mut Reader<'_, C>,
    m: &mut Mechanism,
) -> Result<(), ReplayError> {
    r.marker("_Начало_задания_размеров_механизма")?;
    r.marker("_Положение_шарниров_стойки")?;
    let first_input = m
        .groups
        .iter()
        .position(|g| g.kind == GroupKind::Input)
        .ok_or(CoreError::InvalidStructure("no input group"))?;
    // Synchronizing a ground pair also updates its other arena occurrences,
    // so keep the traversal independent from that mutable model borrow.
    let ground_groups = m.groups[..first_input].to_vec();
    for group in ground_groups {
        let pairs: Vec<_> = match group.kind {
            GroupKind::Ground => [group.pair_x, group.pair_y].into_iter().flatten().collect(),
            GroupKind::Attached => group.more.into_iter().collect(),
            _ => Vec::new(),
        };
        for id in pairs {
            let name = m.pairs[id.0].name;
            r.marker(&format!("_Пара_{}", LegacyName(name.0).text()))?;
            let state = m.pairs[id.0].state;
            m.pairs[id.0].state.x = r.real(
                FieldPath::GroundX(id),
                "Координата X пары стойки",
                "м",
                &[name],
                state.x,
            )?;
            m.pairs[id.0].state.y = r.real(
                FieldPath::GroundY(id),
                "Координата Y пары стойки",
                "м",
                &[name],
                state.y,
            )?;
            if m.pairs[id.0].kind != PairKind::Revolute {
                m.pairs[id.0].state.fi = r.radian(
                    FieldPath::GroundAngle(id),
                    "Угол положительного направления направляющей",
                    &[name],
                    state.fi,
                )?;
            }
            m.sync_named_pair(id)?;
        }
    }
    r.marker("_Задание_размеров_звеньев")?;
    for id in legacy_link_order(m).into_iter().skip(1) {
        let guides = m.classify_link(id)?;
        if guides == b'2' {
            continue;
        }
        let name = m.links[id.0].name;
        r.marker(&format!("_Звено_{}", LegacyName(name.0).text()))?;
        let on_guide = guides == b'1'
            && r.yes(
                FieldPath::LinkOnGuide(id),
                "Вращательная пара лежит на направляющей?",
                &[name],
            )?;
        m.links[id.0].geometry.parameter = if on_guide {
            Real48::ZERO
        } else {
            let mut value = r.real(
                FieldPath::LinkParameter(id),
                "Длина звена или перпендикуляра",
                "м",
                &[name],
                m.links[id.0].geometry.parameter,
            )?;
            while guides == b'0' && value.is_zero() {
                value = r.real(
                    FieldPath::LinkParameter(id),
                    "Нулевая длина RR-звена недопустима; повторите ввод",
                    "м",
                    &[name],
                    value,
                )?;
            }
            value
        };
    }
    r.marker("_Выбор_сборки")?;
    for index in first_input..m.groups.len() {
        read_assembly(r, m, GroupId(index))?;
    }
    if !m.points.is_empty() {
        r.marker("_Положение_точек_на_звеньях")?;
        r.marker("_Положение_просто_точек")?;
        let groups = m.groups[first_input..]
            .iter()
            .filter(|g| g.kind == GroupKind::Attached && (g.assembly == b'T' || g.assembly == b' '))
            .cloned()
            .collect::<Vec<_>>();
        for group in groups {
            read_point_geometry(r, m, &group)?;
        }
    }
    if !m.force_analysis {
        r.marker("_Конец_здания_размеров")?;
        if !r.yes(
            FieldPath::ConfirmDimensions,
            "Размеры и параметры заданы правильно?",
            &[],
        )? {
            return Err(ReplayError::ReviewRequired(FieldPath::ConfirmDimensions));
        }
    }
    Ok(())
}

fn neg(value: Real48) -> Real48 {
    if value.is_zero() {
        return Real48::ZERO;
    }
    let mut b = value.to_le_bytes();
    b[5] ^= 0x80;
    Real48::from_le_bytes(b)
}
fn abs(value: Real48) -> Real48 {
    if value.is_zero() {
        return Real48::ZERO;
    }
    let mut b = value.to_le_bytes();
    b[5] &= 0x7f;
    Real48::from_le_bytes(b)
}
fn offset_sign<C: DialogueNumbers>(
    r: &mut Reader<'_, C>,
    m: &mut Mechanism,
    id: LinkId,
) -> Result<(), ReplayError> {
    let name = m.links[id.0].name;
    if !m.links[id.0].geometry.parameter.is_zero() {
        let positive = r.yes(
            FieldPath::SignedOffset(id),
            "Положительное направление смещения соответствует выбранному обходу?",
            &[name],
        )?;
        let magnitude = abs(m.links[id.0].geometry.parameter);
        m.links[id.0].geometry.parameter = if positive { magnitude } else { neg(magnitude) };
    }
    Ok(())
}
fn read_assembly<C: DialogueNumbers>(
    r: &mut Reader<'_, C>,
    m: &mut Mechanism,
    id: GroupId,
) -> Result<(), ReplayError> {
    let g = m.groups[id.0].clone();
    let a = g.link_a;
    let name = m.links[a.0].name;
    let sign = |value| if value { b'+' } else { b'-' };
    match g.kind {
        GroupKind::Input => match g.input_type {
            b'0' => {}
            b'1' => offset_sign(r, m, a)?,
            b'2' => {
                m.links[a.0].geometry.parameter = abs(m.links[a.0].geometry.parameter);
            }
            b'3' => {
                m.links[a.0].geometry.parameter = r.radian(
                    FieldPath::InputGuideAngle(a),
                    "Угол между направляющими входного звена",
                    &[name],
                    m.links[a.0].geometry.parameter,
                )?;
            }
            _ => return Err(CoreError::InvalidStructure("unknown input type").into()),
        },
        GroupKind::Dyad => {
            let b = g
                .link_b
                .ok_or(CoreError::InvalidStructure("dyad link absent"))?;
            let names = [name, m.links[b.0].name];
            match g.input_type {
                b'1' => {
                    m.groups[id.0].assembly = sign(r.yes(
                        FieldPath::Assembly(id),
                        "Пары диады обходятся по часовой стрелке?",
                        &names,
                    )?);
                }
                b'2' => {
                    let guide =
                        if m.pairs[m.links[a.0].pairs[0].unwrap().0].kind == PairKind::Prismatic {
                            a
                        } else {
                            b
                        };
                    offset_sign(r, m, guide)?;
                    m.groups[id.0].assembly = sign(r.yes(
                        FieldPath::Assembly(id),
                        "Вектор к внутренней паре образует острый угол с направляющей?",
                        &names,
                    )?);
                }
                b'3' => {
                    offset_sign(r, m, a)?;
                    offset_sign(r, m, b)?;
                    m.groups[id.0].assembly = if m.links[a.0].geometry.parameter.is_zero()
                        && m.links[b.0].geometry.parameter.is_zero()
                    {
                        b'+'
                    } else {
                        sign(r.yes(
                            FieldPath::Assembly(id),
                            "Вектор внешних пар образует острый угол с внутренней направляющей?",
                            &names,
                        )?)
                    };
                }
                b'4' => {
                    offset_sign(r, m, a)?;
                    offset_sign(r, m, b)?;
                }
                b'5' => {
                    let rotating =
                        if m.pairs[m.links[a.0].pairs[0].unwrap().0].kind == PairKind::Revolute {
                            a
                        } else {
                            b
                        };
                    let guides = if rotating == a { b } else { a };
                    offset_sign(r, m, rotating)?;
                    m.links[guides.0].geometry.parameter = r.radian(
                        FieldPath::InputGuideAngle(guides),
                        "Угол между направляющими диады",
                        &names,
                        m.links[guides.0].geometry.parameter,
                    )?;
                }
                _ => return Err(CoreError::InvalidStructure("unknown dyad type").into()),
            }
        }
        _ => {}
    }
    Ok(())
}

fn read_point_geometry<C: DialogueNumbers>(
    r: &mut Reader<'_, C>,
    m: &mut Mechanism,
    g: &Group,
) -> Result<(), ReplayError> {
    let id = g
        .attached_point
        .ok_or(CoreError::InvalidStructure("attached geometry absent"))?;
    let point = m.points[id.0].clone();
    let name = m.pairs[point.pair.0].name;
    if (g.assembly == b'T' || g.assembly == b' ') && (g.input_type == b'0' || g.input_type == b'1')
    {
        r.marker(&format!(
            "_{}_{}",
            if g.assembly == b' ' {
                "Пара"
            } else {
                "Точка"
            },
            LegacyName(name.0).text()
        ))?;
    }
    let parameter = if g.input_type == b'3' || g.input_type == b'4' {
        if r.yes(
            FieldPath::PointOnGuide(id),
            "Точка/пара лежит на направляющей?",
            &[name],
        )? {
            Real48::ZERO
        } else {
            r.real(
                FieldPath::PointDistance(id),
                "Перпендикулярное расстояние до направляющей",
                "м",
                &[name],
                point.geometry.parameter,
            )?
        }
    } else {
        let mut value = r.real(
            FieldPath::PointDistance(id),
            "Расстояние дополнительной точки от опорной пары/пересечения",
            "м",
            &[name],
            point.geometry.parameter,
        )?;
        while g.assembly == b' '
            && (g.input_type == b'0' || g.input_type == b'1')
            && value.is_zero()
        {
            value = r.real(
                FieldPath::PointDistance(id),
                "Расстояние между разными парами не может быть нулевым",
                "м",
                &[name],
                value,
            )?;
        }
        value
    };
    m.points[id.0].geometry.parameter = parameter;
    m.points[id.0].geometry.angle =
        if (g.assembly == b'M' || g.assembly == b'F') && parameter.is_zero() {
            Real48::ZERO
        } else {
            r.radian(
                FieldPath::PointAngle(id),
                "Угол от исходного вектора до направления точки",
                &[name],
                point.geometry.angle,
            )?
        };
    Ok(())
}

fn read_forces<C: DialogueNumbers>(
    r: &mut Reader<'_, C>,
    m: &mut Mechanism,
) -> Result<(), ReplayError> {
    r.marker("_Положение_центров_масс")?;
    for code in [b'M', b'F'] {
        if code == b'F' {
            r.marker("_Положение_точек_приложения_сил")?;
        }
        for group in m
            .groups
            .iter()
            .filter(|g| g.kind == GroupKind::Attached && g.assembly == code)
            .cloned()
            .collect::<Vec<_>>()
        {
            read_point_geometry(r, m, &group)?;
        }
    }
    r.marker("_Конец_здания_размеров")?;
    r.marker("_Ввод_сил_и_масс")?;
    if m.links.iter().any(|link| link.mass.is_some()) {
        r.marker("_Массы")?;
        for owner in legacy_link_order(m).into_iter().skip(1) {
            if m.links[owner.0].mass.is_some() {
                let name = m.links[owner.0].name;
                let mass = r.real(
                    FieldPath::Mass(owner),
                    "Масса звена",
                    "кг",
                    &[name],
                    m.links[owner.0].mass.as_ref().unwrap().mass,
                )?;
                let inertia = r.real(
                    FieldPath::Inertia(owner),
                    "Момент инерции звена относительно центра масс",
                    "кг·м²",
                    &[name],
                    m.links[owner.0].mass.as_ref().unwrap().inertia,
                )?;
                let properties = m.links[owner.0].mass.as_mut().unwrap();
                properties.mass = mass;
                properties.inertia = inertia;
            }
        }
        if !r.yes(
            FieldPath::ConfirmMassValues,
            "Массы и моменты инерции всех звеньев заданы правильно?",
            &[],
        )? {
            let mut index = 0;
            loop {
                let name = r.name(
                    FieldPath::MassEditOwner(index),
                    "Звено для уточнения массы и момента инерции; пробел — закончить",
                    &[],
                )?;
                if name == Name::BLANK {
                    break;
                }
                let Some(owner) = m.link_by_name(name) else {
                    index += 1;
                    continue;
                };
                if m.links[owner.0].mass.is_none() {
                    return Err(ReplayError::Unsupported{line:r.cursor.position(),message:"mass correction for a link without a mass-center record is undefined in the original".into()});
                }
                let properties = m.links[owner.0].mass.clone().unwrap();
                let mass = r.real(
                    FieldPath::Mass(owner),
                    "Уточнённая масса звена",
                    "кг",
                    &[name],
                    properties.mass,
                )?;
                let inertia = r.real(
                    FieldPath::Inertia(owner),
                    "Уточнённый момент инерции звена",
                    "кг·м²",
                    &[name],
                    properties.inertia,
                )?;
                let properties = m.links[owner.0].mass.as_mut().unwrap();
                properties.mass = mass;
                properties.inertia = inertia;
                index += 1;
            }
        }
    }
    r.marker("_Силы")?;
    for id in (0..m.points.len())
        .map(PointId)
        .filter(|id| m.pairs[m.points[id.0].pair.0].kind == PairKind::ForcePoint)
        .collect::<Vec<_>>()
    {
        let pair = m.points[id.0].pair;
        let name = m.pairs[pair.0].name;
        m.pairs[pair.0].state.rx = r.real(
            FieldPath::ForceMagnitude(id),
            "Модуль внешней силы",
            "Н",
            &[name],
            m.pairs[pair.0].state.rx,
        )?;
        m.pairs[pair.0].state.ry = if m.pairs[pair.0].state.rx.is_zero() {
            Real48::ZERO
        } else {
            r.radian(
                FieldPath::ForceAngle(id),
                "Угол внешней силы относительно оси X",
                &[name],
                m.pairs[pair.0].state.ry,
            )?
        };
    }
    r.marker("_Моменты")?;
    for owner in legacy_link_order(m).into_iter().skip(m.inputs.len() + 1) {
        let name = m.links[owner.0].name;
        m.links[owner.0].applied_moment = Some(r.real(
            FieldPath::Moment(owner),
            "Внешний момент сил на звене",
            "Н·м",
            &[name],
            m.links[owner.0].applied_moment.unwrap_or(Real48::ZERO),
        )?);
        m.links[owner.0].geometry.last_value = m.links[owner.0].applied_moment.unwrap();
    }
    r.marker("_Конец_задания_сил")?;
    if !r.yes(
        FieldPath::ConfirmForces,
        "Силы и массы заданы правильно?",
        &[],
    )? {
        return Err(ReplayError::ReviewRequired(FieldPath::ConfirmForces));
    }
    Ok(())
}
