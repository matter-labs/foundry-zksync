//! Regression tests for previous issues.

use crate::{config::*, test_helpers::TEST_DATA_DEFAULT};
use alloy_dyn_abi::{DecodedEvent, DynSolValue, EventExt};
use alloy_json_abi::Event;
use alloy_primitives::{Address, U256, address, b256};
use forge::{
    constants::HARDHAT_CONSOLE_ADDRESS,
    decode::decode_console_logs,
    result::{TestKind, TestStatus},
};
use foundry_config::{Config, FsPermissions, fs_permissions::PathPermission};
use foundry_evm::traces::{CallKind, CallTraceDecoder, DecodedCallData, TraceKind};
use foundry_test_utils::Filter;
use std::sync::Arc;

/// Creates a test that runs `testdata/repros/Issue{issue}.t.sol`.
macro_rules! test_repro {
    ($(#[$attr:meta])* $issue_number:literal $(,)?) => {
        test_repro!($(#[$attr])* $issue_number, false, None);
    };
    ($(#[$attr:meta])* $issue_number:literal, $should_fail:expr $(,)?) => {
        test_repro!($(#[$attr])* $issue_number, $should_fail, None);
    };
    ($(#[$attr:meta])* $issue_number:literal, $should_fail:expr, $sender:expr $(,)?) => {
        paste::paste! {
            #[tokio::test(flavor = "multi_thread")]
            $(#[$attr])*
            async fn [< issue_ $issue_number >]() {
                repro_config($issue_number, $should_fail, $sender.into()).await.run().await;
            }
        }
    };
    ($(#[$attr:meta])* $issue_number:literal, $should_fail:expr, $sender:expr, |$res:ident| $e:expr $(,)?) => {
        paste::paste! {
            #[tokio::test(flavor = "multi_thread")]
            $(#[$attr])*
            async fn [< issue_ $issue_number >]() {
                let mut $res = repro_config($issue_number, $should_fail, $sender.into()).await.test().unwrap();
                $e
            }
        }
    };
    ($(#[$attr:meta])* $issue_number:literal; |$config:ident| $e:expr $(,)?) => {
        paste::paste! {
            #[tokio::test(flavor = "multi_thread")]
            $(#[$attr])*
            async fn [< issue_ $issue_number >]() {
                let mut $config = repro_config($issue_number, false, None).await;
                $e
                $config.run().await;
            }
        }
    };
}
pub(crate) use test_repro;

async fn repro_config(issue: usize, should_fail: bool, sender: Option<Address>) -> TestConfig {
    foundry_test_utils::init_tracing();
    let filter = Filter::path(&format!(".*repros/Issue{issue}.t.sol"));

    let runner = TEST_DATA_DEFAULT.runner_with(|config| {
        config.fs_permissions = FsPermissions::new(vec![
            PathPermission::read("./fixtures"),
            PathPermission::read("out"),
        ]);
        if let Some(sender) = sender {
            config.sender = sender;
        }
    });
    TestConfig::with_filter(runner, filter).set_should_fail(should_fail)
}

// https://github.com/foundry-rs/foundry/issues/2623
test_repro!(2623);

// https://github.com/foundry-rs/foundry/issues/2629
test_repro!(2629);

// https://github.com/foundry-rs/foundry/issues/2723
test_repro!(2723);

// https://github.com/foundry-rs/foundry/issues/2898
test_repro!(2898);

// https://github.com/foundry-rs/foundry/issues/2956
test_repro!(2956);

// https://github.com/foundry-rs/foundry/issues/2984
test_repro!(2984);

// https://github.com/foundry-rs/foundry/issues/3055
test_repro!(3055, true);

// https://github.com/foundry-rs/foundry/issues/3077
test_repro!(3077);

// https://github.com/foundry-rs/foundry/issues/3110
test_repro!(3110);

// https://github.com/foundry-rs/foundry/issues/3119
test_repro!(3119);

// https://github.com/foundry-rs/foundry/issues/3189
test_repro!(3189, true);

// https://github.com/foundry-rs/foundry/issues/3190
test_repro!(3190);

// https://github.com/foundry-rs/foundry/issues/3192
test_repro!(3192);

// https://github.com/foundry-rs/foundry/issues/3220
test_repro!(3220);

// https://github.com/foundry-rs/foundry/issues/3221
test_repro!(3221);

// https://github.com/foundry-rs/foundry/issues/3223
test_repro!(3223, false, address!("0xF0959944122fb1ed4CfaBA645eA06EED30427BAA"));

// https://github.com/foundry-rs/foundry/issues/3347
test_repro!(3347, false, None, |res| {
    let mut res = res.remove("default/repros/Issue3347.t.sol:Issue3347Test").unwrap();
    let test = res.test_results.remove("test()").unwrap();
    assert_eq!(test.logs.len(), 1);
    let event = Event::parse("event log2(uint256, uint256)").unwrap();
    let decoded = event.decode_log(&test.logs[0].data).unwrap();
    assert_eq!(
        decoded,
        DecodedEvent {
            selector: Some(b256!(
                "0x78b9a1f3b55d6797ab2c4537e83ee04ff0c65a1ca1bb39d79a62e0a78d5a8a57"
            )),
            indexed: vec![],
            body: vec![
                DynSolValue::Uint(U256::from(1), 256),
                DynSolValue::Uint(U256::from(2), 256)
            ]
        }
    );
});

// https://github.com/foundry-rs/foundry/issues/3437
// 1.0 related
// test_repro!(3437);

// https://github.com/foundry-rs/foundry/issues/3596
test_repro!(3596, true, None);

// https://github.com/foundry-rs/foundry/issues/3653
test_repro!(3653);

// https://github.com/foundry-rs/foundry/issues/3661
test_repro!(3661);

// https://github.com/foundry-rs/foundry/issues/3674
test_repro!(3674, false, address!("0xF0959944122fb1ed4CfaBA645eA06EED30427BAA"));

// https://github.com/foundry-rs/foundry/issues/3685
test_repro!(3685);

// https://github.com/foundry-rs/foundry/issues/3703
test_repro!(
    #[ignore = "flaky polygon RPCs"]
    3703
);

// https://github.com/foundry-rs/foundry/issues/3708
test_repro!(3708);

// https://github.com/foundry-rs/foundry/issues/3723
// 1.0 related
// test_repro!(3723);

// https://github.com/foundry-rs/foundry/issues/3753
test_repro!(3753);

// https://github.com/foundry-rs/foundry/issues/3792
test_repro!(3792);

// https://github.com/foundry-rs/foundry/issues/4232
test_repro!(4232);

// https://github.com/foundry-rs/foundry/issues/4402
test_repro!(4402);

// https://github.com/foundry-rs/foundry/issues/4586
test_repro!(4586);

// https://github.com/foundry-rs/foundry/issues/4630
test_repro!(4630);

// https://github.com/foundry-rs/foundry/issues/4640
test_repro!(4640);

// https://github.com/foundry-rs/foundry/issues/4832
// 1.0 related
// test_repro!(4832);

// https://github.com/foundry-rs/foundry/issues/5038
test_repro!(5038);

// https://github.com/foundry-rs/foundry/issues/5808
test_repro!(5808);

// <https://github.com/foundry-rs/foundry/issues/5929>
test_repro!(5929);

// <https://github.com/foundry-rs/foundry/issues/5935>
test_repro!(5935);

// <https://github.com/foundry-rs/foundry/issues/5948>
test_repro!(5948);

// https://github.com/foundry-rs/foundry/issues/6006
test_repro!(6006);

// https://github.com/foundry-rs/foundry/issues/6032
test_repro!(6032);

// https://github.com/foundry-rs/foundry/issues/6070
test_repro!(6070);

// https://github.com/foundry-rs/foundry/issues/6115
test_repro!(6115);

// https://github.com/foundry-rs/foundry/issues/6170
test_repro!(6170, false, None, |res| {
    let mut res = res.remove("default/repros/Issue6170.t.sol:Issue6170Test").unwrap();
    let test = res.test_results.remove("test()").unwrap();
    assert_eq!(test.status, TestStatus::Failure);
    assert_eq!(test.reason, Some("log != expected log".to_string()));
});

// <https://github.com/foundry-rs/foundry/issues/6293>
test_repro!(6293);

// https://github.com/foundry-rs/foundry/issues/6180
test_repro!(6180);

// https://github.com/foundry-rs/foundry/issues/6355
test_repro!(6355, false, None, |res| {
    let mut res = res.remove("default/repros/Issue6355.t.sol:Issue6355Test").unwrap();
    let test = res.test_results.remove("test_shouldFail()").unwrap();
    assert_eq!(test.status, TestStatus::Failure);

    let test = res.test_results.remove("test_shouldFailWithRevertToState()").unwrap();
    assert_eq!(test.status, TestStatus::Failure);
});

// https://github.com/foundry-rs/foundry/issues/6437
test_repro!(6437);

// Test we decode Hardhat console logs AND traces correctly.
// https://github.com/foundry-rs/foundry/issues/6501
test_repro!(6501, false, None, |res| {
    let mut res = res.remove("default/repros/Issue6501.t.sol:Issue6501Test").unwrap();
    let test = res.test_results.remove("test_hhLogs()").unwrap();
    assert_eq!(test.status, TestStatus::Success);
    assert_eq!(
        decode_console_logs(&test.logs),
        ["a".to_string(), "1".to_string(), "b 2".to_string()]
    );

    let (kind, traces) = test.traces.last().unwrap().clone();
    let nodes = traces.arena.into_nodes();
    assert_eq!(kind, TraceKind::Execution);

    let test_call = nodes.first().unwrap();
    assert_eq!(test_call.idx, 0);
    assert_eq!(test_call.children, [1, 2, 3]);
    assert_eq!(test_call.trace.depth, 0);
    assert!(test_call.trace.success);

    let expected = [
        ("log(string)", vec!["\"a\""]),
        ("log(uint256)", vec!["1"]),
        ("log(string,uint256)", vec!["\"b\"", "2"]),
    ];
    for (node, expected) in nodes[1..=3].iter().zip(expected) {
        let trace = &node.trace;
        let decoded = CallTraceDecoder::new().decode_function(trace).await;
        assert_eq!(trace.kind, CallKind::StaticCall);
        assert_eq!(trace.address, HARDHAT_CONSOLE_ADDRESS);
        assert_eq!(decoded.label, Some("console".into()));
        assert_eq!(trace.depth, 1);
        assert!(trace.success);
        assert_eq!(
            decoded.call_data,
            Some(DecodedCallData {
                signature: expected.0.into(),
                args: expected.1.into_iter().map(ToOwned::to_owned).collect(),
            })
        );
    }
});

// https://github.com/foundry-rs/foundry/issues/6538
test_repro!(6538);

// https://github.com/foundry-rs/foundry/issues/6554
test_repro!(6554; |config| {
    let path = config.runner.config.root.join("out/default/Issue6554.t.sol");

    let mut prj_config = Config::clone(&config.runner.config);
    prj_config.fs_permissions.add(PathPermission::read_write(path));
    config.runner.config = Arc::new(prj_config);

});

// https://github.com/foundry-rs/foundry/issues/6759
test_repro!(6759);

// https://github.com/foundry-rs/foundry/issues/6966
test_repro!(6966);

// https://github.com/foundry-rs/foundry/issues/6616
test_repro!(6616);

// https://github.com/foundry-rs/foundry/issues/5529
test_repro!(5529; |config| {
  let mut prj_config = Config::clone(&config.runner.config);
  prj_config.always_use_create_2_factory = true;
  config.runner.evm_opts.always_use_create_2_factory = true;
  config.runner.config = Arc::new(prj_config);
});

// https://github.com/foundry-rs/foundry/issues/6634
test_repro!(6634; |config| {
  let mut prj_config = Config::clone(&config.runner.config);
  prj_config.always_use_create_2_factory = true;
  config.runner.evm_opts.always_use_create_2_factory = true;
  config.runner.config = Arc::new(prj_config);
});

// https://github.com/foundry-rs/foundry/issues/7457
test_repro!(7457);

// https://github.com/foundry-rs/foundry/issues/7481
test_repro!(7481);

// https://github.com/foundry-rs/foundry/issues/5739
test_repro!(5739);

// https://github.com/foundry-rs/foundry/issues/8004
test_repro!(8004);

// https://github.com/foundry-rs/foundry/issues/2851
test_repro!(2851, false, None, |res| {
    let mut res = res.remove("default/repros/Issue2851.t.sol:Issue2851Test").unwrap();
    let test = res.test_results.remove("invariantNotZero()").unwrap();
    assert_eq!(test.status, TestStatus::Failure);
});

// https://github.com/foundry-rs/foundry/issues/8006
test_repro!(8006);

// https://github.com/foundry-rs/foundry/issues/8277
test_repro!(8277);

// https://github.com/foundry-rs/foundry/issues/8287
test_repro!(8287);

// https://github.com/foundry-rs/foundry/issues/8168
test_repro!(8168);

// https://github.com/foundry-rs/foundry/issues/8383
test_repro!(8383, false, None, |res| {
    let mut res = res.remove("default/repros/Issue8383.t.sol:Issue8383Test").unwrap();
    let test = res.test_results.remove("testP256VerifyOutOfBounds()").unwrap();
    assert_eq!(test.status, TestStatus::Success);
    match test.kind {
        TestKind::Unit { gas } => assert_eq!(gas, 3101),
        _ => panic!("not a unit test kind"),
    }
});

// https://github.com/foundry-rs/foundry/issues/6643
test_repro!(6643);

// https://github.com/foundry-rs/foundry/issues/8971
test_repro!(8971; |config| {
  let mut prj_config = Config::clone(&config.runner.config);
  prj_config.isolate = true;
  config.runner.config = Arc::new(prj_config);
});

// https://github.com/foundry-rs/foundry/issues/8639
test_repro!(8639);

// https://github.com/foundry-rs/foundry/issues/8566
test_repro!(8566);

// https://github.com/foundry-rs/foundry/issues/9643
test_repro!(9643);

// https://github.com/foundry-rs/foundry/issues/7238
test_repro!(7238);

// https://github.com/foundry-rs/foundry/issues/10302
test_repro!(10302);

// https://github.com/foundry-rs/foundry/issues/10477
test_repro!(10477);

// https://github.com/foundry-rs/foundry/issues/10527
test_repro!(10527);

// https://github.com/foundry-rs/foundry/issues/10552
test_repro!(10552);

// https://github.com/foundry-rs/foundry/issues/10586
test_repro!(10586);

// https://github.com/foundry-rs/foundry/issues/10957
test_repro!(10957);
