//! Decoder module copied from anvil-zksync as it's not exported publicly.

use std::sync::OnceLock;

use alloy_json_abi::Event;
use anvil_zksync_console::{ds, hh};
use anvil_zksync_traces::decode::{CallTraceDecoder, CallTraceDecoderBuilderBase};

pub struct CallTraceDecoderBuilder;

impl CallTraceDecoderBuilder {
    /// Create a new builder starting from a CallTraceDecoder capable of
    /// decoding calls of DSTest-style logs
    #[inline]
    pub fn default() -> CallTraceDecoderBuilderBase {
        static INIT: OnceLock<CallTraceDecoder> = OnceLock::new();
        CallTraceDecoderBuilderBase::new(
            INIT.get_or_init(|| {
                CallTraceDecoder::new(
                    hh::abi::functions()
                        .into_values()
                        .flatten()
                        .map(|func| (func.selector(), vec![func]))
                        .collect(),
                    ds::abi::events()
                        .into_values()
                        .flatten()
                        .map(|event| ((event.selector(), indexed_inputs(&event)), vec![event]))
                        .collect(),
                )
            })
            .clone(),
        )
    }
}

fn indexed_inputs(event: &Event) -> usize {
    event.inputs.iter().filter(|param| param.indexed).count()
}
