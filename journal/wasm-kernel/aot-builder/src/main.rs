use enumset::EnumSet;
use std::path::PathBuf;
use std::str::FromStr;
use target_lexicon::Triple;
use wasmer::{Engine, ExternType, Module, Pages, Store};
use wasmer_compiler::{CompilerConfig, EngineBuilder};
use wasmer_compiler_llvm::LLVM;
use wasmer_types::Features;

const MEMORY_MAXIMUM: Pages = Pages(8192);

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut arguments = std::env::args_os().skip(1);
    let input = PathBuf::from(arguments.next().ok_or("missing input .wasm")?);
    let output = PathBuf::from(arguments.next().ok_or("missing output .wasmer")?);
    if arguments.next().is_some() {
        return Err("usage: sync-web-wasmer-aot-builder INPUT.wasm OUTPUT.wasmer".into());
    }

    let bytes = std::fs::read(&input)?;
    let mut features = Features::new();
    features.exceptions(true);
    let mut compiler = LLVM::new();
    compiler.canonicalize_nans(false);
    let engine: Engine = EngineBuilder::new(compiler)
        .set_features(Some(features))
        .set_target(Some(wasmer::sys::Target::new(
            Triple::from_str("x86_64-unknown-linux-gnu").map_err(|_| "invalid target triple")?,
            EnumSet::empty(),
        )))
        .engine()
        .into();
    let store = Store::new(engine);
    let module = Module::new(&store, bytes)?;
    let memory = module
        .exports()
        .find(|export| export.name() == "memory")
        .ok_or("kernel does not export memory")?;
    match memory.ty() {
        ExternType::Memory(memory) if memory.maximum == Some(MEMORY_MAXIMUM) => {}
        _ => return Err("kernel memory maximum must be exactly 512 MiB".into()),
    }
    module.serialize_to_file(output)?;
    Ok(())
}
