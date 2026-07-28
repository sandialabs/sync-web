use crate::Word;
use rand::RngCore;
use rand::rngs::StdRng;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex};

pub trait ScenarioTransport: Send + Sync {
    fn remote(
        &self,
        action: usize,
        source: Word,
        url: String,
        body: String,
    ) -> Result<Vec<u8>, String>;
}

#[derive(Clone)]
pub(crate) struct ScenarioContext {
    pub(crate) action: usize,
    pub(crate) transport: Arc<dyn ScenarioTransport>,
    pub(crate) clock: Arc<AtomicI64>,
    pub(crate) random: Arc<Mutex<StdRng>>,
}

impl ScenarioContext {
    pub(crate) fn unix_time(&self) -> i64 {
        self.clock.load(Ordering::SeqCst)
    }

    pub(crate) fn random_bytes(&self, length: usize) -> Vec<u8> {
        let mut bytes = vec![0; length];
        self.random
            .lock()
            .expect("Scenario random generator lock was poisoned")
            .fill_bytes(&mut bytes);
        bytes
    }
}
