use eyre::Result;
use foundry_block_explorers::verify::{CodeFormat, VerifyContract};

use crate::{
    zk_provider::{CompilerVerificationContext, ZkVerificationContext},
    VerifyArgs,
};

use super::EtherscanVerificationProvider;

/// The contract source provider for [`EtherscanVerificationProvider`]
/// in zksync mode
///
/// Returns source, contract_name and the source [`CodeFormat`]
pub trait EtherscanZksyncSourceProvider {
    fn zksync_source(
        &self,
        _args: &VerifyArgs,
        _context: &ZkVerificationContext,
    ) -> Result<(String, String, CodeFormat)> {
        eyre::bail!("source provider doesn't support etherscan in zksync mode")
    }
}

impl EtherscanVerificationProvider {
    /// Populates the `verify_args` request with context-specific
    /// information
    pub(super) fn zk_verify_args(
        &self,
        context: &CompilerVerificationContext,
        verify_args: &mut VerifyContract,
    ) {
        if let CompilerVerificationContext::ZkSolc(context) = context {
            let compiler_mode =
                if context.compiler_version.is_zksync_solc { "zksync" } else { "solc" }.to_string();

            let extras = [
                ("compilermode".to_string(), compiler_mode),
                ("zksolcVersion".to_string(), format!("v{}", context.compiler_version.zksolc)),
            ];
            verify_args.other.extend(extras);
        }
    }
}
