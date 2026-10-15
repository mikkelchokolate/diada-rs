use crate::drawing::ObjectKey;
use diada_core::*;
use diada_io::{EditableDialogue, FieldBinding, FieldPath, ReplayQuestion};

pub fn rp_pairs(model: &Mechanism, id: LinkId) -> Option<(PairId, PairId)> {
    let [Some(first), Some(second)] = model.link(id)?.pairs else {
        return None;
    };
    match (model.pair(first)?.kind, model.pair(second)?.kind) {
        (PairKind::Revolute, PairKind::Prismatic) => Some((first, second)),
        (PairKind::Prismatic, PairKind::Revolute) => Some((second, first)),
        _ => None,
    }
}

pub fn question_text(question: &ReplayQuestion, model: Option<&Mechanism>) -> String {
    let Some(model) = model else {
        return question.text.clone();
    };
    match question.path {
        FieldPath::GroupLink(index, which) => {
            return if which == 0 {
                format!("Диада {}: первое звено группы из двух звеньев", index + 1)
            } else if let Some(first) = question.objects.first() {
                format!("Диада {}: второе звено к звену {}", index + 1, first.text())
            } else {
                format!("Диада {}: второе звено группы", index + 1)
            };
        }
        FieldPath::GroundX(id) | FieldPath::GroundY(id) => {
            if let Some(pair) = model.pair(id) {
                return format!(
                    "Координата {} {} {} на стойке",
                    if matches!(question.path, FieldPath::GroundX(_)) {
                        "X"
                    } else {
                        "Y"
                    },
                    if pair.kind == PairKind::Revolute {
                        "шарнира"
                    } else {
                        "опорной точки направляющей"
                    },
                    pair.name.display(),
                );
            }
        }
        FieldPath::GroundAngle(id) => {
            if let Some(pair) = model.pair(id) {
                return format!(
                    "Угол положительного направления направляющей {}",
                    pair.name.display()
                );
            }
        }
        FieldPath::InputGuideAngle(id) => {
            if let Some(link) = model.link(id)
                && let [Some(first), Some(second)] = link.pairs
                && let (Some(a), Some(b)) = (model.pair(first), model.pair(second))
                && a.kind == PairKind::Prismatic
                && b.kind == PairKind::Prismatic
            {
                return format!(
                    "Угол от положительного направления направляющей {} к направляющей {} звена {}",
                    a.name.display(),
                    b.name.display(),
                    link.name.display()
                );
            }
        }
        FieldPath::LinkOnGuide(id) => {
            if let Some((hinge, guide)) = rp_pairs(model, id) {
                return format!(
                    "Шарнир {} звена {} лежит на направляющей ползуна {}?",
                    model.pairs[hinge.0].name.display(),
                    model.links[id.0].name.display(),
                    model.pairs[guide.0].name.display(),
                );
            }
        }
        FieldPath::LinkParameter(id) => {
            if let Some((hinge, guide)) = rp_pairs(model, id) {
                return format!(
                    "Перпендикулярное расстояние от шарнира {} звена {} до направляющей ползуна {}",
                    model.pairs[hinge.0].name.display(),
                    model.links[id.0].name.display(),
                    model.pairs[guide.0].name.display(),
                );
            }
            if let Some(link) = model.link(id)
                && let [Some(first), Some(second)] = link.pairs
                && let (Some(a), Some(b)) = (model.pair(first), model.pair(second))
                && a.kind == PairKind::Revolute
                && b.kind == PairKind::Revolute
            {
                return format!(
                    "Длина звена {} между шарнирами {} и {}",
                    link.name.display(),
                    a.name.display(),
                    b.name.display(),
                );
            }
        }
        _ => {}
    }
    question.text.clone()
}

pub fn mobility_summary(model: &Mechanism) -> String {
    let links = model.links.len();
    let pairs = model
        .pairs
        .iter()
        .filter(|pair| matches!(pair.kind, PairKind::Revolute | PairKind::Prismatic))
        .count();
    format!(
        "Звеньев: {links}; пар: {pairs}. Подвижность W = 3 × ({links} − 1) − 2 × {pairs} = {}.",
        model.formal_mobility()
    )
}

pub fn mobility_error(model: &Mechanism) -> String {
    format!(
        "{} Для расчёта требуется W > 0. Проверьте таблицу связей; повторяющиеся имена пар также учитываются как отдельные соединения.",
        mobility_summary(model)
    )
}

pub fn raw(document: &EditableDialogue, binding: &FieldBinding) -> String {
    binding.answer_text(document.current_journal())
}

pub fn describe(path: &FieldPath, m: &Mechanism) -> (String, &'static str, Option<ObjectKey>) {
    use FieldPath::*;
    let link = |id: LinkId| {
        m.links
            .get(id.0)
            .map(|v| v.name.display())
            .unwrap_or_default()
    };
    let pair = |id: PairId| {
        m.pairs
            .get(id.0)
            .map(|v| v.name.display())
            .unwrap_or_default()
    };
    let point = |id: PointId| {
        m.points
            .get(id.0)
            .and_then(|p| m.pairs.get(p.pair.0))
            .map(|v| v.name.display())
            .unwrap_or_default()
    };
    let lk = |id: LinkId| Some(ObjectKey::Link(id.0));
    let pk = |id: PairId| Some(ObjectKey::Pair(id.0));
    let pt = |id: PointId| m.points.get(id.0).map(|p| ObjectKey::Pair(p.pair.0));
    match path {
        GroundX(id) => (format!("{} · X стойки", pair(*id)), "м", pk(*id)),
        GroundY(id) => (format!("{} · Y стойки", pair(*id)), "м", pk(*id)),
        GroundAngle(id) => (format!("{} · направление", pair(*id)), "град", pk(*id)),
        InputGuideAngle(id) => (
            format!("{} · угол между направляющими", link(*id)),
            "град",
            lk(*id),
        ),
        LinkParameter(id) => (format!("{} · размер", link(*id)), "м", lk(*id)),
        SignedOffset(id) => (
            format!("{} · сторона перпендикуляра", link(*id)),
            "",
            lk(*id),
        ),
        LinkOnGuide(id) => (
            format!("{} · точка на направляющей", link(*id)),
            "",
            lk(*id),
        ),
        Assembly(id) => (format!("Группа {} · сборка", id.0 + 1), "", None),
        PointDistance(id) => (format!("{} · расстояние", point(*id)), "м", pt(*id)),
        PointAngle(id) => (format!("{} · угол", point(*id)), "град", pt(*id)),
        PointOnGuide(id) => (format!("{} · на направляющей", point(*id)), "", pt(*id)),
        Mass(id) => (format!("{} · масса", link(*id)), "кг", lk(*id)),
        Inertia(id) => (format!("{} · момент инерции", link(*id)), "кг·м²", lk(*id)),
        Moment(id) => (format!("{} · внешний момент", link(*id)), "Н·м", lk(*id)),
        ForceMagnitude(id) => (format!("{} · сила", point(*id)), "Н", pt(*id)),
        ForceAngle(id) => (
            format!("{} · направление силы", point(*id)),
            "град",
            pt(*id),
        ),
        Ground => ("Стойка".into(), "", m.ground.and_then(lk)),
        Input(i) => (
            format!("Ведущее звено {}", i + 1),
            "",
            m.inputs.get(*i).copied().and_then(lk),
        ),
        ForceMode => ("Силовой анализ".into(), "", None),
        PairCommand(i) => (format!("Пара {} · тип", i + 1), "", None),
        PairLink(i, j) => (format!("Пара {} · звено {}", i + 1, j + 1), "", None),
        PairName(i) => (format!("Пара {} · имя", i + 1), "", None),
        MassCenter(id) => (format!("{} · центр масс", link(*id)), "", lk(*id)),
        ForceOwner(i) => (format!("Сила {} · звено", i + 1), "", None),
        ForceName(i) => (format!("Сила {} · имя точки", i + 1), "", None),
        PointOwner(i) => (format!("Точка {} · звено", i + 1), "", None),
        PointName(i) => (format!("Точка {} · имя", i + 1), "", None),
        GroupLink(i, j) => (format!("Диада {} · звено {}", i + 1, j + 1), "", None),
        AdditionalPoints => ("Дополнительные точки".into(), "", None),
        ConfirmPairs => ("Проверка пар".into(), "", None),
        ConfirmGroups => ("Проверка групп".into(), "", None),
        ConfirmDimensions => ("Проверка размеров".into(), "", None),
        ConfirmForces => ("Проверка нагрузок".into(), "", None),
        ConfirmMassCenters => ("Проверка центров масс".into(), "", None),
        ConfirmForcePoints => ("Проверка точек сил".into(), "", None),
        RenamePairConfirm(i) => (format!("Пара {} · переименование", i + 1), "", None),
        MassRenameOwner(i) => (format!("Изменение центра масс {}", i + 1), "", None),
        MassEditOwner(i) => (format!("Изменение массы {}", i + 1), "", None),
        ConfirmMassValues => ("Проверка масс и инерции".into(), "", None),
    }
}

pub fn section(path: &FieldPath) -> &'static str {
    use FieldPath::*;
    match path {
        GroundX(_) | GroundY(_) | GroundAngle(_) | LinkParameter(_) | SignedOffset(_)
        | Assembly(_) | InputGuideAngle(_) | PointDistance(_) | PointAngle(_) | PointOnGuide(_)
        | LinkOnGuide(_) | ConfirmDimensions => "Размеры",
        Mass(_) | Inertia(_) | Moment(_) | ForceMagnitude(_) | ForceAngle(_) | ConfirmForces => {
            "Нагрузки"
        }
        _ => "Структура",
    }
}

pub fn structural(path: &FieldPath) -> bool {
    section(path) == "Структура"
}

pub fn confirmation(path: &FieldPath) -> bool {
    matches!(
        path,
        FieldPath::ConfirmPairs
            | FieldPath::ConfirmGroups
            | FieldPath::ConfirmDimensions
            | FieldPath::ConfirmForces
            | FieldPath::ConfirmMassCenters
            | FieldPath::ConfirmForcePoints
            | FieldPath::ConfirmMassValues
    )
}
