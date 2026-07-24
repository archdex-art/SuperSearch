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

pub mod context;
pub mod controller;
pub mod executor;
pub mod mcp;
pub mod memory;
pub mod patterns;
pub mod planner;
pub mod task_graph;

pub use context::ContextEngine;
pub use controller::AgentController;
pub use executor::AgentExecutor;
pub use memory::AgentMemory;
pub use patterns::{AgentIntent, PatternEngine, SystemCommand};
pub use planner::TaskPlanner;
pub use task_graph::{TaskGraph, TaskNode, TaskNodeKind, TaskStatus};
