# Private Beta Exit Metrics

This document defines the quantitative and qualitative success criteria for the SuperSearch Extension Platform Private Beta. These metrics do not block the *start* of the beta; rather, they define what must be achieved during the beta to confidently progress to General Availability (GA).

## 1. Stability & Reliability

| Metric | Target | Measurement Method |
|---|---|---|
| **Crash-free sessions (Host)** | ≥ 99.9% | Automated crash reporting from the Tauri host process. |
| **Crash-free sessions (Isolate)** | ≥ 99.5% | Telemetry on V8 Isolate unexpected terminations/OOMs. |
| **Critical Bugs** | 0 Open | Issue tracker (Severity: Critical/P0). |

## 2. Core Extension Lifecycle

| Metric | Target | Measurement Method |
|---|---|---|
| **Extension Install Success** | ≥ 99.0% | Telemetry on `registry.install()` success vs. error rates. |
| **Extension Launch Success** | ≥ 99.5% | Telemetry on `launch_extension()` successfully returning the first `UiSync` envelope. |
| **Search Latency (P95)** | ≤ 50ms | Time from keystroke to merged extension results appearing in the UI. |

## 3. Developer Experience (DX)

| Metric | Target | Measurement Method |
|---|---|---|
| **Time to First Extension** | ≤ 15 minutes | Measured from clone/install to a successful local render during onboarding sessions. |
| **SDK Adoption** | Positive Feedback | Qualitative feedback on `@supersearch/api` ergonomics via developer surveys. |
| **Manifest Under-scoping** | < 10% | Percentage of dev-mode runs failing due to unrequested permissions (measures clarity of security model). |

## Progression to GA

Once the platform sustains these metrics over a 14-day rolling window with at least 10 active external developers, the platform will be considered operationally proven and ready for General Availability.
