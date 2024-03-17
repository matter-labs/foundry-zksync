//! Several ABI-related utilities for executors.

use alloy_primitives::{address, Address};
// pub use foundry_cheatcodes_spec::Vm;

mod interface;
pub use interface::{format_units_int, format_units_uint, Console};

mod hardhat_console;
pub use hardhat_console::{
    hh_console_selector, patch_hh_console_selector, HardhatConsole,
    HARDHAT_CONSOLE_SELECTOR_PATCHES,
};

/// The Hardhat console address.
///
/// See: <https://github.com/nomiclabs/hardhat/blob/master/packages/hardhat-core/console.sol>
pub const HARDHAT_CONSOLE_ADDRESS: Address = address!("000000000000000000636F6e736F6c652e6c6f67");
