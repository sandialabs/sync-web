use super::*;

pub(super) struct Compiled {
    pub(super) engine: Engine,
    pub(super) module: Module,
    pub(super) artifact_sha256: String,
    pub(super) memory_minimum_bytes: u64,
}
static COMPILED: Lazy<Mutex<Option<Arc<Compiled>>>> = Lazy::new(|| Mutex::new(None));
pub(super) fn read_artifact(path: &str) -> Result<Vec<u8>, String> {
    let file = std::fs::File::open(path).map_err(|error| error.to_string())?;
    let length = file.metadata().map_err(|error| error.to_string())?.len();
    if length > AOT_ARTIFACT_LIMIT {
        return Err("Wasmer kernel artifact exceeds 64 MiB".into());
    }
    let mut artifact = Vec::with_capacity(usize::try_from(length).unwrap_or(0));
    file.take(AOT_ARTIFACT_LIMIT + 1)
        .read_to_end(&mut artifact)
        .map_err(|error| error.to_string())?;
    if artifact.len() as u64 > AOT_ARTIFACT_LIMIT {
        return Err("Wasmer kernel artifact exceeds 64 MiB".into());
    }
    Ok(artifact)
}

pub(super) fn compiled() -> Result<Arc<Compiled>, String> {
    if std::env::consts::OS != "linux" || std::env::consts::ARCH != "x86_64" {
        return Err("Journal request evaluation is unsupported on this target".into());
    }
    Lazy::force(&INTERRUPT_WATCHER);
    let mut cache = COMPILED.lock().map_err(|_| "Wasm module cache poisoned")?;
    if let Some(compiled) = cache.as_ref() {
        return Ok(compiled.clone());
    }
    let path = std::env::var("SYNC_WEB_WASMER_KERNEL")
        .map_err(|_| "SYNC_WEB_WASMER_KERNEL is required")?;
    if !path.ends_with(".wasmer") {
        return Err("headless Wasmer runtime requires a trusted .wasmer artifact".into());
    }
    let expected_sha256 = std::env::var("SYNC_WEB_WASMER_KERNEL_SHA256")
        .map_err(|_| "SYNC_WEB_WASMER_KERNEL_SHA256 is required")?;
    if expected_sha256.len() != 64
        || !expected_sha256
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err("SYNC_WEB_WASMER_KERNEL_SHA256 must be lowercase SHA-256".into());
    }
    let artifact = read_artifact(&path)?;
    let artifact_sha256 = format!("{:x}", Sha256::digest(&artifact));
    if artifact_sha256 != expected_sha256 {
        return Err("Wasmer kernel SHA-256 mismatch".into());
    }
    let mut engine: Engine = EngineBuilder::headless().into();
    let tunables = BoundedTunables(BaseTunables::for_target(engine.target()));
    engine.set_tunables(tunables);
    let store = Store::new(engine.clone());
    let module =
        unsafe { Module::deserialize(&store, artifact) }.map_err(|error| error.to_string())?;
    let memory = module
        .exports()
        .find(|export| export.name() == "memory")
        .ok_or("Wasmer kernel does not export memory")?;
    let memory_minimum_bytes = match memory.ty() {
        ExternType::Memory(memory)
            if memory.maximum == Some(MEMORY_MAXIMUM)
                && u64::from(MEMORY_MAXIMUM.0) * WASM_PAGE_SIZE as u64 == MEMORY_MAXIMUM_BYTES =>
        {
            u64::from(memory.minimum.0) * 64 * 1024
        }
        _ => return Err("Wasmer kernel memory maximum must be 512 MiB".into()),
    };
    let compiled = Arc::new(Compiled {
        engine,
        module,
        artifact_sha256,
        memory_minimum_bytes,
    });
    *cache = Some(compiled.clone());
    Ok(compiled)
}
