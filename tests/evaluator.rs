use std::time::Duration;

use std::collections::HashSet;

use agent_loop::contracts::{DoneDecision, GateName, GateReport, ReviewOutput, ReviewStatus};
use agent_loop::evaluator::{DoneEvaluator, ParentDecisionInput};

#[test]
fn decides_done_when_all_conditions_are_green() {
    let evaluator = DoneEvaluator::default();
    let gates = GateReport::all_green();
    let review = ReviewOutput {
        status: ReviewStatus::Done,
        coverage_score: 0.95,
        missing_items: vec![],
        spawn_candidates: vec![],
        risk_flags: vec![],
    };

    let decision = evaluator.decide_parent(&ParentDecisionInput {
        gates: &gates,
        non_blocking_gates: &HashSet::new(),
        review: &review,
        has_open_blocking_children: false,
        parent_iteration: 1,
        max_iterations: 3,
        elapsed: Duration::from_secs(10),
        timeout: Duration::from_secs(300),
    });

    assert_eq!(decision, DoneDecision::Done);
}

#[test]
fn decides_rework_when_gate_fails_before_limit() {
    let evaluator = DoneEvaluator::default();
    let gates = GateReport {
        fmt_ok: true,
        clippy_ok: false,
        tests_ok: true,
        perf_ok: true,
    };
    let review = ReviewOutput {
        status: ReviewStatus::Done,
        coverage_score: 1.0,
        missing_items: vec![],
        spawn_candidates: vec![],
        risk_flags: vec![],
    };

    let decision = evaluator.decide_parent(&ParentDecisionInput {
        gates: &gates,
        non_blocking_gates: &HashSet::new(),
        review: &review,
        has_open_blocking_children: false,
        parent_iteration: 1,
        max_iterations: 3,
        elapsed: Duration::from_secs(10),
        timeout: Duration::from_secs(300),
    });

    assert_eq!(decision, DoneDecision::Rework);
}

#[test]
fn decides_needs_human_on_iteration_limit() {
    let evaluator = DoneEvaluator::default();
    let gates = GateReport::all_green();
    let review = ReviewOutput {
        status: ReviewStatus::Rework,
        coverage_score: 0.4,
        missing_items: vec!["missing".into()],
        spawn_candidates: vec![],
        risk_flags: vec![],
    };

    let decision = evaluator.decide_parent(&ParentDecisionInput {
        gates: &gates,
        non_blocking_gates: &HashSet::new(),
        review: &review,
        has_open_blocking_children: false,
        parent_iteration: 3,
        max_iterations: 3,
        elapsed: Duration::from_secs(10),
        timeout: Duration::from_secs(300),
    });

    assert_eq!(decision, DoneDecision::NeedsHuman);
}

#[test]
fn decides_needs_human_on_timeout() {
    let evaluator = DoneEvaluator::default();
    let gates = GateReport::all_green();
    let review = ReviewOutput {
        status: ReviewStatus::Done,
        coverage_score: 1.0,
        missing_items: vec![],
        spawn_candidates: vec![],
        risk_flags: vec![],
    };

    let decision = evaluator.decide_parent(&ParentDecisionInput {
        gates: &gates,
        non_blocking_gates: &HashSet::new(),
        review: &review,
        has_open_blocking_children: false,
        parent_iteration: 1,
        max_iterations: 3,
        elapsed: Duration::from_secs(301),
        timeout: Duration::from_secs(300),
    });

    assert_eq!(decision, DoneDecision::NeedsHuman);
}

#[test]
fn decides_child_done_only_when_green() {
    let evaluator = DoneEvaluator::default();
    let done = evaluator.decide_child(
        &GateReport::all_green(),
        &ReviewOutput {
            status: ReviewStatus::Done,
            coverage_score: 1.0,
            missing_items: vec![],
            spawn_candidates: vec![],
            risk_flags: vec![],
        },
        &HashSet::new(),
    );
    assert_eq!(done, DoneDecision::Done);

    let rework = evaluator.decide_child(
        &GateReport {
            fmt_ok: true,
            clippy_ok: true,
            tests_ok: false,
            perf_ok: true,
        },
        &ReviewOutput {
            status: ReviewStatus::Done,
            coverage_score: 1.0,
            missing_items: vec![],
            spawn_candidates: vec![],
            risk_flags: vec![],
        },
        &HashSet::new(),
    );
    assert_eq!(rework, DoneDecision::Rework);
}

#[test]
fn non_blocking_gate_can_still_be_done() {
    let evaluator = DoneEvaluator::default();
    let mut non_blocking = HashSet::new();
    non_blocking.insert(GateName::Clippy);

    let gates = GateReport {
        fmt_ok: true,
        clippy_ok: false,
        tests_ok: true,
        perf_ok: true,
    };
    let review = ReviewOutput {
        status: ReviewStatus::Done,
        coverage_score: 1.0,
        missing_items: vec![],
        spawn_candidates: vec![],
        risk_flags: vec![],
    };

    let decision = evaluator.decide_parent(&ParentDecisionInput {
        gates: &gates,
        non_blocking_gates: &non_blocking,
        review: &review,
        has_open_blocking_children: false,
        parent_iteration: 1,
        max_iterations: 3,
        elapsed: Duration::from_secs(10),
        timeout: Duration::from_secs(300),
    });

    assert_eq!(decision, DoneDecision::Done);
}
