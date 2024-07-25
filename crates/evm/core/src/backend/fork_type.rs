use std::collections::HashMap;

use alloy_provider::Provider;

/// Defines a fork of the type EVM or ZK.
#[derive(Debug, Clone)]
pub enum ForkType {
    Evm,
    Zk,
}

impl ForkType {
    /// Returns true if type is [ForkType::Zk]
    pub fn is_zk(&self) -> bool {
        matches!(self, Self::Zk)
    }

    /// Returns true if type is [ForkType::Evm]
    pub fn is_evm(&self) -> bool {
        matches!(self, Self::Evm)
    }
}

/// A cached implementation for retrieving the [ForkType] of a given url.
#[derive(Default, Debug, Clone)]
pub struct CachedForkType(HashMap<String, ForkType>);

impl CachedForkType {
    /// Retrieve the [ForkType] of a url.
    /// We attempt to query the rpc provider for "zks_L1ChainId" method. If it returns successfully,
    /// then the chain is [ForkType::Zk], else it's [ForkType::Evm].
    /// The result is then cached
    pub fn get(&mut self, fork_url: &str) -> ForkType {
        if let Some(fork_url_type) = self.0.get(fork_url) {
            return fork_url_type.clone()
        }

        let is_zk_url = foundry_common::provider::try_get_http_provider(fork_url)
            .map(|provider| {
                let is_zk_url = tokio::runtime::Builder::new_multi_thread()
                    .enable_all()
                    .build()
                    .unwrap()
                    .block_on(provider.raw_request("zks_L1ChainId".into(), ()))
                    .map(|_: String| true)
                    .unwrap_or_default();

                is_zk_url
            })
            .unwrap_or_default();

        let fork_type = if is_zk_url { ForkType::Zk } else { ForkType::Evm };
        self.0.insert(fork_url.to_string(), fork_type.clone());

        fork_type
    }
}
