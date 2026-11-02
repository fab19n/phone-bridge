//! Stage 1 validation spikes (docs/DEVELOPMENT_PLAN.md; results in docs/STAGE1_RESULTS.md). Compiled only
//! with the `spikes` feature. The matching page is `app/src/spikes/`, and `scripts/run-spikes.mjs` drives
//! it over the DevTools protocol so the numbers are reproducible.

pub mod adb_probe;
pub mod mirror;
pub mod ordering;
pub mod throughput;
