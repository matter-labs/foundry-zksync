use foundry_test_utils::{forgetest_async, util, TestProject};

use crate::test_helpers::run_script_test;

forgetest_async!(script_zk_can_deploy_nft, |prj, cmd| {
    setup_nft_prj(&mut prj);
    run_script_test(prj.root(), &mut cmd, "NFT", "MyScript", Some("transmissions11/solmate"), 1);
});

fn setup_nft_prj(prj: &mut TestProject) {
    util::initialize(prj.root());
    prj.add_script("NFT.s.sol", include_str!("../../fixtures/zk/NFT.s.sol")).unwrap();
}