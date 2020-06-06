use std::str;
use crate::{imports, };
use near_runtime_fees::RuntimeFeesConfig;
use near_vm_errors::{VMError, FunctionCallError, MethodResolveError};
use near_vm_logic::types::PromiseResult;
use near_vm_logic::{External, VMConfig, VMContext, VMLogic, VMOutcome, MemoryLike};
use wasmtime::{Module, Store, MemoryType, Limits, Memory, Linker, Engine};
use std::ffi::c_void;
use near_vm_errors::FunctionCallError::LinkError;
use crate::errors::IntoVMError;

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

pub struct NearEngine {
    pub engine: Engine,
    // TODO: make it properly typed as VMLogic?
    pub ctx: *mut c_void,
}

impl Default for NearEngine {
    fn default() -> Self {
        Self {
            engine: Engine::default(),
            ctx: 0 as *mut c_void,
        }
    }
}

impl IntoVMError for anyhow::Error {
    fn into_vm_error(self) -> VMError {
        // TODO: incorrect
        VMError::FunctionCallError(LinkError { msg: "unknown any".to_string() })
    }
}

impl IntoVMError for wasmtime::Trap {
    fn into_vm_error(self) -> VMError {
        // TODO: incorrect
        VMError::FunctionCallError(LinkError { msg: "unknown trap".to_string() })
    }
}

pub fn run_wasmtime<'a>(
    _code_hash: Vec<u8>,
    code: &[u8],
    method_name: &[u8],
    ext: &mut dyn External,
    context: VMContext,
    wasm_config: &'a VMConfig,
    fees_config: &'a RuntimeFeesConfig,
    promise_results: &'a [PromiseResult],
) -> (Option<VMOutcome>, Option<VMError>) {
    let mut near_engine = NearEngine::default();
    let engine = near_engine.engine;
    let store = Store::new(&engine);
    let mut memory =
        WasmtimeMemory::new(
            &store,
            wasm_config.limit_config.initial_memory_pages,
            wasm_config.limit_config.max_memory_pages).unwrap();
    let module = Module::new(&engine, code).unwrap();
    // Note that we don't clone the actual backing memory, just increase the RC.
    let memory_copy = memory.clone();
    let mut linker = Linker::new(&store);
    let mut logic =
        VMLogic::new(ext, context, wasm_config, fees_config, promise_results, &mut memory);
    near_engine.ctx = &mut logic as *mut _ as *mut c_void ;
    imports::link_wasmtime(&mut linker, memory_copy);
    match linker.instantiate(&module) {
        Ok(instance) =>
            match instance.get_func(str::from_utf8(method_name).unwrap()) {
                Some(func) => {
                    match func.get0::<()>() {
                        Ok(run) => {
                            match run() {
                                Ok(_) => (Some(logic.outcome()), None),
                                Err(err) => (Some(logic.outcome()), Some(err.into_vm_error())),
                            }
                        }
                        Err(err) => (Some(logic.outcome()), Some(err.into_vm_error())),
                    }
                },
                None => (
                    None,
                    Some(VMError::FunctionCallError(FunctionCallError::MethodResolveError(
                        MethodResolveError::MethodUTF8Error,
                    ))),
                )
            },
        Err(err) => (Some(logic.outcome()), Some(err.into_vm_error())),
    }
}