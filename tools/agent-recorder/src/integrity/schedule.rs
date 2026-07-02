use anyhow::{bail, Result};

use super::{
    crypto::{b64, decode_key, hmac_bytes},
    IntegrityFutureKey, IntegrityKey,
};

impl IntegrityKey {
    /// Build an integrity key from operator-supplied secret bytes.
    ///
    /// The derived key id is deterministic for the secret and is stored in
    /// public integrity metadata so verifiers can choose the right root secret.
    pub fn from_secret(secret: impl AsRef<[u8]>) -> Self {
        let root_secret = secret.as_ref().to_vec();
        let key_id = b64(&hmac_bytes(
            &root_secret,
            &[b"agent-recorder/integrity/key-id/v1"],
        ));
        Self {
            root_secret,
            key_id,
        }
    }

    /// Public key identifier derived from the root secret.
    pub fn key_id(&self) -> &str {
        &self.key_id
    }

    pub(crate) fn initial_event_key(&self) -> [u8; 32] {
        hmac_bytes(
            &self.root_secret,
            &[
                b"agent-recorder/integrity/event-key/v1",
                &0u64.to_be_bytes(),
            ],
        )
    }

    pub(crate) fn event_key(&self, index: u64) -> [u8; 32] {
        let mut key = self.initial_event_key();
        let mut schedule_position = 1u64;
        let schedule_target = index + 1;
        while schedule_position < schedule_target {
            let remaining = schedule_target - schedule_position;
            let max_step = highest_power_of_two_at_most(remaining.min(lowbit(schedule_position)));
            let level = 63 - max_step.leading_zeros() as u8;
            let step = 1u64 << level;
            let next = schedule_position + step;
            key = jump_key(&key, schedule_position - 1, level, next - 1);
            schedule_position = next;
        }
        key
    }
}

pub(crate) fn canonical_event_key(index: u64, consumed: &[IntegrityFutureKey]) -> Result<[u8; 32]> {
    if let Some(key) = consumed
        .iter()
        .find(|key| key.source == index && key.target == index)
    {
        return decode_key(&key.key);
    }

    let (source, level) = canonical_predecessor(index)?;
    let Some(key) = consumed
        .iter()
        .find(|key| key.source == source && key.target == index && key.level == level)
    else {
        bail!("integrity state is missing canonical event key for index {index}");
    };
    decode_key(&key.key)
}

pub(crate) fn generated_future_keys(index: u64, event_key: &[u8; 32]) -> Vec<IntegrityFutureKey> {
    let schedule_index = index + 1;
    let height = schedule_index.trailing_zeros() as u8;
    (0..=height)
        .filter_map(|level| {
            let target = schedule_index.checked_add(1u64 << level)?;
            Some(IntegrityFutureKey {
                source: index,
                target: target - 1,
                level,
                key: b64(&jump_key(event_key, index, level, target - 1)),
            })
        })
        .collect()
}

fn canonical_predecessor(index: u64) -> Result<(u64, u8)> {
    let target = index + 1;
    let mut position = 1u64;
    let mut predecessor = None;
    while position < target {
        let remaining = target - position;
        let max_step = remaining.min(lowbit(position));
        let level = 63 - max_step.leading_zeros() as u8;
        let step = 1u64 << level;
        let next = position + step;
        predecessor = Some((position - 1, level));
        position = next;
    }
    predecessor.ok_or_else(|| anyhow::anyhow!("index 0 has no predecessor"))
}

fn lowbit(value: u64) -> u64 {
    value & value.wrapping_neg()
}

fn highest_power_of_two_at_most(value: u64) -> u64 {
    1u64 << (63 - value.leading_zeros())
}

fn jump_key(key: &[u8], index: u64, depth: u8, target: u64) -> [u8; 32] {
    hmac_bytes(
        key,
        &[
            b"agent-recorder/integrity/jump/v1",
            &[depth],
            &index.to_be_bytes(),
            &target.to_be_bytes(),
        ],
    )
}
