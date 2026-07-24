# ENGINEERING JOURNAL & DISCOVERY LOG

*This log captures implementation discoveries, benchmark anomalies, rejected approaches, and unexpected constraints encountered during execution. It serves as the precursor and context for formal Architecture Decision Records (ADRs).*

---

### M1: Core Foundation & Sandbox

**[Date: 2026-07-23] - Initialization & Strategy**
*   **Hypothesis:** `deno_core` will allow us to boot a V8 isolate in < 5ms while maintaining strict capability isolation.
*   **Action Plan:** Focus exclusively on bringing up the minimal runtime, manifest parsing, capability registration, and a trivial JS execution test.
*   **Notes:** We are intentionally deferring all IPC and React Reconciler work until the core sandbox limits (memory quotas, watchdog termination) are fully proven via automated benchmarks.
