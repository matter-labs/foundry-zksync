use crate::{Cheatcode, Cheatcodes, CheatsCtxt, DatabaseExt, Result, Vm::*};
use alloy_primitives::{Address, Bytes, U256};
use foundry_cheatcodes_common::mock::{MockCallDataContext, MockCallReturnData};
use revm::{interpreter::InstructionResult, primitives::Bytecode};

impl Cheatcode for clearMockedCallsCall {
    fn apply(&self, state: &mut Cheatcodes) -> Result {
        let Self {} = self;
        state.mocked_calls = Default::default();
        Ok(Default::default())
    }
}

impl Cheatcode for mockCall_0Call {
    fn apply_full<DB: DatabaseExt>(&self, ccx: &mut CheatsCtxt<DB>) -> Result {
        let Self { callee, data, returnData } = self;
        // TODO: use ecx.load_account
        let (acc, _) = ccx.ecx.journaled_state.load_account(*callee, &mut ccx.ecx.db)?;

        // Etches a single byte onto the account if it is empty to circumvent the `extcodesize`
        // check Solidity might perform.
        let empty_bytecode = acc.info.code.as_ref().map_or(true, Bytecode::is_empty);
        if empty_bytecode {
            let code = revm::interpreter::analysis::to_analysed(Bytecode::new_raw(
                Bytes::copy_from_slice(&foundry_zksync_core::EMPTY_CODE),
            ));
            ccx.ecx.journaled_state.set_code(*callee, code.clone());
        }

        if ccx.state.use_zk_vm {
            foundry_zksync_core::cheatcodes::set_mocked_account(*callee, ccx.ecx, ccx.caller);
        }

        mock_call(ccx.state, callee, data, None, returnData, InstructionResult::Return);
        Ok(Default::default())
    }
}

impl Cheatcode for mockCall_1Call {
    fn apply_full<DB: DatabaseExt>(&self, ccx: &mut CheatsCtxt<DB>) -> Result {
        let Self { callee, msgValue, data, returnData } = self;
        ccx.ecx.load_account(*callee)?;
        mock_call(ccx.state, callee, data, Some(msgValue), returnData, InstructionResult::Return);
        Ok(Default::default())
    }
}

impl Cheatcode for mockCallRevert_0Call {
    fn apply(&self, state: &mut Cheatcodes) -> Result {
        let Self { callee, data, revertData } = self;
        mock_call(state, callee, data, None, revertData, InstructionResult::Revert);
        Ok(Default::default())
    }
}

impl Cheatcode for mockCallRevert_1Call {
    fn apply(&self, state: &mut Cheatcodes) -> Result {
        let Self { callee, msgValue, data, revertData } = self;
        mock_call(state, callee, data, Some(msgValue), revertData, InstructionResult::Revert);
        Ok(Default::default())
    }
}

#[allow(clippy::ptr_arg)] // Not public API, doesn't matter
fn mock_call(
    state: &mut Cheatcodes,
    callee: &Address,
    cdata: &Bytes,
    value: Option<&U256>,
    rdata: &Bytes,
    ret_type: InstructionResult,
) {
    state.mocked_calls.entry(*callee).or_default().insert(
        MockCallDataContext { calldata: Bytes::copy_from_slice(cdata), value: value.copied() },
        MockCallReturnData { ret_type, data: Bytes::copy_from_slice(rdata) },
    );
}
