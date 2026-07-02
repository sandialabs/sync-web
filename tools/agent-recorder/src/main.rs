use anyhow::Result;

fn main() -> Result<()> {
    agent_recorder::cli::run_with_registry(agent_recorder::adapters::AdapterRegistry::builtins())
}
