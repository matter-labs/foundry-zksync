pub mod executor;
pub use executor::*;

mod inspector;

#[allow(dead_code)]
enum MultiVMState {
    Stop,
    Inactive { executor: executor::Executor },
    Active { executor: executor::Executor },
}

#[allow(dead_code)]
pub struct MultiMV {
    state: MultiVMState,
}

impl Default for MultiMV {
    fn default() -> Self {
        Self { state: MultiVMState::Stop }
    }
}

impl MultiMV {}
