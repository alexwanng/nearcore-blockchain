use std::str;
use crate::errors::IntoVMError;
use crate::{cache, imports, };
use near_runtime_fees::RuntimeFeesConfig;
use near_vm_errors::{FunctionCallError, MethodResolveError, VMError};
use near_vm_logic::types::PromiseResult;
use near_vm_logic::{External, VMConfig, VMContext, VMLogic, VMOutcome, MemoryLike};
use wasmtime::{Module, Store, MemoryType, Limits, Memory, Instance, Extern, Linker};

pub struct WasmtimeMemory(Memory);

impl WasmtimeMemory {
    pub fn new(
        store: &Store,
        initial_memory_bytes: u32,
        max_memory_bytes: u32
    ) -> Result<Self, VMError> {
        Ok(WasmtimeMemory(
            Memory::new(store,
                MemoryType::new(Limits::new(
                    initial_memory_bytes,
                    Some(max_memory_bytes)
                    )
                ),
            ),
        ))
    }

    pub fn clone(&self) -> Memory {
        self.0.clone()
    }
}

impl MemoryLike for WasmtimeMemory {
    fn fits_memory(&self, offset: u64, len: u64) -> bool {
        match offset.checked_add(len) {
            None => false,
            Some(end) => self.0.size() as u64 >= end,
        }
    }

    fn read_memory(&self, offset: u64, buffer: &mut [u8]) {
        let offset = offset as usize;
        unsafe {
            for i in 0..buffer.len() {
                buffer[i] = self.0.data_unchecked()[i + offset];
            }
        }
    }

    fn read_memory_u8(&self, offset: u64) -> u8 {
        unsafe {
            self.0.data_unchecked()[offset as usize]
        }
    }

    fn write_memory(&mut self, offset: u64, buffer: &[u8]) {
        unsafe {
            let offset = offset as usize;
            for i in 0..buffer.len() {
                self.0.data_unchecked_mut()[i + offset] = buffer[i];
            }
        }
    }
}

pub fn run_wasmtime<'a>(
    code_hash: Vec<u8>,
    code: &[u8],
    method_name: &[u8],
    ext: &mut dyn External,
    context: VMContext,
    wasm_config: &'a VMConfig,
    fees_config: &'a RuntimeFeesConfig,
    promise_results: &'a [PromiseResult],
) -> (Option<VMOutcome>, Option<VMError>) {
    let store = Store::default();
    let mut memory =
        WasmtimeMemory::new(
            &store,
            wasm_config.limit_config.initial_memory_pages,
            wasm_config.limit_config.max_memory_pages).unwrap();
    let module = Module::new(&store, code).unwrap();
    // Note that we don't clone the actual backing memory, just increase the RC.
    let mut memory_copy = memory.clone();
    let mut linker = Linker::new(&store);
    let mut logic =
        VMLogic::new(ext, context, wasm_config, fees_config, promise_results, &mut memory);
    imports::link_wasmtime(
        &store, &linker, memory_copy, &mut logic);
    match linker.instantiate(&module) {
        Ok(instance) =>
            match instance.get_func(str::from_utf8(method_name).unwrap()) {
                Some(func) => {
                    let run = func.get0::<()>();
                    run;
                    (Some(logic.outcome()), None)
                },
                None => panic!("No function"),
            },
        Err(err) => panic!("Error"),
    }
}