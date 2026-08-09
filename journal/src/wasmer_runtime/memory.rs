use super::capabilities::*;
use super::*;

thread_local! {
    static CURRENT_MEMORY_RESERVATION: RefCell<Option<Arc<MemoryReservation>>> = const { RefCell::new(None) };
}

pub(super) struct ReservationContext(Option<Arc<MemoryReservation>>);

impl ReservationContext {
    pub(super) fn enter(reservation: Arc<MemoryReservation>) -> Self {
        Self(CURRENT_MEMORY_RESERVATION.with(|current| current.replace(Some(reservation))))
    }
}

impl Drop for ReservationContext {
    fn drop(&mut self) {
        CURRENT_MEMORY_RESERVATION.with(|current| {
            current.replace(self.0.take());
        });
    }
}

struct BoundedMemory {
    inner: Box<dyn LinearMemory + Send + Sync + 'static>,
    reservation: Arc<MemoryReservation>,
}

impl std::fmt::Debug for BoundedMemory {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("BoundedMemory")
            .finish_non_exhaustive()
    }
}

impl LinearMemory for BoundedMemory {
    fn ty(&self) -> MemoryType {
        self.inner.ty()
    }
    fn size(&self) -> Pages {
        self.inner.size()
    }
    fn style(&self) -> MemoryStyle {
        self.inner.style()
    }

    fn grow(&mut self, delta: Pages) -> Result<Pages, MemoryError> {
        let bytes = u64::from(delta.0) * WASM_PAGE_SIZE as u64;
        #[cfg(test)]
        TEST_MEMORY_GROWS.fetch_add(1, Ordering::AcqRel);
        self.reservation.reserve(bytes)?;
        match self.inner.grow(delta) {
            Ok(previous) => Ok(previous),
            Err(error) => {
                self.reservation.release(bytes);
                #[cfg(test)]
                TEST_MEMORY_GROW_ROLLBACK_BYTES.fetch_add(bytes, Ordering::AcqRel);
                Err(error)
            }
        }
    }

    fn grow_at_least(&mut self, minimum: u64) -> Result<(), MemoryError> {
        let current = u64::from(self.inner.size().0) * WASM_PAGE_SIZE as u64;
        let bytes = minimum
            .saturating_sub(current)
            .div_ceil(WASM_PAGE_SIZE as u64)
            * WASM_PAGE_SIZE as u64;
        #[cfg(test)]
        TEST_MEMORY_GROWS.fetch_add(1, Ordering::AcqRel);
        self.reservation.reserve(bytes)?;
        match self.inner.grow_at_least(minimum) {
            Ok(()) => Ok(()),
            Err(error) => {
                self.reservation.release(bytes);
                #[cfg(test)]
                TEST_MEMORY_GROW_ROLLBACK_BYTES.fetch_add(bytes, Ordering::AcqRel);
                Err(error)
            }
        }
    }

    fn reset(&mut self) -> Result<(), MemoryError> {
        self.inner.reset()
    }
    fn vmmemory(&self) -> NonNull<VMMemoryDefinition> {
        self.inner.vmmemory()
    }

    fn try_clone(&self) -> Result<Box<dyn LinearMemory + Send + Sync + 'static>, MemoryError> {
        Err(MemoryError::InvalidMemory {
            reason: "bounded guest memory cannot be cloned".into(),
        })
    }

    fn copy(&self) -> Result<Box<dyn LinearMemory + Send + Sync + 'static>, MemoryError> {
        Err(MemoryError::InvalidMemory {
            reason: "bounded guest memory cannot be copied".into(),
        })
    }
}

pub(super) struct BoundedTunables(pub(super) BaseTunables);

impl BoundedTunables {
    fn wrap(&self, memory: VMMemory) -> Result<VMMemory, MemoryError> {
        let reservation = CURRENT_MEMORY_RESERVATION
            .with(|current| current.borrow().clone())
            .ok_or_else(|| MemoryError::InvalidMemory {
                reason: "guest memory created without an admission reservation".into(),
            })?;
        Ok(VMMemory(Box::new(BoundedMemory {
            inner: memory.0,
            reservation,
        })))
    }
}

impl Tunables for BoundedTunables {
    fn memory_style(&self, memory: &MemoryType) -> MemoryStyle {
        self.0.memory_style(memory)
    }
    fn table_style(&self, table: &TableType) -> TableStyle {
        self.0.table_style(table)
    }

    fn create_host_memory(
        &self,
        ty: &MemoryType,
        style: &MemoryStyle,
    ) -> Result<VMMemory, MemoryError> {
        self.wrap(self.0.create_host_memory(ty, style)?)
    }

    unsafe fn create_vm_memory(
        &self,
        ty: &MemoryType,
        style: &MemoryStyle,
        definition: NonNull<VMMemoryDefinition>,
    ) -> Result<VMMemory, MemoryError> {
        self.wrap(unsafe { self.0.create_vm_memory(ty, style, definition) }?)
    }

    fn create_host_table(&self, ty: &TableType, style: &TableStyle) -> Result<VMTable, String> {
        self.0.create_host_table(ty, style)
    }

    unsafe fn create_vm_table(
        &self,
        ty: &TableType,
        style: &TableStyle,
        definition: NonNull<VMTableDefinition>,
    ) -> Result<VMTable, String> {
        unsafe { self.0.create_vm_table(ty, style, definition) }
    }

    fn vmconfig(&self) -> &VMConfig {
        self.0.vmconfig()
    }
}

pub(super) struct MemoryReservation {
    operation_memory: Arc<Mutex<u64>>,
    pub(super) bytes: AtomicU64,
}

impl MemoryReservation {
    pub(super) fn enter(budget: SharedBudget, initial_bytes: u64) -> Result<Arc<Self>, String> {
        let operation_memory = {
            let state = budget.lock().map_err(|_| "request budget poisoned")?;
            if Instant::now() > state.deadline {
                return Err("bounded nested evaluator exhausted".into());
            }
            state.operation_memory.clone()
        };
        let mut reserved = operation_memory
            .lock()
            .map_err(|_| "operation guest memory poisoned")?;
        let next = reserved
            .checked_add(initial_bytes)
            .ok_or("operation guest memory overflow")?;
        if next > OPERATION_GUEST_MEMORY_LIMIT {
            return Err("bounded nested evaluator exhausted".into());
        }
        *reserved = next;
        drop(reserved);
        Ok(Arc::new(Self {
            operation_memory,
            bytes: AtomicU64::new(initial_bytes),
        }))
    }

    pub(super) fn reserve(&self, bytes: u64) -> Result<(), MemoryError> {
        if bytes == 0 {
            return Ok(());
        }
        let attempted_delta =
            Pages(u32::try_from(bytes.div_ceil(WASM_PAGE_SIZE as u64)).unwrap_or(u32::MAX));
        let failure = || MemoryError::CouldNotGrow {
            current: Pages(
                u32::try_from(self.bytes.load(Ordering::Acquire) / WASM_PAGE_SIZE as u64)
                    .unwrap_or(u32::MAX),
            ),
            attempted_delta,
        };
        let mut operation_memory = self.operation_memory.lock().map_err(|_| failure())?;
        let reserved = operation_memory.checked_add(bytes).ok_or_else(failure)?;
        if reserved > OPERATION_GUEST_MEMORY_LIMIT {
            return Err(failure());
        }
        *operation_memory = reserved;
        self.bytes.fetch_add(bytes, Ordering::AcqRel);
        Ok(())
    }

    pub(super) fn release(&self, bytes: u64) {
        if bytes == 0 {
            return;
        }
        if let Ok(mut operation_memory) = self.operation_memory.lock() {
            *operation_memory = operation_memory.saturating_sub(bytes);
            self.bytes.fetch_sub(bytes, Ordering::AcqRel);
        }
    }
}

impl Drop for MemoryReservation {
    fn drop(&mut self) {
        let bytes = self.bytes.load(Ordering::Acquire);
        if let Ok(mut operation_memory) = self.operation_memory.lock() {
            *operation_memory = operation_memory.saturating_sub(bytes);
        }
    }
}
