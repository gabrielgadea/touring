//! The runtime execution engine for a configured [`FlowPipeline`].

use crate::flow::flow_result::{FlowResult, OutputTarget, StageOutcome};
use crate::flow::stages::NamedStage;
use crate::flow::types::Item;
use crate::flow::{Error, Result};
use std::path::PathBuf;
use std::time::{Duration, Instant};

/// A configured pipeline that can execute over one or more input items.
///
/// Built via [`TouringFlowBuilder`](super::new_flow_builder::TouringFlowBuilder).
#[derive(Debug)]
pub struct FlowPipeline {
    stages: Vec<NamedStage<Item>>,
    timeout: Option<Duration>,
    output: OutputTarget,
}

impl FlowPipeline {
    /// Construct a new pipeline with the given stages.
    pub fn new(stages: Vec<NamedStage<Item>>, output: OutputTarget) -> Self {
        Self {
            stages,
            timeout: None,
            output,
        }
    }

    /// Set an optional timeout for each stage.
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    /// Run the pipeline over a single input item.
    pub fn run(&self, input: Item) -> FlowResult<Item> {
        let started_at = Instant::now();
        let mut item = input;
        let mut outcomes: Vec<StageOutcome> = Vec::with_capacity(self.stages.len());

        for ns in &self.stages {
            let stage_started = Instant::now();
            let next = ns.stage.process(item.clone());
            match next {
                Ok(out) => {
                    let elapsed = stage_started.elapsed();
                    outcomes.push(StageOutcome::ok(&ns.name, &ns.name, elapsed));
                    item = out;
                }
                Err(e) => {
                    let elapsed = stage_started.elapsed();
                    outcomes.push(StageOutcome::err(&ns.name, elapsed, e.to_string()));
                    let total = started_at.elapsed();
                    return FlowResult::failed(item, outcomes, total);
                }
            }
        }

        let total = started_at.elapsed();
        let result = FlowResult::ok(item, outcomes, total);
        if !result.success {
            let total = started_at.elapsed();
            return FlowResult::failed(result.item, result.stage_outcomes, total);
        }

        if let Err(e) = self.write_output(&result) {
            let mut outcomes = result.stage_outcomes.clone();
            outcomes.push(StageOutcome::err("output", Duration::ZERO, e.to_string()));
            let total = started_at.elapsed();
            return FlowResult::failed(result.item, outcomes, total);
        }

        result
    }

    /// Run the pipeline over a batch of input items.
    pub fn run_batch(&self, inputs: Vec<Item>) -> Vec<FlowResult<Item>> {
        inputs.into_iter().map(|item| self.run(item)).collect()
    }

    /// Returns a reference to the configured stages.
    pub fn stages(&self) -> &[NamedStage<Item>] {
        &self.stages
    }

    /// Returns the configured output target.
    pub fn output_target(&self) -> &OutputTarget {
        &self.output
    }

    fn write_output(&self, result: &FlowResult<Item>) -> Result<()> {
        match &self.output {
            OutputTarget::Stdout => self.write_stdout(result),
            OutputTarget::File(path) => self.write_file(path, result),
            OutputTarget::Json(path) => self.write_json(path, result),
            OutputTarget::Discard => Ok(()),
        }
    }

    fn write_stdout(&self, result: &FlowResult<Item>) -> Result<()> {
        println!("{:?}", result.item);
        Ok(())
    }

    fn write_file(&self, path: &PathBuf, result: &FlowResult<Item>) -> Result<()> {
        std::fs::write(path, format!("{:?}", result.item)).map_err(Error::Io)
    }

    fn write_json(&self, path: &PathBuf, result: &FlowResult<Item>) -> Result<()> {
        let json = serde_json::to_string_pretty(result).map_err(Error::Serialize)?;
        std::fs::write(path, json).map_err(Error::Io)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flow::error::StageError;
    use crate::flow::stages::{Filter, Transform};
    use crate::flow::types::Item;

    fn pipeline() -> FlowPipeline {
        let _filter = Filter::new("only_even", |item: &Item| item.id.contains("even"));
        let _transform = Transform::new(
            "upper",
            |item: Item| -> std::result::Result<Item, StageError> {
                Ok(Item::new(item.id.to_uppercase(), item.label.to_uppercase()))
            },
        );

        FlowPipeline::new(vec![], OutputTarget::default())
    }

    #[test]
    fn run_with_no_stages() {
        let pipeline = pipeline();
        let item = Item::new("id1", "test");
        let result = pipeline.run(item);
        assert!(result.is_ok());
        assert_eq!(result.stage_outcomes.len(), 0);
    }

    #[test]
    fn run_batch_with_no_stages() {
        let pipeline = pipeline();
        let items = vec![Item::new("a", "x"), Item::new("b", "y")];
        let results = pipeline.run_batch(items);
        assert_eq!(results.len(), 2);
        for r in results {
            assert!(r.is_ok());
        }
    }
}
