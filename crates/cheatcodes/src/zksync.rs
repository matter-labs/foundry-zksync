use crate::{Cheatcode, CheatsCtxt, DatabaseExt, Result, Vm::*};

impl Cheatcode for switchToREVMCall {
    fn apply_full<DB: DatabaseExt>(&self, _ccx: &mut CheatsCtxt<DB>) -> Result {
        todo!()
    }
}

impl Cheatcode for switchToZkSyncCall {
    fn apply_full<DB: DatabaseExt>(&self, _ccx: &mut CheatsCtxt<DB>) -> Result {
        todo!()
    }
}
