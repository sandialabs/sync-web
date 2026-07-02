use std::collections::HashMap;
use std::path::Path;

use anyhow::Result;

use crate::GraphRecord;

/// Local flat JSON Lines storage.
pub mod jsonl;
/// OpenTelemetry logs exporter.
pub mod otel;
/// Sync Web gateway/direct-journal storage.
pub mod sync_web;

/// Absolute backend index selector for readable record backends.
///
/// Ranges are half-open: `start <= index < end`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordSelector {
    Index(u64),
    Range { start: u64, end: u64 },
}

impl RecordSelector {
    pub fn contains(self, index: u64) -> bool {
        match self {
            Self::Index(target) => index == target,
            Self::Range { start, end } => start <= index && index < end,
        }
    }

    pub fn is_before(self, index: u64) -> bool {
        match self {
            Self::Index(target) => index < target,
            Self::Range { start, .. } => index < start,
        }
    }

    pub fn is_after(self, index: u64) -> bool {
        match self {
            Self::Index(target) => index > target,
            Self::Range { end, .. } => index >= end,
        }
    }
}

/// A graph record paired with its absolute backend index.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct IndexedGraphRecord {
    pub index: u64,
    pub record: GraphRecord,
}

/// Writable destination for normalized graph records.
pub trait RecordAdapter {
    /// Stable adapter name used by CLI/config.
    fn name(&self) -> &'static str;
    /// Append or otherwise persist one normalized record.
    fn log(&mut self, record: &GraphRecord) -> Result<()>;
}

/// Readable backend used by `read`, `verify`, `status`, and integrity recovery.
pub trait RecordReader {
    /// Stable reader name used by CLI/config.
    fn name(&self) -> &'static str;
    /// Emit indexed records matching `selector` in ascending index order.
    fn read(
        &self,
        selector: RecordSelector,
        emit: &mut dyn FnMut(IndexedGraphRecord) -> Result<()>,
    ) -> Result<()>;
}

/// Registry of writable record adapters and readable backends.
pub struct RecordRegistry {
    factories: HashMap<String, Box<dyn Fn(&str) -> Result<Box<dyn RecordAdapter>>>>,
    reader_factories: HashMap<String, Box<dyn Fn(&str) -> Result<Box<dyn RecordReader>>>>,
}

impl RecordRegistry {
    pub fn new() -> Self {
        Self {
            factories: HashMap::new(),
            reader_factories: HashMap::new(),
        }
    }

    /// Return all built-in record adapters and readers shipped by this crate.
    pub fn builtins() -> Self {
        Self::new()
            .with("file", |out| Ok(Box::new(jsonl_file(Path::new(out))?)))
            .with_reader("file", |input| {
                Ok(Box::new(jsonl_reader(Path::new(input))?))
            })
            .with_reader("sync-web", |_input| {
                anyhow::bail!("sync-web reader requires Sync Web auth options; use the built-in CLI --sync-web-* flags")
            })
            .with("otel", |out| {
                Ok(Box::new(otel::OtelRecordAdapter::create(out)?))
            })
            .with("sync-web", |_out| {
                anyhow::bail!("sync-web recorder requires Sync Web auth options; use the built-in CLI --sync-web-* flags")
            })
    }

    pub fn with(
        mut self,
        name: impl Into<String>,
        factory: impl Fn(&str) -> Result<Box<dyn RecordAdapter>> + 'static,
    ) -> Self {
        self.factories.insert(name.into(), Box::new(factory));
        self
    }

    pub fn with_reader(
        mut self,
        name: impl Into<String>,
        factory: impl Fn(&str) -> Result<Box<dyn RecordReader>> + 'static,
    ) -> Self {
        self.reader_factories.insert(name.into(), Box::new(factory));
        self
    }

    pub fn create(&self, name: &str, out: &str) -> Option<Result<Box<dyn RecordAdapter>>> {
        self.factories.get(name).map(|factory| factory(out))
    }

    pub fn create_reader(&self, name: &str, input: &str) -> Option<Result<Box<dyn RecordReader>>> {
        self.reader_factories
            .get(name)
            .map(|factory| factory(input))
    }

    pub fn names(&self) -> Vec<&str> {
        let mut names = self
            .factories
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>();
        names.sort_unstable();
        names
    }

    pub fn reader_names(&self) -> Vec<&str> {
        let mut names = self
            .reader_factories
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>();
        names.sort_unstable();
        names
    }
}

impl Default for RecordRegistry {
    fn default() -> Self {
        Self::builtins()
    }
}

/// Create a local flat JSONL writer.
pub fn jsonl_file(path: &Path) -> Result<jsonl::JsonlRecordAdapter> {
    jsonl::JsonlRecordAdapter::create(path)
}

/// Create a local flat JSONL reader.
pub fn jsonl_reader(path: &Path) -> Result<jsonl::JsonlRecordReader> {
    jsonl::JsonlRecordReader::create(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_recorders_include_file_and_otel() {
        let registry = RecordRegistry::builtins();
        let names = registry.names();
        assert_eq!(names, vec!["file", "otel", "sync-web"]);
    }

    #[test]
    fn builtin_readers_include_file() {
        let registry = RecordRegistry::builtins();
        let names = registry.reader_names();
        assert_eq!(names, vec!["file", "sync-web"]);
    }
}
