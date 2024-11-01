use alloy_primitives::{Address, Bytes, Log, U256};
use foundry_evm_core::{backend::DatabaseExt, InspectorExt};
use foundry_evm_traces::{
    CallTraceArena, GethTraceBuilder, ParityTraceBuilder, TracingInspector, TracingInspectorConfig,
};
use foundry_zksync_core::{
    convert::{ConvertH160, ConvertU256},
    Call,
};
use revm::{
    interpreter::{
        CallInputs, CallOutcome, CreateInputs, CreateOutcome, EOFCreateInputs, Gas,
        InstructionResult, Interpreter, InterpreterResult,
    },
    Database, EvmContext, Inspector,
};

/// A Wrapper around [TracingInspector] to allow adding zkEVM traces.
#[derive(Clone, Debug, Default)]
pub struct TraceCollector {
    inner: TracingInspector,
}

impl TraceCollector {
    /// Returns a new instance for the given config
    pub fn new(config: TracingInspectorConfig) -> Self {
        Self { inner: TracingInspector::new(config) }
    }

    /// Returns the inner [`TracingInspector`]
    #[inline]
    pub fn inner(&mut self) -> &mut TracingInspector {
        &mut self.inner
    }

    /// Resets the inspector to its initial state of [Self::new].
    /// This makes the inspector ready to be used again.
    ///
    /// Note that this method has no effect on the allocated capacity of the vector.
    #[inline]
    pub fn fuse(&mut self) {
        self.inner.fuse()
    }

    /// Resets the inspector to it's initial state of [Self::new].
    #[inline]
    pub fn fused(self) -> Self {
        Self { inner: self.inner.fused() }
    }

    /// Returns the config of the inspector.
    pub const fn config(&self) -> &TracingInspectorConfig {
        self.inner.config()
    }

    /// Returns a mutable reference to the config of the inspector.
    pub fn config_mut(&mut self) -> &mut TracingInspectorConfig {
        self.inner.config_mut()
    }

    /// Updates the config of the inspector.
    pub fn update_config(
        &mut self,
        f: impl FnOnce(TracingInspectorConfig) -> TracingInspectorConfig,
    ) {
        self.inner.update_config(f);
    }

    /// Gets a reference to the recorded call traces.
    pub const fn traces(&self) -> &CallTraceArena {
        self.inner.traces()
    }

    /// Gets a mutable reference to the recorded call traces.
    pub fn traces_mut(&mut self) -> &mut CallTraceArena {
        self.inner.traces_mut()
    }

    /// Consumes the inspector and returns the recorded call traces.
    pub fn into_traces(self) -> CallTraceArena {
        self.inner.into_traces()
    }

    /// Manually the gas used of the root trace.
    ///
    /// This is useful if the root trace's gasUsed should mirror the actual gas used by the
    /// transaction.
    ///
    /// This allows setting it manually by consuming the execution result's gas for example.
    #[inline]
    pub fn set_transaction_gas_used(&mut self, gas_used: u64) {
        self.inner.set_transaction_gas_used(gas_used)
    }

    /// Convenience function for [ParityTraceBuilder::set_transaction_gas_used] that consumes the
    /// type.
    #[inline]
    pub fn with_transaction_gas_used(self, gas_used: u64) -> Self {
        Self { inner: self.inner.with_transaction_gas_used(gas_used) }
    }

    /// Consumes the Inspector and returns a [ParityTraceBuilder].
    #[inline]
    pub fn into_parity_builder(self) -> ParityTraceBuilder {
        self.inner.into_parity_builder()
    }

    /// Consumes the Inspector and returns a [GethTraceBuilder].
    #[inline]
    pub fn into_geth_builder(self) -> GethTraceBuilder<'static> {
        self.inner.into_geth_builder()
    }
}

impl<DB> Inspector<DB> for TraceCollector
where
    DB: Database,
{
    #[inline]
    fn step(&mut self, interp: &mut Interpreter, context: &mut EvmContext<DB>) {
        self.inner.step(interp, context)
    }

    #[inline]
    fn step_end(&mut self, interp: &mut Interpreter, context: &mut EvmContext<DB>) {
        self.inner.step_end(interp, context)
    }

    fn log(&mut self, interp: &mut Interpreter, context: &mut EvmContext<DB>, log: &Log) {
        self.inner.log(interp, context, log)
    }

    fn call(
        &mut self,
        context: &mut EvmContext<DB>,
        inputs: &mut CallInputs,
    ) -> Option<CallOutcome> {
        self.inner.call(context, inputs)
    }

    fn call_end(
        &mut self,
        context: &mut EvmContext<DB>,
        inputs: &CallInputs,
        outcome: CallOutcome,
    ) -> CallOutcome {
        self.inner.call_end(context, inputs, outcome)
    }

    fn create(
        &mut self,
        context: &mut EvmContext<DB>,
        inputs: &mut CreateInputs,
    ) -> Option<CreateOutcome> {
        self.inner.create(context, inputs)
    }

    fn create_end(
        &mut self,
        context: &mut EvmContext<DB>,
        inputs: &CreateInputs,
        outcome: CreateOutcome,
    ) -> CreateOutcome {
        self.inner.create_end(context, inputs, outcome)
    }

    fn eofcreate(
        &mut self,
        context: &mut EvmContext<DB>,
        inputs: &mut EOFCreateInputs,
    ) -> Option<CreateOutcome> {
        self.inner.eofcreate(context, inputs)
    }

    fn eofcreate_end(
        &mut self,
        context: &mut EvmContext<DB>,
        inputs: &EOFCreateInputs,
        outcome: CreateOutcome,
    ) -> CreateOutcome {
        self.inner.eofcreate_end(context, inputs, outcome)
    }

    fn selfdestruct(&mut self, contract: Address, target: Address, value: U256) {
        <TracingInspector as Inspector<DB>>::selfdestruct(&mut self.inner, contract, target, value)
    }
}

impl InspectorExt for TraceCollector {
    fn trace_zksync(
        &mut self,
        context: &mut EvmContext<&mut dyn DatabaseExt>,
        call_traces: Vec<Call>,
    ) {
        fn trace_call_recursive(
            tracer: &mut TracingInspector,
            context: &mut EvmContext<&mut dyn DatabaseExt>,
            call: Call,
            suppressed_top_call: bool,
        ) -> u64 {
            let inputs = &mut CallInputs {
                input: call.input.into(),
                gas_limit: call.gas,
                scheme: revm::interpreter::CallScheme::Call,
                caller: call.from.to_address(),
                value: revm::interpreter::CallValue::Transfer(call.value.to_ru256()),
                target_address: call.to.to_address(),
                bytecode_address: call.to.to_address(),
                is_eof: false,
                is_static: false,
                return_memory_offset: Default::default(),
            };
            let is_first_non_system_call = if !suppressed_top_call {
                !foundry_zksync_core::is_system_address(inputs.caller) &&
                    !foundry_zksync_core::is_system_address(inputs.target_address)
            } else {
                false
            };

            // We ignore traces from system addresses, the default account abstraction calls on
            // caller address, and the original call (identified when neither `to` or
            // `from` are system addresses) since it is already included in EVM trace.
            let record_trace = !is_first_non_system_call &&
                !foundry_zksync_core::is_system_address(inputs.target_address) &&
                inputs.target_address != context.env.tx.caller;

            let (new_depth, overflow) = context.journaled_state.depth.overflowing_add(1);
            if !overflow && record_trace {
                context.journaled_state.depth = new_depth;
            }

            let mut outcome = if let Some(reason) = &call.revert_reason {
                CallOutcome {
                    result: InterpreterResult {
                        result: InstructionResult::Revert,
                        output: reason.as_bytes().to_owned().into(),
                        gas: Gas::new_spent(call.gas_used),
                    },
                    memory_offset: Default::default(),
                }
            } else {
                CallOutcome {
                    result: InterpreterResult {
                        result: InstructionResult::Return,
                        output: call.output.clone().into(),
                        gas: Gas::new_spent(call.gas_used),
                    },
                    memory_offset: Default::default(),
                }
            };

            let is_create = matches!(call.r#type, foundry_zksync_core::CallType::Create);
            let mut create_inputs = if is_create {
                Some(CreateInputs {
                    caller: inputs.caller,
                    scheme: revm::primitives::CreateScheme::Create,
                    value: inputs.value.get(),
                    init_code: inputs.input.clone(),
                    gas_limit: inputs.gas_limit,
                })
            } else {
                None
            };

            // start span
            if record_trace {
                if let Some(inputs) = &mut create_inputs {
                    tracer.create(context, inputs);
                } else {
                    tracer.call(context, inputs);
                }
            }

            // recurse into inner calls
            // record extra gas from ignored traces, to add it at end
            let mut extra_gas = if record_trace { 0u64 } else { call.gas_used };
            for inner_call in call.calls {
                let inner_extra_gas = trace_call_recursive(
                    tracer,
                    context,
                    inner_call,
                    suppressed_top_call || is_first_non_system_call,
                );
                extra_gas = extra_gas.saturating_add(inner_extra_gas);
            }

            // finish span
            if record_trace {
                if let Some(inputs) = &mut create_inputs {
                    let outcome = if let Some(reason) = call.revert_reason {
                        CreateOutcome {
                            result: InterpreterResult {
                                result: InstructionResult::Revert,
                                output: reason.as_bytes().to_owned().into(),
                                gas: Gas::new_spent(call.gas_used + extra_gas),
                            },
                            address: None,
                        }
                    } else {
                        CreateOutcome {
                            result: InterpreterResult {
                                result: InstructionResult::Return,
                                output: Bytes::from(call.output),
                                gas: Gas::new_spent(call.gas_used + extra_gas),
                            },
                            address: Some(call.to.to_address()),
                        }
                    };

                    tracer.create_end(context, inputs, outcome);
                } else {
                    if extra_gas != 0 {
                        outcome.result.gas = Gas::new_spent(outcome.result.gas.spent() + extra_gas);
                    }
                    tracer.call_end(context, inputs, outcome);
                }
            }

            if !overflow && record_trace {
                context.journaled_state.depth = context.journaled_state.depth.saturating_sub(1);
            }

            extra_gas
        }

        for call in call_traces {
            trace_call_recursive(&mut self.inner, context, call, false);
        }
    }
}
