//! Declarative builder for [`FlowPipeline`].

use crate::flow::flow_pipeline::FlowPipeline;
use crate::flow::flow_result::OutputTarget;
use crate::flow::stages::{NamedStage, Stage};
use crate::flow::types::Item;
use std::time::Duration;

/// A fluent builder for constructing a [`FlowPipeline`].
#[derive(Debug, Default)]
pub struct TouringFlowBuilder {
    stages: Vec<NamedStage<Item>>,
    timeout: Option<Duration>,
    output: OutputTarget,
}

impl TouringFlowBuilder {
    /// Begin building a new pipeline.
    pub fn new() -> Self {
        Self::default()
    }

    /// Append a stage to the pipeline.
    ///
    /// The stage's output becomes the next stage's input.
    pub fn add_stage<S>(mut self, name: impl Into<String>, stage: S) -> Self
    where
        S: Stage<Item> + 'static,
    {
        self.stages.push(NamedStage::new(name, Box::new(stage)));
        self
    }

    /// Set a per-stage timeout.
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    /// Set the output target for pipeline results.
    pub fn with_output_target(mut self, target: OutputTarget) -> Self {
        self.output = target;
        self
    }

    /// Consume the builder and produce a runnable pipeline.
    pub fn build(self) -> FlowPipeline {
        let mut pipeline = FlowPipeline::new(self.stages, self.output);
        if let Some(t) = self.timeout {
            pipeline = pipeline.with_timeout(t);
        }
        pipeline
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flow::stages::{Filter, Transform};
    use crate::flow::types::Item;

    #[test]
    fn empty_pipeline() {
        let pipeline = TouringFlowBuilder::new().build();
        let item = Item::new("id1", "label");
        let result = pipeline.run(item);
        assert!(result.is_ok());
    }

    #[test]
    fn pipeline_with_filter_and_transform() {
        let pipeline = TouringFlowBuilder::new()
            .add_stage(
                "filter",
                Filter::new("even", |item: &Item| item.id.starts_with("even")),
            )
            .add_stage(
                "transform",
                Transform::new("upper", |item: Item| {
                    Ok(Item::new(item.id.to_uppercase(), item.label.to_uppercase()))
                }),
            )
            .build();

        let item = Item::new("even_1", "test");
        let result = pipeline.run(item);
        assert!(result.is_ok());
        assert_eq!(result.stage_outcomes.len(), 2);
    }

    #[test]
    fn pipeline_with_output_target() {
        let pipeline = TouringFlowBuilder::new()
            .with_output_target(OutputTarget::Discard)
            .build();

        let item = Item::new("id1", "label");
        let result = pipeline.run(item);
        assert!(result.is_ok());
    }
}
