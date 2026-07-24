//! Workflow system for multi-step operations
//!
//! Workflows orchestrate complex operations that require multiple API calls,
//! waiting for async operations, and conditional logic.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;

pub mod cloud;
pub mod enterprise;

/// Common trait for all workflows
pub trait Workflow: Send + Sync {
    /// Unique identifier for the workflow
    fn name(&self) -> &str;

    /// Human-readable description
    fn description(&self) -> &str;

    /// Execute the workflow with the given arguments
    fn execute(
        &self,
        context: WorkflowContext,
        args: WorkflowArgs,
    ) -> Pin<Box<dyn Future<Output = Result<WorkflowResult>> + Send>>;
}

/// Context provided to workflows for accessing API clients and configuration
#[derive(Clone)]
pub struct WorkflowContext {
    pub conn_mgr: crate::connection::ConnectionManager,
    pub profile_name: Option<String>,
    pub output_format: crate::output::OutputFormat,
}

/// Arguments passed to a workflow
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WorkflowArgs {
    params: HashMap<String, serde_json::Value>,
}

impl WorkflowArgs {
    pub fn new() -> Self {
        Self {
            params: HashMap::new(),
        }
    }

    pub fn insert(&mut self, key: impl Into<String>, value: impl Serialize) {
        self.params
            .insert(key.into(), serde_json::to_value(value).unwrap());
    }

    pub fn get<T: for<'de> Deserialize<'de>>(&self, key: &str) -> Option<T> {
        self.params
            .get(key)
            .and_then(|v| serde_json::from_value(v.clone()).ok())
    }

    pub fn get_string(&self, key: &str) -> Option<String> {
        self.get(key)
    }

    pub fn get_bool(&self, key: &str) -> Option<bool> {
        self.get(key)
    }

    pub fn get_i64(&self, key: &str) -> Option<i64> {
        self.get(key)
    }
}

/// Result of a workflow execution
#[derive(Debug, Serialize, Deserialize)]
pub struct WorkflowResult {
    pub success: bool,
    pub message: String,
    pub outputs: HashMap<String, serde_json::Value>,
}

impl WorkflowResult {
    pub fn success(message: impl Into<String>) -> Self {
        Self {
            success: true,
            message: message.into(),
            outputs: HashMap::new(),
        }
    }

    pub fn with_output(mut self, key: impl Into<String>, value: impl Serialize) -> Self {
        self.outputs
            .insert(key.into(), serde_json::to_value(value).unwrap());
        self
    }
}

/// Registry of available workflows
pub struct WorkflowRegistry {
    workflows: HashMap<String, Box<dyn Workflow>>,
}

impl WorkflowRegistry {
    pub fn new() -> Self {
        let mut registry = Self {
            workflows: HashMap::new(),
        };

        // Register all built-in workflows
        registry.register(Box::new(enterprise::InitClusterWorkflow::new()));
        registry.register(Box::new(
            cloud::subscription_setup::SubscriptionSetupWorkflow,
        ));

        registry
    }

    pub fn register(&mut self, workflow: Box<dyn Workflow>) {
        self.workflows.insert(workflow.name().to_string(), workflow);
    }

    pub fn get(&self, name: &str) -> Option<&dyn Workflow> {
        self.workflows.get(name).map(|w| w.as_ref())
    }

    pub fn list(&self) -> Vec<(&str, &str)> {
        self.workflows
            .values()
            .map(|w| (w.name(), w.description()))
            .collect()
    }
}

impl Default for WorkflowRegistry {
    fn default() -> Self {
        Self::new()
    }
}
