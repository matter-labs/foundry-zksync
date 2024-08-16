use foundry_test_utils::{forgetest_async, util, TestCommand, TestProject, ZkSyncNode};

forgetest_async!(script_zk_can_deploy_proxy, |prj, cmd| {
    setup_proxy_prj(&mut prj);
    run_proxy_script_test(prj.root(), &mut cmd, "ProxyScript", 4);
});

fn setup_proxy_prj(prj: &mut TestProject) {
    util::initialize(prj.root());
    prj.add_script("Proxy.s.sol", include_str!("../../fixtures/zk/Proxy.s.sol")).unwrap();
}

fn run_proxy_script_test(
    root: impl AsRef<std::path::Path>,
    cmd: &mut TestCommand,
    name: &str,
    expected_broadcastable_txs: usize,
) {
    let node = ZkSyncNode::start();

    cmd.args(["install", "OpenZeppelin/openzeppelin-contracts"])
        .args(["--no-commit"])
        .ensure_execute_success()
        .expect("Installed successfully");

    cmd.forge_fuse();

    cmd.arg("script").args([
        "--zk-startup",
        &format!("./script/Proxy.s.sol:{name}"),
        "--broadcast",
        "--private-key",
        "0x3d3cbc973389cb26f657686445bcc75662b415b656078503592ac8c1abb8810e",
        "--chain",
        "260",
        "--gas-estimate-multiplier",
        "310",
        "--rpc-url",
        node.url().as_str(),
        "--slow",
        "--evm-version",
        "shanghai",
    ]);

    println!("OUTPUT: {:?}", cmd.stdout_lossy());

    let run_latest = foundry_common::fs::json_files(root.as_ref().join("broadcast").as_path())
        .find(|file| file.ends_with("run-latest.json"))
        .expect("No broadcast artifacts");

    let content = foundry_common::fs::read_to_string(run_latest).unwrap();

    let json: serde_json::Value = serde_json::from_str(&content).unwrap();
    assert_eq!(
        json["transactions"].as_array().expect("broadcastable txs").len(),
        expected_broadcastable_txs
    );
    cmd.forge_fuse();
}
