use std::time::Duration;

use std::collections::HashSet;

use crate::contracts::{DoneDecision, GateName, GateReport, ReviewOutput, ReviewStatus};

#[derive(Debug, Clone)]
pub struct DoneEvaluatorConfig {
    pub coverage_threshold: f64,
}

impl Default for DoneEvaluatorConfig {
    fn default() -> Self {
        Self {
            coverage_threshold: 0.9,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct DoneEvaluator {
    pub config: DoneEvaluatorConfig,
}

#[derive(Debug, Clone)]
pub struct ParentDecisionInput<'a> {
    pub gates: &'a GateReport,
    pub non_blocking_gates: &'a HashSet<GateName>,
    pub review: &'a ReviewOutput,
    pub has_open_blocking_children: bool,
    pub parent_iteration: u32,
    pub max_iterations: u32,
    pub elapsed: Duration,
    pub timeout: Duration,
}

impl DoneEvaluator {
    pub fn decide_parent(&self, input: &ParentDecisionInput<'_>) -> DoneDecision {
        if input.elapsed >= input.timeout {
            return DoneDecision::NeedsHuman;
        }

        let ready_for_done = input
            .gates
            .all_passed_with_non_blocking(input.non_blocking_gates)
            && !input.has_open_blocking_children
            && input.review.coverage_score >= self.config.coverage_threshold
            && input.review.missing_items.is_empty()
            && matches!(input.review.status, ReviewStatus::Done);

        if ready_for_done {
            DoneDecision::Done
        } else if input.parent_iteration >= input.max_iterations {
            DoneDecision::NeedsHuman
        } else {
            DoneDecision::Rework
        }
    }

    pub fn decide_child(
        &self,
        gates: &GateReport,
        review: &ReviewOutput,
        non_blocking_gates: &HashSet<GateName>,
    ) -> DoneDecision {
        let ready_for_done = gates.all_passed_with_non_blocking(non_blocking_gates)
            && review.coverage_score >= self.config.coverage_threshold
            && review.missing_items.is_empty()
            && matches!(review.status, ReviewStatus::Done);

        if ready_for_done {
            DoneDecision::Done
        } else {
            DoneDecision::Rework
        }
    }
}
