//! # Agent Module — AI-Native Intent Orchestration
//!
//! The agentic controller that transforms natural-language queries into
//! deterministic, capability-gated task graphs executed by the runtime kernel.
//!
//! ## Pipeline
//! ```text
//! Query → PatternEngine → AgentIntent → TaskPlanner → TaskGraph
//!       → Executor → AutomationActions → RuntimeEvents
//! ```
//!
//! ## Design Principles
//! - **Offline-first**: Rule-based intent classification, no LLM required
//! - **Deterministic**: All plans are journalable and replayable
//! - **Capability-gated**: Every action flows through the security broker

pub mod patterns;
pub mod task_graph;
pub mod planner;
pub mod executor;
pub mod controller;
pub mod memory;
pub mod context;

pub use controller::AgentController;
pub use patterns::{AgentIntent, PatternEngine, SystemCommand};
pub use task_graph::{TaskGraph, TaskNode, TaskNodeKind, TaskStatus};
pub use planner::TaskPlanner;
pub use executor::AgentExecutor;
pub use memory::AgentMemory;
pub use context::ContextEngine;
