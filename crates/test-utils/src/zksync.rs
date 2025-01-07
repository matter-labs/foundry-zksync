//! Contains in-memory implementation of era-test-node.

use alloy_primitives::hex;
use alloy_zksync::node_bindings::{AnvilZKsync, AnvilZKsyncInstance};

/// In-memory era-test-node that is stopped when dropped.
pub struct ZkSyncNode {
    instance: AnvilZKsyncInstance,
}

impl ZkSyncNode {
    /// Returns the server url.
    #[inline]
    pub fn url(&self) -> String {
        self.instance.endpoint()
    }

    /// Start era-test-node in memory, binding a random available port
    ///
    /// The server is automatically stopped when the instance is dropped.
    pub fn start() -> Self {
        // TODO: For now we choose random port and hope it's not busy, but it can be flaky.
        // Replace with `0` once https://github.com/matter-labs/anvil-zksync/pull/513 is merged

        let mut attempt = 0;
        let instance = loop {
            // Generate random port from 5000 to 9000
            let random_port = rand::random::<u16>() % 4000 + 5000;
            if let Ok(instance) = AnvilZKsync::new().port(random_port).try_spawn() {
                break instance;
            }

            attempt += 1;
            if attempt == 10 {
                panic!("Failed to start era-test-node after 10 attempts");
            }
        };
        Self { instance }
    }

    pub fn rich_wallets(&self) -> Vec<(String, String)> {
        self.instance
            .addresses()
            .iter()
            .zip(self.instance.keys())
            .map(|(address, key)| {
                (address.to_string(), format!("0x{}", hex::encode(key.to_bytes().as_slice())))
            })
            .collect()
    }
}
