use crate::{inspector::InnerEcx, Cheatcode, Cheatcodes, CheatsCtxt, Result, Vm::*};
use alloy_primitives::{Address, Bytes, U256};
use foundry_cheatcodes_common::mock::{MockCallDataContext, MockCallReturnData};
use revm::{interpreter::InstructionResult, primitives::Bytecode};
use std::collections::VecDeque;

impl Cheatcode for clearMockedCallsCall {
    fn apply(&self, state: &mut Cheatcodes) -> Result {
        let Self {} = self;
        state.mocked_calls = Default::default();
        Ok(Default::default())
    }
}

impl Cheatcode for mockCall_0Call {
    fn apply_stateful(&self, ccx: &mut CheatsCtxt) -> Result {
        let Self { callee, data, returnData } = self;
        let _ = make_acc_non_empty(callee, ccx.ecx)?;

        if ccx.state.use_zk_vm {
            foundry_zksync_core::cheatcodes::set_mocked_account(*callee, ccx.ecx, ccx.caller);
        }

        mock_call(ccx.state, callee, data, None, returnData, InstructionResult::Return);
        Ok(Default::default())
    }
}

impl Cheatcode for mockCall_1Call {
    fn apply_stateful(&self, ccx: &mut CheatsCtxt) -> Result {
        let Self { callee, msgValue, data, returnData } = self;
        ccx.ecx.load_account(*callee)?;
        mock_call(ccx.state, callee, data, Some(msgValue), returnData, InstructionResult::Return);
        Ok(Default::default())
    }
}

impl Cheatcode for mockCalls_0Call {
    fn apply_stateful(&self, ccx: &mut CheatsCtxt) -> Result {
        let Self { callee, data, returnData } = self;
        let _ = make_acc_non_empty(callee, ccx.ecx)?;

        mock_calls(ccx.state, callee, data, None, returnData, InstructionResult::Return);
        Ok(Default::default())
    }
}

impl Cheatcode for mockCalls_1Call {
    fn apply_stateful(&self, ccx: &mut CheatsCtxt) -> Result {
        let Self { callee, msgValue, data, returnData } = self;
        ccx.ecx.load_account(*callee)?;
        mock_calls(ccx.state, callee, data, Some(msgValue), returnData, InstructionResult::Return);
        Ok(Default::default())
    }
}

impl Cheatcode for mockCallRevert_0Call {
    fn apply_stateful(&self, ccx: &mut CheatsCtxt) -> Result {
        let Self { callee, data, revertData } = self;
        let _ = make_acc_non_empty(callee, ccx.ecx)?;

        if ccx.state.use_zk_vm {
            foundry_zksync_core::cheatcodes::set_mocked_account(*callee, ccx.ecx, ccx.caller);
        }

        mock_call(ccx.state, callee, data, None, revertData, InstructionResult::Revert);
        Ok(Default::default())
    }
}

impl Cheatcode for mockCallRevert_1Call {
    fn apply_stateful(&self, ccx: &mut CheatsCtxt) -> Result {
        let Self { callee, msgValue, data, revertData } = self;
        let _ = make_acc_non_empty(callee, ccx.ecx)?;

        mock_call(ccx.state, callee, data, Some(msgValue), revertData, InstructionResult::Revert);
        Ok(Default::default())
    }
}

impl Cheatcode for mockFunctionCall {
    fn apply(&self, state: &mut Cheatcodes) -> Result {
        let Self { callee, target, data } = self;
        state.mocked_functions.entry(*callee).or_default().insert(data.clone(), *target);

        Ok(Default::default())
    }
}

fn mock_call(
    state: &mut Cheatcodes,
    callee: &Address,
    cdata: &Bytes,
    value: Option<&U256>,
    rdata: &Bytes,
    ret_type: InstructionResult,
) {
    mock_calls(state, callee, cdata, value, std::slice::from_ref(rdata), ret_type)
}

fn mock_calls(
    state: &mut Cheatcodes,
    callee: &Address,
    cdata: &Bytes,
    value: Option<&U256>,
    rdata_vec: &[Bytes],
    ret_type: InstructionResult,
) {
    state.mocked_calls.entry(*callee).or_default().insert(
        MockCallDataContext { calldata: Bytes::copy_from_slice(cdata), value: value.copied() },
        rdata_vec
            .iter()
            .map(|rdata| MockCallReturnData { ret_type, data: rdata.clone() })
            .collect::<VecDeque<_>>(),
    );
}

// Etches a single byte onto the account if it is empty to circumvent the `extcodesize`
// check Solidity might perform.
fn make_acc_non_empty(callee: &Address, ecx: InnerEcx) -> Result {
    let acc = ecx.load_account(*callee)?;

    let empty_bytecode = acc.info.code.as_ref().map_or(true, Bytecode::is_empty);
    if empty_bytecode {
        let code = Bytecode::new_raw(Bytes::from_static(&foundry_zksync_core::EMPTY_CODE));
        ecx.journaled_state.set_code(*callee, code);
    }

    Ok(Default::default())
}
