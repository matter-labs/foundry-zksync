use foundry_test_utils::{forgetest_async, util, ZkSyncNode};

forgetest_async!(forge_zk_can_deploy_erc20, |prj, cmd| {
    util::initialize(prj.root());
    prj.add_source("ERC20.sol", include_str!("../../../../../testdata/zk/ERC20.sol")).unwrap();

    let node = ZkSyncNode::start();
    let url = node.url();

    let private_key =
        ZkSyncNode::rich_wallets().next().map(|(_, pk, _)| pk).expect("No rich wallets available");

    cmd.forge_fuse().args([
        "create",
        "--zk-startup",
        "./src/ERC20.sol:MyToken",
        "--rpc-url",
        url.as_str(),
        "--private-key",
        private_key,
    ]);

    let (stdout, _) = cmd.output_lossy();
    assert!(stdout.contains("Deployer: "));
    assert!(stdout.contains("Deployed to: "));
});

forgetest_async!(forge_zk_can_deploy_token_receiver, |prj, cmd| {
    util::initialize(prj.root());
    prj.add_source(
        "TokenReceiver.sol",
        include_str!("../../../../../testdata/zk/TokenReceiver.sol"),
    )
    .unwrap();

    let node = ZkSyncNode::start();
    let url = node.url();

    let private_key =
        ZkSyncNode::rich_wallets().next().map(|(_, pk, _)| pk).expect("No rich wallets available");

    cmd.forge_fuse().args([
        "create",
        "--zk-startup",
        "./src/TokenReceiver.sol:TokenReceiver",
        "--rpc-url",
        url.as_str(),
        "--private-key",
        private_key,
    ]);

    let (stdout, _) = cmd.output_lossy();
    assert!(stdout.contains("Deployer: "));
    assert!(stdout.contains("Deployed to: "));
});
