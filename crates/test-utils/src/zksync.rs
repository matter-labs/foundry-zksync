//! Contains in-memory implementation of era-test-node.
use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr},
    str::FromStr,
};

use era_test_node::{
    http_fork_source::HttpForkSource,
    namespaces::{
        ConfigurationApiNamespaceT, DebugNamespaceT, EthNamespaceT, EthTestNodeNamespaceT,
        EvmNamespaceT, HardhatNamespaceT, NetNamespaceT, Web3NamespaceT, ZksNamespaceT,
    },
    node::InMemoryNode,
};
use jsonrpc_core::IoHandler;
use zksync_types::H160;

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

/// In-memory era-test-node that is stopped when dropped.
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

    /// Start era-test-node in memory, binding a random available port
    ///
    /// The server is automatically stopped when the instance is dropped.
    pub fn start() -> Self {
        let (_guard, _guard_rx) = tokio::sync::oneshot::channel::<()>();

        let io_handler = {
            let node: InMemoryNode<HttpForkSource> =
                InMemoryNode::new(None, None, Default::default(), None);

            for wallet in LEGACY_RICH_WALLETS.iter() {
                let address = wallet.0;
                node.set_rich_account(H160::from_str(address).unwrap());
            }
            for wallet in RICH_WALLETS.iter() {
                let address = wallet.0;
                node.set_rich_account(H160::from_str(address).unwrap());
            }

            let mut io = IoHandler::default();

            io.extend_with(NetNamespaceT::to_delegate(node.clone()));
            io.extend_with(Web3NamespaceT::to_delegate(node.clone()));
            io.extend_with(ConfigurationApiNamespaceT::to_delegate(node.clone()));
            io.extend_with(DebugNamespaceT::to_delegate(node.clone()));
            io.extend_with(EthNamespaceT::to_delegate(node.clone()));
            io.extend_with(EthTestNodeNamespaceT::to_delegate(node.clone()));
            io.extend_with(EvmNamespaceT::to_delegate(node.clone()));
            io.extend_with(HardhatNamespaceT::to_delegate(node.clone()));
            io.extend_with(ZksNamespaceT::to_delegate(node));
            io
        };
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 0);
        let (port_tx, port) = tokio::sync::oneshot::channel();

        std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .worker_threads(2)
                .build()
                .unwrap();

            let server = jsonrpc_http_server::ServerBuilder::new(io_handler)
                .threads(1)
                .event_loop_executor(runtime.handle().clone())
                .start_http(&addr)
                .unwrap();

            // if no receiver was ready to receive the spawning thread died
            _ = port_tx.send(server.address().port());
            // we only care that the channel is alive
            _ = tokio::task::block_in_place(move || runtime.block_on(_guard_rx));

            server.close();
        });

        // wait for server to start
        std::thread::sleep(std::time::Duration::from_millis(600));
        let port =
            tokio::task::block_in_place(move || tokio::runtime::Handle::current().block_on(port))
                .expect("failed to start server");

        Self { _guard, port }
    }

    pub fn rich_wallets() -> impl Iterator<Item = (&'static str, &'static str, &'static str)> {
        RICH_WALLETS.iter().copied()
    }
}
