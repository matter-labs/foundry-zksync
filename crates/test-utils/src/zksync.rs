//! Contains in-memory implementation of anvil-zksync.
use std::{future::Future, net::SocketAddr, pin::Pin, str::FromStr, sync::Arc};

use anvil_zksync_api_server::NodeServerBuilder;
use anvil_zksync_config::{
    constants::{
        DEFAULT_ESTIMATE_GAS_PRICE_SCALE_FACTOR, DEFAULT_ESTIMATE_GAS_SCALE_FACTOR,
        DEFAULT_FAIR_PUBDATA_PRICE, DEFAULT_L1_GAS_PRICE, DEFAULT_L2_GAS_PRICE,
    },
    types::{CacheConfig, SystemContractsOptions},
    TestNodeConfig,
};
use anvil_zksync_core::{
    filters::EthFilters,
    node::{
        fork::{ForkClient, ForkConfig},
        BlockSealer, BlockSealerMode, ImpersonationManager, InMemoryNode, InMemoryNodeInner,
        NodeExecutor, StorageKeyLayout, TestNodeFeeInputProvider, TxPool,
    },
    system_contracts::SystemContracts,
};
use anvil_zksync_l1_sidecar::L1Sidecar;
use tokio::sync::RwLock;
use tower_http::cors::AllowOrigin;
use zksync_types::{L2BlockNumber, DEFAULT_ERA_CHAIN_ID, H160, U256};

/// List of legacy wallets (address, private key) that we seed with tokens at start.
const LEGACY_RICH_WALLETS: [(&str, &str); 10] = [
    (
        "0x36615Cf349d7F6344891B1e7CA7C72883F5dc049",
        "0x7726827caac94a7f9e1b160f7ea819f172f7b6f9d2a97f992c38edeab82d4110",
    ),
    (
        "0xa61464658AfeAf65CccaaFD3a512b69A83B77618",
        "0xac1e735be8536c6534bb4f17f06f6afc73b2b5ba84ac2cfb12f7461b20c0bbe3",
    ),
    (
        "0x0D43eB5B8a47bA8900d84AA36656c92024e9772e",
        "0xd293c684d884d56f8d6abd64fc76757d3664904e309a0645baf8522ab6366d9e",
    ),
    (
        "0xA13c10C0D5bd6f79041B9835c63f91de35A15883",
        "0x850683b40d4a740aa6e745f889a6fdc8327be76e122f5aba645a5b02d0248db8",
    ),
    (
        "0x8002cD98Cfb563492A6fB3E7C8243b7B9Ad4cc92",
        "0xf12e28c0eb1ef4ff90478f6805b68d63737b7f33abfa091601140805da450d93",
    ),
    (
        "0x4F9133D1d3F50011A6859807C837bdCB31Aaab13",
        "0xe667e57a9b8aaa6709e51ff7d093f1c5b73b63f9987e4ab4aa9a5c699e024ee8",
    ),
    (
        "0xbd29A1B981925B94eEc5c4F1125AF02a2Ec4d1cA",
        "0x28a574ab2de8a00364d5dd4b07c4f2f574ef7fcc2a86a197f65abaec836d1959",
    ),
    (
        "0xedB6F5B4aab3dD95C7806Af42881FF12BE7e9daa",
        "0x74d8b3a188f7260f67698eb44da07397a298df5427df681ef68c45b34b61f998",
    ),
    (
        "0xe706e60ab5Dc512C36A4646D719b889F398cbBcB",
        "0xbe79721778b48bcc679b78edac0ce48306a8578186ffcb9f2ee455ae6efeace1",
    ),
    (
        "0xE90E12261CCb0F3F7976Ae611A29e84a6A85f424",
        "0x3eb15da85647edd9a1159a4a13b9e7c56877c4eb33f614546d4db06a51868b1c",
    ),
];

/// List of wallets (address, private key, mnemonic) that we seed with tokens at start.
const RICH_WALLETS: [(&str, &str, &str); 10] = [
    (
        "0xBC989fDe9e54cAd2aB4392Af6dF60f04873A033A",
        "0x3d3cbc973389cb26f657686445bcc75662b415b656078503592ac8c1abb8810e",
        "mass wild lava ripple clog cabbage witness shell unable tribe rubber enter",
    ),
    (
        "0x55bE1B079b53962746B2e86d12f158a41DF294A6",
        "0x509ca2e9e6acf0ba086477910950125e698d4ea70fa6f63e000c5a22bda9361c",
        "crumble clutch mammal lecture lazy broken nominee visit gentle gather gym erupt",
    ),
    (
        "0xCE9e6063674DC585F6F3c7eaBe82B9936143Ba6C",
        "0x71781d3a358e7a65150e894264ccc594993fbc0ea12d69508a340bc1d4f5bfbc",
        "illegal okay stereo tattoo between alien road nuclear blind wolf champion regular",
    ),
    (
        "0xd986b0cB0D1Ad4CCCF0C4947554003fC0Be548E9",
        "0x379d31d4a7031ead87397f332aab69ef5cd843ba3898249ca1046633c0c7eefe",
        "point donor practice wear alien abandon frozen glow they practice raven shiver",
    ),
    (
        "0x87d6ab9fE5Adef46228fB490810f0F5CB16D6d04",
        "0x105de4e75fe465d075e1daae5647a02e3aad54b8d23cf1f70ba382b9f9bee839",
        "giraffe organ club limb install nest journey client chunk settle slush copy",
    ),
    (
        "0x78cAD996530109838eb016619f5931a03250489A",
        "0x7becc4a46e0c3b512d380ca73a4c868f790d1055a7698f38fb3ca2b2ac97efbb",
        "awful organ version habit giraffe amused wire table begin gym pistol clean",
    ),
    (
        "0xc981b213603171963F81C687B9fC880d33CaeD16",
        "0xe0415469c10f3b1142ce0262497fe5c7a0795f0cbfd466a6bfa31968d0f70841",
        "exotic someone fall kitten salute nerve chimney enlist pair display over inside",
    ),
    (
        "0x42F3dc38Da81e984B92A95CBdAAA5fA2bd5cb1Ba",
        "0x4d91647d0a8429ac4433c83254fb9625332693c848e578062fe96362f32bfe91",
        "catch tragic rib twelve buffalo also gorilla toward cost enforce artefact slab",
    ),
    (
        "0x64F47EeD3dC749d13e49291d46Ea8378755fB6DF",
        "0x41c9f9518aa07b50cb1c0cc160d45547f57638dd824a8d85b5eb3bf99ed2bdeb",
        "arrange price fragile dinner device general vital excite penalty monkey major faculty",
    ),
    (
        "0xe2b8Cb53a43a56d4d2AB6131C81Bd76B86D3AFe5",
        "0xb0680d66303a0163a19294f1ef8c95cd69a9d7902a4aca99c05f3e134e68a11a",
        "increase pulp sing wood guilt cement satoshi tiny forum nuclear sudden thank",
    ),
];

/// Represents fork config for [ZkSyncNode].
#[derive(Debug, Default)]
pub struct Fork {
    url: String,
    block: Option<u64>,
}

impl Fork {
    /// Create a fork config with the provided url and the latest block.
    pub fn new(url: String) -> Self {
        Self { url, ..Default::default() }
    }

    /// Create a fork config with the provided url and block.
    pub fn new_with_block(url: String, block: u64) -> Self {
        Self { url, block: Some(block) }
    }
}

fn new_fork_config(url: &str) -> ForkConfig {
    const MAINNET_URL: &str = "https://mainnet.era.zksync.io:443";
    const SEPOLIA_TESTNET_URL: &str = "https://sepolia.era.zksync.dev:443";
    const ABSTRACT_MAINNET_URL: &str = "https://api.mainnet.abs.xyz";
    const ABSTRACT_TESTNET_URL: &str = "https://api.testnet.abs.xyz";

    match url {
        "mainnet" => ForkConfig {
            url: MAINNET_URL.parse().unwrap(),
            estimate_gas_price_scale_factor: 1.5,
            estimate_gas_scale_factor: 1.3,
        },
        "sepolia-testnet" => ForkConfig {
            url: SEPOLIA_TESTNET_URL.parse().unwrap(),
            estimate_gas_price_scale_factor: 2.0,
            estimate_gas_scale_factor: 1.3,
        },
        "abstract" => ForkConfig {
            url: ABSTRACT_MAINNET_URL.parse().unwrap(),
            estimate_gas_price_scale_factor: 1.5,
            estimate_gas_scale_factor: 1.3,
        },
        "abstract-testnet" => ForkConfig {
            url: ABSTRACT_TESTNET_URL.parse().unwrap(),
            estimate_gas_price_scale_factor: 1.5,
            estimate_gas_scale_factor: 1.3,
        },
        _ => ForkConfig {
            url: url.parse().unwrap(),
            estimate_gas_price_scale_factor: DEFAULT_ESTIMATE_GAS_PRICE_SCALE_FACTOR,
            estimate_gas_scale_factor: DEFAULT_ESTIMATE_GAS_SCALE_FACTOR,
        },
    }
}

/// In-memory anvil-zksync that is stopped when dropped.
pub struct ZkSyncNode {
    port: u16,
    _guard: tokio::sync::oneshot::Sender<()>,
}

impl ZkSyncNode {
    /// Returns the server url.
    #[inline]
    pub fn url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    /// Start anvil-zksync in memory, binding a random available port.
    ///
    /// The server is automatically stopped when the instance is dropped.
    pub async fn start() -> Self {
        Self::start_inner(None).await
    }

    /// Start anvil-zksync in memory, binding a random available port and with the provided fork url
    /// and block.
    ///
    /// The server is automatically stopped when the instance is dropped.
    pub async fn start_with_fork(fork: Fork) -> Self {
        Self::start_inner(Some(fork)).await
    }

    async fn start_inner(fork: Option<Fork>) -> Self {
        let (_guard, guard_rx) = tokio::sync::oneshot::channel::<()>();
        let (port_tx, port) = tokio::sync::oneshot::channel();

        let fork = if let Some(fork) = fork {
            Some(
                ForkClient::at_block_number(
                    new_fork_config(&fork.url),
                    fork.block.map(|block| L2BlockNumber(block as u32)),
                )
                .await
                .expect("failed creating fork config"),
            )
        } else {
            None
        };

        std::thread::spawn(move || {
            // We need to spawn a thread since `run_inner` future is not `Send`.
            let runtime =
                tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
            runtime.block_on(Self::run_inner(port_tx, guard_rx, fork));
        });

        // wait for server to start
        let port = port.await.expect("failed to start server");

        Self { port, _guard }
    }

    async fn run_inner(
        port_tx: tokio::sync::oneshot::Sender<u16>,
        stop_guard: tokio::sync::oneshot::Receiver<()>,
        fork_client: Option<ForkClient>,
    ) {
        // We need to init telemetry else anvil-zksync will panic.
        zksync_telemetry::init_telemetry("", "", "", None, None, None).await.ok();

        const MAX_TRANSACTIONS: usize = 100; // Not that important for testing purposes.

        let config = TestNodeConfig::default()
            .with_l1_gas_price(Some(DEFAULT_L1_GAS_PRICE))
            .with_l2_gas_price(Some(DEFAULT_L2_GAS_PRICE))
            .with_price_scale(Some(DEFAULT_ESTIMATE_GAS_PRICE_SCALE_FACTOR))
            .with_gas_limit_scale(Some(DEFAULT_ESTIMATE_GAS_SCALE_FACTOR))
            .with_l1_pubdata_price(Some(DEFAULT_FAIR_PUBDATA_PRICE))
            .with_chain_id(Some(DEFAULT_ERA_CHAIN_ID))
            .with_cache_config(Some(CacheConfig::Memory))
            .with_bytecode_compression(Some(true)); // This currently is a inverted boolean bug on anvil-zksync and should be fixed

        let impersonation = ImpersonationManager::default();
        let pool = TxPool::new(impersonation.clone(), config.transaction_order);
        let fee_input_provider =
            TestNodeFeeInputProvider::from_fork(fork_client.as_ref().map(|f| &f.details));
        let filters = Arc::new(RwLock::new(EthFilters::default()));
        let system_contracts = SystemContracts::from_options(
            &SystemContractsOptions::BuiltInWithoutSecurity,
            false,
            false,
        );
        let storage_key_layout = StorageKeyLayout::ZkEra;

        let (inner, storage, blockchain, time, fork, vm_runner) = InMemoryNodeInner::init(
            fork_client,
            fee_input_provider.clone(),
            filters,
            config.clone(),
            impersonation.clone(),
            system_contracts.clone(),
            storage_key_layout,
            false,
        );

        let mut node_service_tasks: Vec<Pin<Box<dyn Future<Output = anyhow::Result<()>>>>> =
            Vec::new();

        let (node_executor, node_handle) =
            NodeExecutor::new(inner.clone(), vm_runner, storage_key_layout);

        let sealing_mode = BlockSealerMode::immediate(MAX_TRANSACTIONS, pool.add_tx_listener());
        let (block_sealer, block_sealer_state) =
            BlockSealer::new(sealing_mode, pool.clone(), node_handle.clone());
        node_service_tasks.push(Box::pin(block_sealer.run()));

        let node: InMemoryNode = InMemoryNode::new(
            inner,
            blockchain,
            storage,
            fork,
            node_handle,
            None,
            time,
            impersonation,
            pool,
            block_sealer_state,
            system_contracts,
            storage_key_layout,
        );

        for wallet in LEGACY_RICH_WALLETS.iter() {
            let address = wallet.0;
            node.set_rich_account(
                H160::from_str(address).unwrap(),
                U256::from(1000u128 * 10u128.pow(18)),
            )
            .await;
        }
        for wallet in RICH_WALLETS.iter() {
            let address = wallet.0;
            node.set_rich_account(
                H160::from_str(address).unwrap(),
                U256::from(1000u128 * 10u128.pow(18)),
            )
            .await;
        }

        tokio::spawn(async move {
            node_executor.run().await.expect("node executor failed to start");
        });

        let server_builder =
            NodeServerBuilder::new(node.clone(), L1Sidecar::none(), AllowOrigin::any());

        let server = server_builder
            .build(SocketAddr::from(([0, 0, 0, 0], 0)))
            .await
            .expect("failed building server");

        // if no receiver was ready to receive the spawning thread died
        port_tx.send(server.local_addr().port()).expect("failed to send port");

        let server_handle = server.run();

        let node_service_stopped = futures::future::select_all(node_service_tasks);
        let any_server_stopped = server_handle.stopped();

        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                tracing::trace!("received shutdown signal, shutting down");
            },
            _ = any_server_stopped => {
                tracing::trace!("node server was stopped")
            },
            _ = node_service_stopped => {
                tracing::trace!("node service was stopped")
            },
            _ = stop_guard => {
                tracing::trace!("node server was stopped by guard")
            }
        };
    }

    pub fn rich_wallets() -> impl Iterator<Item = (&'static str, &'static str, &'static str)> {
        RICH_WALLETS.iter().copied()
    }
}
