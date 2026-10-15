use diada_core::*;
fn n(value: &str) -> Real48 {
    diada_compat::parse_real(value.as_bytes()).unwrap()
}
fn four_bar() -> Mechanism {
    let mut m = Mechanism::default();
    let ground = m.add_link(Name([b'0', b' ']));
    let crank = m.add_link(Name([b'1', b' ']));
    let rod = m.add_link(Name([b'2', b' ']));
    let rocker = m.add_link(Name([b'3', b' ']));
    m.add_pair(PairKind::Revolute, Name([b'O', b' ']), [ground, crank])
        .unwrap();
    m.add_pair(PairKind::Revolute, Name([b'A', b' ']), [crank, rod])
        .unwrap();
    let fixed = m
        .add_pair(PairKind::Revolute, Name([b'D', b' ']), [ground, rocker])
        .unwrap();
    m.add_pair(PairKind::Revolute, Name([b'B', b' ']), [rod, rocker])
        .unwrap();
    m.pairs[fixed.0].state.x = n("4");
    m.links[crank.0].geometry.parameter = n("1");
    m.links[rod.0].geometry.parameter = n("1.5");
    m.links[rocker.0].geometry.parameter = n("2");
    m.set_ground_and_inputs(ground, vec![crank]).unwrap();
    m.prepare_initial_groups().unwrap();
    let dyad = m.append_dyad(rod, rocker).unwrap();
    m.groups[dyad.0].assembly = b'+';
    m
}
#[test]
fn failed_frames_advance_to_a_later_assembling_position() {
    let m = four_bar();
    let config = AnalysisConfig {
        positions: 3,
        inputs: vec![InputSeries {
            link: m.inputs[0],
            start: n("180"),
            other_start: n("-90"),
            rate: n("1"),
            rate_derivative: Real48::ZERO,
            step: n("-90"),
        }],
        mode: AnalysisMode::Kinematics,
    };
    let mut sampler = KinematicSampler::new(&m, &config).unwrap();
    assert!(sampler.next_frame().unwrap().is_err());
    assert!(!sampler.last_kinematics_succeeded());
    // The defined input is available in the failed preview, while no result
    // snapshot is manufactured for the impossible dyad configuration.
    let first = sampler.preview_mechanism().pairs[1].state;
    assert!(
        first
            .x
            .load_extended()
            .compare(Extended80::ZERO)
            .unwrap()
            .is_lt()
    );
    assert!(sampler.next_frame().unwrap().is_err());
    assert!(!sampler.last_kinematics_succeeded());
    let last = sampler.next_frame().unwrap().unwrap();
    assert!(sampler.last_kinematics_succeeded());
    assert_eq!(last.position_index, 2);
    assert_eq!(last.inputs[0].position, Real48::ZERO);
    assert_eq!(last.mechanism.pairs[1].state.x, n("1"));
    assert_eq!(last.mechanism.pairs[1].state.y, Real48::ZERO);
    assert!(sampler.next_frame().is_none());
    let mut strict = KinematicSampler::new(&m, &config).unwrap();
    assert!(strict.next_snapshot().is_err());
    assert!(!strict.last_kinematics_succeeded());
    assert!(strict.next_snapshot().is_err());
    assert!(!strict.last_kinematics_succeeded());
}
#[test]
fn graph_cycle_keeps_the_closed_endpoint_and_explicit_gaps() {
    let m = four_bar();
    let config = m.original_graph_config(AnalysisMode::Kinematics).unwrap();
    assert_eq!(config.positions, 73);
    let frames = m.sample_frames(&config).unwrap();
    assert_eq!(frames.len(), 73);
    assert_eq!(frames[0].as_ref().unwrap().inputs[0].position, n("0"));
    assert_eq!(frames[1].as_ref().unwrap().inputs[0].position, n("5"));
    // Fixed outer C=(4,0), crank radius 1: outer-pair separation squared is
    // 17-8*cos(phi). The two dyad radii total 3.5, so the circle-intersection
    // boundary lies strictly between 50 and 55 degrees. The original stops
    // that D traversal, records a graph gap, then resumes at the next input.
    assert!(frames[10].is_ok());
    assert!(frames[11].is_err());
    assert!(frames[61].is_err());
    assert!(frames[62].is_ok());
    assert!(frames[36].is_err());
    assert_eq!(frames[71].as_ref().unwrap().inputs[0].position, n("355"));
    let endpoint = frames[72].as_ref().unwrap();
    assert_eq!(endpoint.inputs[0].position, n("360"));
    assert_eq!(endpoint.mechanism, frames[0].as_ref().unwrap().mechanism);
}

#[test]
fn motion_keeps_rotation_sign_and_two_distinct_oscillation_passes() {
    let m = four_bar();
    let full = m
        .original_motion_configs(n("-2"), None, AnalysisMode::Kinematics)
        .unwrap();
    let frames = m.sample_frames(&full[0]).unwrap();
    assert_eq!(frames[1].as_ref().unwrap().inputs[0].position, n("-5"));
    assert_eq!(frames[1].as_ref().unwrap().inputs[0].rate, n("-1"));
    assert!(
        frames[1].as_ref().unwrap().mechanism.pairs[1]
            .state
            .y
            .load_extended()
            .compare(Extended80::ZERO)
            .unwrap()
            .is_lt()
    );
    let passes = m
        .original_motion_configs(n("-2"), Some([n("-10"), n("10")]), AnalysisMode::Kinematics)
        .unwrap();
    let first = m.sample_frames(&passes[0]).unwrap();
    let second = m.sample_frames(&passes[1]).unwrap();
    assert_eq!(first[0].as_ref().unwrap().inputs[0].position, n("-10"));
    assert_eq!(second[0].as_ref().unwrap().inputs[0].position, n("10"));
    assert_eq!(first[0].as_ref().unwrap().inputs[0].rate, n("-1"));
    assert_eq!(second[0].as_ref().unwrap().inputs[0].rate, n("-1"));
    assert_eq!(
        passes[0].inputs[0]
            .step
            .load_extended()
            .neg()
            .store_real()
            .unwrap(),
        passes[1].inputs[0].step
    );
    assert!(
        first.last().unwrap().as_ref().unwrap().inputs[0]
            .position
            .load_extended()
            .compare(n("10").load_extended())
            .unwrap()
            .is_lt()
    );
    assert!(
        second.last().unwrap().as_ref().unwrap().inputs[0]
            .position
            .load_extended()
            .compare(n("-10").load_extended())
            .unwrap()
            .is_gt()
    );
}

/// A raw leading PR group, matching the resident input-link oracle. This test
/// exercises series sequencing, not the whole-structure mobility validator.
fn translational_input() -> Mechanism {
    let mut model = Mechanism::default();
    let ground = model.add_link(Name(*b"0 "));
    let body = model.add_link(Name(*b"1 "));
    let base = model
        .add_pair(PairKind::Prismatic, Name(*b"P2"), [ground, body])
        .unwrap();
    let out = model
        .add_pair(PairKind::Revolute, Name(*b"P1"), [body, ground])
        .unwrap();
    let mut group = Group::new(GroupKind::Input, body);
    group.input_type = b'1';
    group.pair_x = Some(base);
    group.more = Some(out);
    model.groups.push(group);
    model.ground = Some(ground);
    model.inputs.push(body);
    model
}
fn extreme_series(model: &Mechanism, positions: usize) -> AnalysisConfig {
    AnalysisConfig {
        positions,
        inputs: vec![InputSeries {
            link: model.inputs[0],
            start: n("1e38"),
            other_start: n("1e38"),
            rate: Real48::ZERO,
            rate_derivative: Real48::ZERO,
            step: n("1e38"),
        }],
        mode: AnalysisMode::Kinematics,
    }
}
fn assert_original_extreme_snapshot(snapshot: &CalculationSnapshot) {
    // Independent DOSBox AUTO measurement of original 028D:1344, followed by
    // the original TP Real48 ADD/store which exits with runtime error 205.
    let original = include_bytes!("fixtures/sampling-overflow/current.bin");
    assert_eq!(snapshot.position_index, 0);
    assert_eq!(
        snapshot.mechanism.pairs[1].state.x.to_le_bytes(),
        original[0..6]
    );
    assert_eq!(
        snapshot.mechanism.pairs[1].state.y.to_le_bytes(),
        original[6..12]
    );
    assert_eq!(snapshot.inputs[0].position.to_le_bytes(), original[12..18]);
    assert_eq!(
        snapshot.inputs[0].other_position.to_le_bytes(),
        original[18..24]
    );
    assert_eq!(snapshot.inputs[0].rate.to_le_bytes(), original[24..30]);
    assert_eq!(
        snapshot.inputs[0].rate_derivative.to_le_bytes(),
        original[30..36]
    );
}
#[test]
fn increment_overflow_follows_the_defined_current_frame() {
    let model = translational_input();
    let mut sampler = KinematicSampler::new(&model, &extreme_series(&model, 3)).unwrap();
    let current = sampler.next_frame().unwrap().unwrap();
    assert_original_extreme_snapshot(&current);
    assert!(sampler.last_kinematics_succeeded());
    assert_eq!(
        sampler.next_frame().unwrap(),
        Err(CoreError::Numeric(NumericError::Overflow))
    );
    assert!(!sampler.last_kinematics_succeeded());
    assert_eq!(sampler.preview_mechanism(), &current.mechanism);
    assert!(sampler.next_frame().is_none());
}
#[test]
fn strict_sampler_also_returns_current_before_next_increment_failure() {
    let model = translational_input();
    let mut sampler = KinematicSampler::new(&model, &extreme_series(&model, 2)).unwrap();
    assert_original_extreme_snapshot(&sampler.next_snapshot().unwrap().unwrap());
    assert!(sampler.last_kinematics_succeeded());
    assert_eq!(
        sampler.next_snapshot(),
        Err(CoreError::Numeric(NumericError::Overflow))
    );
    assert!(!sampler.last_kinematics_succeeded());
    assert!(sampler.next_snapshot().unwrap().is_none());
}
#[test]
fn last_position_does_not_prepare_an_unrequested_overflowing_input() {
    let model = translational_input();
    let frames = model.sample_frames(&extreme_series(&model, 1)).unwrap();
    assert_eq!(frames.len(), 1);
    assert_original_extreme_snapshot(frames[0].as_ref().unwrap());
}
#[test]
fn force_failure_preserves_metadata_for_a_complete_defined_pose() {
    let model = translational_input();
    let mut config = extreme_series(&model, 1);
    config.mode = AnalysisMode::Kinetostatics;
    let mut sampler = KinematicSampler::new(&model, &config).unwrap();
    // Original force dispatch for PR skips updating reactions. The approved
    // undefined-state policy reports that path, while PR kinematics is valid.
    assert!(matches!(
        sampler.next_frame().unwrap(),
        Err(CoreError::UndefinedPosition { .. })
    ));
    assert!(sampler.last_kinematics_succeeded());
    assert_eq!(sampler.preview_mechanism().pairs[1].state.x, n("1e38"));
}

#[test]
fn strict_force_failure_certifies_only_the_completed_kinematics() {
    let model = translational_input();
    let mut config = extreme_series(&model, 1);
    config.mode = AnalysisMode::Kinetostatics;
    let mut sampler = KinematicSampler::new(&model, &config).unwrap();
    assert!(matches!(
        sampler.next_snapshot(),
        Err(CoreError::UndefinedPosition { .. })
    ));
    assert!(sampler.last_kinematics_succeeded());
    assert_eq!(
        sampler.preview_mechanism().pairs[1].state.x.to_le_bytes(),
        include_bytes!("fixtures/sampling-overflow/current.bin")[0..6]
    );
}

#[test]
fn failed_dyad_prevents_force_dispatch_and_keeps_the_leading_preview_only() {
    let model = four_bar();
    let config = AnalysisConfig {
        positions: 1,
        inputs: vec![InputSeries {
            link: model.inputs[0],
            start: n("180"),
            other_start: Real48::ZERO,
            rate: n("1"),
            rate_derivative: Real48::ZERO,
            step: Real48::ZERO,
        }],
        mode: AnalysisMode::Kinetostatics,
    };
    let mut sampler = KinematicSampler::new(&model, &config).unwrap();
    assert!(matches!(
        sampler.next_frame().unwrap(),
        Err(CoreError::Unassemblable { group: Some(_) })
    ));
    assert!(!sampler.last_kinematics_succeeded());
    // The resident traversal exits at the first impossible D. A remains a
    // defined input position; B and both dyad link states retain prior values
    // and must not be exposed as a successful current calculation snapshot.
    assert_ne!(
        sampler.preview_mechanism().pairs[1].state.x,
        model.pairs[1].state.x
    );
    assert_eq!(
        sampler.preview_mechanism().pairs[3].state,
        model.pairs[3].state
    );
    assert_eq!(
        sampler.preview_mechanism().links[2].geometry,
        model.links[2].geometry
    );
}

#[test]
fn original_motion_refuses_translating_inputs_after_its_provisional_calculation() {
    // Full unmodified application probes: PR reaches graphics to show the
    // two CS:1A58/1A76 refusal captions, then returns to the text menu after
    // acknowledgement. RR remains in the motion controls. Both share an
    // earlier numerical pose, so entering graphics alone is not support.
    for typ in [b'1', b'3'] {
        let mut model = translational_input();
        model.groups[0].input_type = typ;
        if typ == b'3' {
            model.pairs[1].kind = PairKind::Prismatic;
        }
        assert_eq!(
            model.original_motion_configs(n("1"), None, AnalysisMode::Kinematics),
            Err(CoreError::InvalidAnalysis(
                "original cyclic motion requires a rotating input"
            ))
        );
    }
    let model = four_bar();
    assert_eq!(
        model
            .original_motion_configs(n("1"), None, AnalysisMode::Kinematics)
            .unwrap()[0]
            .positions,
        72
    );
}
