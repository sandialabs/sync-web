//! Narrow evaluation hooks for external test harnesses.

use crate::evaluator::Primitive;
use crate::extensions::crypto::{primitive_s7_crypto_generate, primitive_s7_crypto_sign};
use crate::persistor::PERSISTOR;
use crate::scenario_context::ScenarioContext;
use crate::{GENESIS_STR, JOURNAL, NULL, Word};
use rand::SeedableRng;
use rand::rngs::StdRng;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex};

pub use crate::scenario_context::ScenarioTransport;

/// Primitives useful to the trusted outer Scheme evaluator in a test harness.
pub fn driver_primitives() -> Vec<Primitive> {
    vec![primitive_s7_crypto_generate(), primitive_s7_crypto_sign()]
}

/// Deterministic context shared by evaluations in one external test scenario.
pub struct ScenarioEnvironment {
    transport: Arc<dyn ScenarioTransport>,
    clock: Arc<AtomicI64>,
    random: Arc<Mutex<StdRng>>,
}

impl ScenarioEnvironment {
    /// Create an environment whose clock begins at Unix zero and whose random
    /// generator uses a fixed seed.
    pub fn new(transport: Arc<dyn ScenarioTransport>) -> Self {
        Self {
            transport,
            clock: Arc::new(AtomicI64::new(0)),
            random: Arc::new(Mutex::new(StdRng::seed_from_u64(0))),
        }
    }

    /// Create an empty journal record with an explicit identifier.
    pub fn create_record(&self, record: Word) -> Result<(), String> {
        PERSISTOR
            .root_new(
                record,
                PERSISTOR
                    .branch_set(
                        PERSISTOR
                            .leaf_set(GENESIS_STR.as_bytes().to_vec())
                            .map_err(|error| format!("Could not create genesis leaf: {error:?}"))?,
                        NULL,
                        NULL,
                    )
                    .map_err(|error| format!("Could not create genesis branch: {error:?}"))?,
            )
            .map(|_| ())
            .map_err(|error| format!("Could not create journal record: {error:?}"))
    }

    /// Set the global discrete scenario time observed by subsequent evaluation.
    pub fn set_time(&self, time: u64) {
        self.clock.store(time as i64, Ordering::SeqCst);
    }

    /// Evaluate one expression against a record under deterministic test
    /// transport, time, and randomness.
    pub fn evaluate(&self, record: Word, action: usize, expression: &str) -> String {
        JOURNAL.evaluate_record_with_context(
            record,
            expression,
            Some(ScenarioContext {
                action,
                transport: self.transport.clone(),
                clock: self.clock.clone(),
                random: self.random.clone(),
            }),
        )
    }
}
