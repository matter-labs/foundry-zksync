//! Forge tests for zkysnc functionality.
mod basic;
mod cheats;
mod contracts;
mod factory;
mod fuzz;
mod invariant;
mod logs;
mod repros;

macro_rules! test_zk {
    ($name:ident; |$runner:ident| $e:expr $(,)?) => {
        paste::paste! {
            #[tokio::test(flavor = "multi_thread")]
            async fn [< test_zk_ $name>]() {
                let $runner = crate::test_helpers::TEST_DATA_DEFAULT.runner_zksync();
                $e
            }
        }
    };
    ($name:ident, $test_name_pat:literal) => {
        test_zk!($name, $test_name_pat, ".*", ".*");
    };
    ($name:ident, $test_name_pat:literal, $contract_name_pat:literal) => {
        test_zk!($name, $test_name_pat, $contract_name_pat, ".*");
    };
    ($name:ident, $test_name_pattern:literal, $contract_name_pattern:literal, $path_pattern:literal) => {
        test_zk!($name; |runner| {
            let filter = foundry_test_utils::Filter::new(
                $test_name_pattern,
                $contract_name_pattern,
                $path_pattern,
            );
            crate::config::TestConfig::with_filter(runner, filter)
                .evm_spec(forge::revm::primitives::SpecId::SHANGHAI)
                .run()
                .await;
        });
    };
}
pub(crate) use test_zk;
