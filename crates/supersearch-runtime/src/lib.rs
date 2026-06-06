//! # SuperSearch Runtime Kernel
//!
//! AI-Native Productivity Operating Layer — the deterministic, capability-secured
//! desktop runtime kernel. This crate provides the foundational scheduling,
//! supervision, and capability mediation infrastructure.
//!
//! ## Architecture Invariants
//!
//! 1. **Deterministic Execution**: Every runtime action is journaled. Replays
//!    use literal token streams and exact tool payloads — no live inference.
//! 2. **Capability Injection**: Plugins receive only explicitly granted,
//!    revocable, namespaced capabilities. Zero implicit trust.
//! 3. **Decoupled Governance**: The scheduler owns time-slicing exclusively.
//!    Token budgets, inference ceilings, and quota monitoring are external
//!    middleware concerns — never baked into the scheduling loop.
//!
//! ## Module Map
//!
//! | Module      | Responsibility                                          |
//! |-------------|---------------------------------------------------------|
//! | `scheduler` | Multi-queue cooperative scheduling + supervision        |
//! | `journal`   | Append-only event journal for deterministic replay      |
//! | `capability`| Namespace-scoped capability injection and mediation     |
//! | `reactive`  | Dependency graph with topological evaluation            |
//! | `plugin`    | Sandboxed WASM adapter plugin runtime                   |
//! | `kernel`    | Privileged OS automation primitives                     |

pub mod scheduler;
pub mod journal;
pub mod capability;
pub mod reactive;
pub mod plugin;
pub mod kernel;
pub mod agent;
pub mod extension;
