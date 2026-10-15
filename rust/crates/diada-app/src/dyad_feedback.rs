//! Explanations for rejected dyad answers. The original dialogue still owns
//! acceptance, retries and the saved answer history.
use diada_core::{CoreError, GroupKind, LinkId, Mechanism, Name, PairId};

pub fn rejection(model: &Mechanism, first: Name, second: Name) -> Option<String> {
    // A blank answer intentionally returns to the connection description.
    if first == Name::BLANK || second == Name::BLANK {
        return None;
    }
    let (Some(a), Some(b)) = (model.link_by_name(first), model.link_by_name(second)) else {
        let unknown = if model.link_by_name(first).is_none() {
            first
        } else {
            second
        };
        return Some(format!("Звено {unknown} не найдено в таблице связей."));
    };
    let mut candidate = model.clone();
    let error = candidate.append_dyad(a, b).err()?;
    Some(match error {
        CoreError::InvalidStructure("dyad requires distinct non-ground non-input links") => {
            if a == b {
                format!("В диаде должны быть два разных звена. Звено {first} указано дважды.")
            } else if let Some(ground) = [a, b].into_iter().find(|id| Some(*id) == model.ground) {
                format!(
                    "Звено {} — стойка. В диаду входят только подвижные неведущие звенья.",
                    model.links[ground.0].name
                )
            } else if let Some(input) = [a, b].into_iter().find(|id| model.inputs.contains(id)) {
                format!(
                    "Звено {} — ведущее. Оно уже относится к начальному механизму и не входит в диаду.",
                    model.links[input.0].name
                )
            } else {
                generic(first, second)
            }
        }
        CoreError::InvalidStructure("link already belongs to a dyad") => {
            let assigned = [a, b].into_iter().find(|id| {
                model.groups.iter().any(|group| {
                    group.kind == GroupKind::Dyad
                        && (group.link_a == *id || group.link_b == Some(*id))
                })
            });
            assigned.map_or_else(
                || generic(first, second),
                |id| format!("Звено {} уже входит в предыдущую диаду.", model.links[id.0].name),
            )
        }
        CoreError::InvalidStructure("dyad must share exactly one pair name") => format!(
            "Звенья {first} и {second} должны быть соединены ровно одной общей кинематической парой. Проверьте введённые имена и таблицу связей."
        ),
        CoreError::InvalidStructure("dyad outer pair is not yet known") => {
            unknown_outer_pair(&candidate, [a, b]).map_or_else(
                || "Порядок анализа задан неверно: внешняя пара диады ещё не определена предыдущими группами.".into(),
                |name| format!(
                    "Порядок анализа задан неверно. Нельзя определить положение пары {name}: она ещё не определена предыдущими группами."
                ),
            )
        }
        CoreError::InvalidStructure("three prismatic pairs in a dyad") => {
            "В указанной диаде три поступательные пары. Такая диада не поддерживается исходной ДиАДА.".into()
        }
        _ => generic(first, second),
    })
}

fn generic(first: Name, second: Name) -> String {
    format!(
        "Звенья {first} и {second} не образуют допустимую очередную диаду. Проверьте имена звеньев, их соединения и порядок анализа."
    )
}

fn unknown_outer_pair(model: &Mechanism, links: [LinkId; 2]) -> Option<Name> {
    // append_dyad has already moved the shared pair into the second slot.
    // Match known endpoints by name, including original pair aliases.
    let known = |id: PairId| {
        let name = model.pair(id)?.name;
        Some(model.groups.iter().any(|group| {
            [group.pair_x, group.pair_y, group.more]
                .into_iter()
                .flatten()
                .any(|pair| model.pair(pair).is_some_and(|pair| pair.name == name))
        }))
    };
    links.into_iter().find_map(|id| {
        let outer = model.link(id)?.pairs[0]?;
        (known(outer) == Some(false)).then(|| model.pair(outer).map(|pair| pair.name))?
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use diada_core::PairKind;

    fn name(value: u8) -> Name {
        Name::new(value, b' ')
    }

    fn user_mechanism() -> Mechanism {
        let mut model = Mechanism::default();
        for (pair, kind, a, b) in [
            (b'O', PairKind::Revolute, b'0', b'1'),
            (b'A', PairKind::Revolute, b'1', b'2'),
            (b'B', PairKind::Revolute, b'2', b'3'),
            (b'C', PairKind::Prismatic, b'3', b'0'),
            (b'D', PairKind::Revolute, b'2', b'4'),
            (b'E', PairKind::Revolute, b'4', b'5'),
            (b'F', PairKind::Prismatic, b'5', b'0'),
        ] {
            let a = model.add_link(name(a));
            let b = model.add_link(name(b));
            model.add_pair(kind, name(pair), [a, b]).unwrap();
        }
        let ground = model.link_by_name(name(b'0')).unwrap();
        let input = model.link_by_name(name(b'1')).unwrap();
        model.set_ground_and_inputs(ground, vec![input]).unwrap();
        model.prepare_initial_groups().unwrap();
        model
    }

    #[test]
    fn the_users_reversed_analysis_order_names_unknown_d_without_changing_the_model() {
        let model = user_mechanism();
        let original = model.clone();
        for (a, b) in [(b'4', b'5'), (b'5', b'4')] {
            let message = rejection(&model, name(a), name(b)).unwrap();
            assert!(message.contains("пары D:"), "{message}");
            assert!(message.contains("Порядок анализа задан неверно"));
            assert_eq!(model, original);
        }
    }

    #[test]
    fn valid_dyads_are_silent_and_the_second_becomes_valid_after_the_first() {
        let mut model = user_mechanism();
        let original = model.clone();
        assert_eq!(rejection(&model, name(b'2'), name(b'3')), None);
        assert_eq!(rejection(&model, name(b'3'), name(b'2')), None);
        assert_eq!(model, original);
        model
            .append_dyad(
                model.link_by_name(name(b'2')).unwrap(),
                model.link_by_name(name(b'3')).unwrap(),
            )
            .unwrap();
        let after_first = model.clone();
        assert_eq!(rejection(&model, name(b'4'), name(b'5')), None);
        assert_eq!(rejection(&model, name(b'5'), name(b'4')), None);
        let message = rejection(&model, name(b'2'), name(b'3')).unwrap();
        assert!(message.contains("Звено 2 уже входит"), "{message}");
        assert_eq!(model, after_first);
    }

    #[test]
    fn invalid_names_and_link_roles_have_specific_reasons_and_blank_returns_are_silent() {
        let model = user_mechanism();
        let original = model.clone();
        for (a, b, expected) in [
            (b'1', b'2', "Звено 1 — ведущее"),
            (b'2', b'0', "Звено 0 — стойка"),
            (b'2', b'2', "Звено 2 указано дважды"),
            (b'Z', b'3', "Звено Z не найдено"),
            (b'2', b'Z', "Звено Z не найдено"),
            (b'2', b'5', "ровно одной общей кинематической парой"),
        ] {
            let message = rejection(&model, name(a), name(b)).unwrap();
            assert!(message.contains(expected), "{message}");
            assert_eq!(model, original);
        }
        for (a, b) in [(Name::BLANK, name(b'3')), (name(b'2'), Name::BLANK)] {
            assert_eq!(rejection(&model, a, b), None);
            assert_eq!(model, original);
        }
    }
}
