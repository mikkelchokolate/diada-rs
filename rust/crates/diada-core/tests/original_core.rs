//! Every expected bit comes from the measured original runtime. Fixture
//! provenance and exclusions are in fixtures/manifest.json.
// The standalone oracle executables are reused as test modules; their small
// fixture reader is intentionally local to each executable.
#![allow(clippy::duplicate_mod)]
#[allow(dead_code)]
#[path = "../examples/dyad_oracle.rs"]
mod dyad;
#[allow(dead_code)]
#[path = "../examples/force_oracle.rs"]
mod force;
#[allow(dead_code)]
#[path = "../examples/full_force_oracle.rs"]
mod full_force;
#[allow(dead_code)]
#[path = "../examples/graph_oracle.rs"]
mod graph;
#[allow(dead_code)]
#[path = "../examples/input_oracle.rs"]
mod input;

#[test]
fn all_input_types_and_defined_aliases() {
    input::verify(include_bytes!("fixtures/input.real48.bin"));
}
#[test]
fn dyad_1_original() {
    dyad::verify(b'1', include_bytes!("fixtures/dyad-1.real48.bin"));
}
#[test]
fn dyad_2_original() {
    dyad::verify(b'2', include_bytes!("fixtures/dyad-2.real48.bin"));
}
#[test]
fn dyad_3_original() {
    dyad::verify(b'3', include_bytes!("fixtures/dyad-3.real48.bin"));
}
#[test]
fn dyad_4_original() {
    dyad::verify(b'4', include_bytes!("fixtures/dyad-4.real48.bin"));
}
#[test]
fn dyad_5_original() {
    dyad::verify(b'5', include_bytes!("fixtures/dyad-5.real48.bin"));
}
#[test]
fn five_force_solvers() {
    force::verify(include_bytes!("fixtures/forces-solver.real48.bin"));
}
#[test]
fn whole_force_traversal_with_approved_empty_cache() {
    full_force::verify(include_bytes!("fixtures/forces-whole-corrected.real48.bin"));
}
#[test]
fn original_graph_values() {
    graph::verify(include_bytes!("fixtures/graphs.real48.bin"));
}
