mod crypto;
mod schedule;
mod signing;
mod state;
mod verify;

use std::path::{Path, PathBuf};

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::{
    records::{RecordAdapter, RecordReader},
    GraphRecord,
};

pub use signing::sign_record;
pub use state::{integrity_status, load_state, rekey_state};
pub use verify::{verify_indexed_record, verify_indexed_records, verify_record};

use schedule::{canonical_event_key, generated_future_keys};
use signing::sign_record_with_event_key;
use state::{
    advance_state, consume_event_keys, load_or_create_state, reconcile_state, store_state,
};

/// Public algorithm identifier for the record integrity metadata shape.
pub const ALGORITHM: &str = "agent-recorder-integrity-v1";

/// Public per-record integrity metadata.
///
/// This is deliberately minimal and safe to store with backend records. Local
/// future keys/frontier state are private and never published in normal records.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub struct IntegrityMetadata {
    pub algorithm: String,
    pub key_id: String,
    pub index: u64,
    pub payload_hash: String,
    pub authenticator: String,
}

/// Private mutable key-evolution state stored outside record backends.
///
/// The state tracks the next absolute record index and pending future edge keys
/// for the no-horizon v2 skip schedule. It must be protected like signing key
/// material.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct IntegrityState {
    pub algorithm: String,
    pub key_id: String,
    pub next_index: u64,
    pub future_keys: Vec<IntegrityFutureKey>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct IntegrityFutureKey {
    pub source: u64,
    pub target: u64,
    pub level: u8,
    pub key: String,
}

/// [`RecordAdapter`] wrapper that signs records before delegating writes.
///
/// On append, it consumes the pending key for the current absolute index,
/// attaches [`IntegrityMetadata`], writes the record to the inner adapter, and
/// only then advances the local state file. `create_checked` can reconcile the
/// single crash window where the backend write succeeded but local state was not
/// advanced.
pub struct IntegrityRecordAdapter {
    inner: Box<dyn RecordAdapter>,
    state_path: PathBuf,
    state: IntegrityState,
}

/// Root verifier/signing secret for one integrity stream.
///
/// The root secret is never stored in normal records. Verification derives the
/// per-index event key from this root in logarithmic time using the same skip
/// schedule used by the writer's local frontier state.
#[derive(Debug, Clone)]
pub struct IntegrityKey {
    root_secret: Vec<u8>,
    key_id: String,
}

/// Verification result category for one indexed record.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum VerificationStatus {
    Verified,
    MissingIntegrity,
    IndexMismatch,
    PayloadHashMismatch,
    AuthenticatorMismatch,
    UnsupportedAlgorithm,
}

/// Structured verification result returned by non-throwing record checks.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub struct VerificationResult {
    pub index: u64,
    pub status: VerificationStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// Relationship between local integrity state and backend record count.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum IntegrityAlignment {
    Aligned,
    OneStepRepairable,
    StateAheadOfBackend,
    BackendTooFarAhead,
}

/// Status report for preflight/recovery checks.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub struct IntegrityStatus {
    pub algorithm: String,
    pub key_id: String,
    pub state_next_index: u64,
    pub backend_next_index: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backend_latest_index: Option<u64>,
    pub pending_key_count: usize,
    pub alignment: IntegrityAlignment,
}

impl IntegrityRecordAdapter {
    /// Create a signing wrapper without consulting the backend.
    pub fn create(
        inner: Box<dyn RecordAdapter>,
        state_path: impl AsRef<Path>,
        init_key: Option<IntegrityKey>,
    ) -> Result<Self> {
        Self::create_checked(inner, state_path, init_key, None)
    }

    /// Create a signing wrapper and optionally reconcile state against a reader.
    ///
    /// If the backend is exactly one record ahead, the missing local state
    /// advance is repaired from pending keys and the backend record. Larger
    /// divergences are refused to preserve deletion/forward-integrity semantics.
    pub fn create_checked(
        inner: Box<dyn RecordAdapter>,
        state_path: impl AsRef<Path>,
        init_key: Option<IntegrityKey>,
        reader: Option<&dyn RecordReader>,
    ) -> Result<Self> {
        let state_path = state_path.as_ref().to_path_buf();
        let mut state = load_or_create_state(&state_path, init_key.as_ref())?;
        if let Some(reader) = reader {
            if reconcile_state(reader, &mut state)? {
                store_state(&state_path, &state)?;
            }
        }
        Ok(Self {
            inner,
            state_path,
            state,
        })
    }
}

impl RecordAdapter for IntegrityRecordAdapter {
    fn name(&self) -> &'static str {
        self.inner.name()
    }

    fn log(&mut self, record: &GraphRecord) -> Result<()> {
        let index = self.state.next_index;
        let consumed = consume_event_keys(&mut self.state, index)?;
        let event_key = canonical_event_key(index, &consumed)?;
        let generated = generated_future_keys(index, &event_key);
        let signed = sign_record_with_event_key(record, index, &self.state.key_id, &event_key)?;
        self.inner.log(&signed)?;

        advance_state(&mut self.state, index, generated)?;
        store_state(&self.state_path, &self.state)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::{
        EdgeType, GraphEdge, GraphNode, GraphNodeType, Message, RecordType, Role, SourceRef,
    };

    use super::*;
    use crate::integrity::{schedule::generated_future_keys, signing::canonical_payload_bytes};

    fn sample_record(content: &str) -> GraphRecord {
        GraphRecord {
            record_type: RecordType::AgentRecord,
            node: GraphNode::Message(Message {
                node_type: GraphNodeType::Message,
                id: "msg_test".to_string(),
                timestamp: None,
                role: Role::User,
                content: content.to_string(),
                cwd: None,
                provider: None,
                model: None,
                tool_calls: Vec::new(),
                tool_results: Vec::new(),
                source: SourceRef {
                    agent_adapter: "test".to_string(),
                    path: None,
                    locator: None,
                },
                metadata: json!({}),
            }),
            edges: Vec::new(),
            integrity: None,
        }
    }

    #[test]
    fn signs_and_verifies_record() -> Result<()> {
        let key = IntegrityKey::from_secret("secret");
        let signed = sign_record(&sample_record("hello"), 13, &key)?;
        let verification = verify_record(13, &signed, &key)?;
        assert_eq!(verification.status, VerificationStatus::Verified);
        assert_eq!(signed.integrity.as_ref().unwrap().algorithm, ALGORITHM);
        Ok(())
    }

    #[test]
    fn verifies_after_json_round_trip() -> Result<()> {
        let key = IntegrityKey::from_secret("secret");
        let signed = sign_record(&sample_record("hello"), 1, &key)?;
        let line = serde_json::to_string(&signed)?;
        let round_trip = serde_json::from_str::<GraphRecord>(&line)?;
        let verification = verify_record(1, &round_trip, &key)?;
        assert_eq!(verification.status, VerificationStatus::Verified, "{line}");
        Ok(())
    }

    #[test]
    fn detects_payload_tampering() -> Result<()> {
        let key = IntegrityKey::from_secret("secret");
        let mut signed = sign_record(&sample_record("hello"), 1, &key)?;
        if let GraphNode::Message(message) = &mut signed.node {
            message.content = "tampered".to_string();
        }
        let verification = verify_record(1, &signed, &key)?;
        assert_eq!(verification.status, VerificationStatus::PayloadHashMismatch);
        Ok(())
    }

    #[test]
    fn detects_edge_tampering() -> Result<()> {
        let key = IntegrityKey::from_secret("secret");
        let mut record = sample_record("hello");
        record.edges.push(GraphEdge {
            edge_type: EdgeType::FollowsMessage,
            target: "msg_previous".to_string(),
            metadata: json!({}),
        });
        let mut signed = sign_record(&record, 1, &key)?;
        signed.edges[0].target = "msg_tampered".to_string();
        let verification = verify_record(1, &signed, &key)?;
        assert_eq!(verification.status, VerificationStatus::PayloadHashMismatch);
        Ok(())
    }

    #[test]
    fn payload_hash_omits_integrity_block() -> Result<()> {
        let key = IntegrityKey::from_secret("secret");
        let mut signed = sign_record(&sample_record("hello"), 1, &key)?;
        let original = canonical_payload_bytes(&signed)?;
        signed.integrity.as_mut().unwrap().authenticator = "tampered".to_string();
        assert_eq!(canonical_payload_bytes(&signed)?, original);
        let verification = verify_record(1, &signed, &key)?;
        assert_eq!(
            verification.status,
            VerificationStatus::AuthenticatorMismatch
        );
        Ok(())
    }

    #[test]
    fn derives_event_keys_by_no_horizon_skip_path() {
        let key = IntegrityKey::from_secret("secret");
        assert_ne!(key.event_key(13), key.event_key(12));
        assert_eq!(key.event_key(13), key.event_key(13));
    }

    #[test]
    fn generates_no_horizon_v2_skip_keys() {
        let event_key = [7; 32];

        let from_1 = generated_future_keys(0, &event_key);
        assert_eq!(edges(&from_1), vec![(0, 1, 0)]);

        let from_2 = generated_future_keys(1, &event_key);
        assert_eq!(edges(&from_2), vec![(1, 2, 0), (1, 3, 1)]);

        let from_3 = generated_future_keys(2, &event_key);
        assert_eq!(edges(&from_3), vec![(2, 3, 0)]);

        let from_4 = generated_future_keys(3, &event_key);
        assert_eq!(edges(&from_4), vec![(3, 4, 0), (3, 5, 1), (3, 7, 2)]);

        let from_8 = generated_future_keys(7, &event_key);
        assert_eq!(
            edges(&from_8),
            vec![(7, 8, 0), (7, 9, 1), (7, 11, 2), (7, 15, 3)]
        );
    }

    fn edges(keys: &[IntegrityFutureKey]) -> Vec<(u64, u64, u8)> {
        keys.iter()
            .map(|key| (key.source, key.target, key.level))
            .collect()
    }
}
