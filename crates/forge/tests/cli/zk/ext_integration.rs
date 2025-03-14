use foundry_test_utils::util::ExtTester;

#[test]
fn test_zk_aave_di() {
    ExtTester::new("Moonsong-Labs", "aave-delivery-infrastructure", "ci")
        .args(["--zksync", "--skip", "\"*/PayloadScripts.t.sol\""])
        .run()
}
