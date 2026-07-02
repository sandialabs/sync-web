use std::path::PathBuf;

use agent_recorder::adapters::{AdapterRegistry, AgentAdapter, ReadHint};
use agent_recorder::GraphRecord;
use anyhow::Result;

struct MyAgentAdapter;

impl AgentAdapter for MyAgentAdapter {
    fn name(&self) -> &'static str {
        "my-custom-agent"
    }

    fn read(
        &self,
        _roots: &[PathBuf],
        _hint: ReadHint,
        _emit: &mut dyn FnMut(GraphRecord) -> Result<()>,
    ) -> Result<()> {
        // Parse custom source artifacts here and call:
        // emit(record)?;
        Ok(())
    }
}

fn main() -> Result<()> {
    let registry = AdapterRegistry::builtins().with("my-custom-agent", || Box::new(MyAgentAdapter));
    agent_recorder::cli::run_with_registry(registry)
}
