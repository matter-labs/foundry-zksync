pub mod executor;
pub use executor::*;

enum MultiVMState {
    Stop,
    Inactive { executor: executor::Executor },
    Active { executor: executor::Executor },
}

pub struct MultiMV {
    state: MultiVMState,
}

impl Default for MultiMV {
    fn default() -> Self {
        Self { state: MultiVMState::Stop }
    }
}

impl MultiMV {}
