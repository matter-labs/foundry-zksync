//! Forge tests for zksync logs.

use std::{path::Path, sync::LazyLock};

use crate::{config::*, test_helpers::TEST_DATA_DEFAULT};
use alloy_primitives::{address, hex, Address, Bytes};
use forge::{
    revm::primitives::SpecId,
    traces::{CallKind, CallTraceNode, SparsedTraceArena, TraceKind},
};
use foundry_common::fs;
use foundry_test_utils::Filter;
use itertools::Itertools;
use serde::Deserialize;

const ADDRESS_ZK_TRACE_TEST: Address = address!("7fa9385be102ac3eac297483dd6233d62b3e1496");
const ADDRESS_ADDER: Address = address!("f9e9ba9ed9b96ab918c74b21dd0f1d5f2ac38a30");
const ADDRESS_NUMBER: Address = address!("f232f12e115391c535fd519b00efadf042fc8be5");
const ADDRESS_FIRST_INNER_NUMBER: Address = address!("ed570f3f91621894e001df0fb70bfbd123d3c8ad");
const ADDRESS_SECOND_INNER_NUMBER: Address = address!("abceaeac3d3a2ac3dcffd7a60ca00a3fac9490ca");
const ADDRESS_CONSOLE: Address = address!("000000000000000000636f6e736f6c652e6c6f67");
const SELECTOR_TEST_CALL: Bytes = Bytes::from_static(hex!("0d3282c4").as_slice());
const SELECTOR_TEST_CREATE: Bytes = Bytes::from_static(hex!("61bdc916").as_slice());
const SELECTOR_ADD: Bytes = Bytes::from_static(hex!("4f2be91f").as_slice());
const SELECTOR_FIVE: Bytes = Bytes::from_static(hex!("af11c34c").as_slice());
const SELECTOR_INNER_FIVE: Bytes = Bytes::from_static(hex!("3a0a858d").as_slice());
const VALUE_FIVE: Bytes = Bytes::from_static(
    hex!("0000000000000000000000000000000000000000000000000000000000000005").as_slice(),
);
const VALUE_TEN: Bytes = Bytes::from_static(
    hex!("000000000000000000000000000000000000000000000000000000000000000a").as_slice(),
);
const VALUE_LOG_UINT_TEN: Bytes = Bytes::from_static(
    hex!("f5b1bba9000000000000000000000000000000000000000000000000000000000000000a").as_slice(),
); // selector: log(uint)

static BYTECODE_ADDER: LazyLock<Vec<u8>> =
    LazyLock::new(|| get_zk_artifact_bytecode("Trace.t.sol/Adder.json"));
static BYTECODE_CONSTRUCTOR_ADDER: LazyLock<Vec<u8>> =
    LazyLock::new(|| get_zk_artifact_bytecode("Trace.t.sol/ConstructorAdder.json"));
static BYTECODE_NUMBER: LazyLock<Vec<u8>> =
    LazyLock::new(|| get_zk_artifact_bytecode("Trace.t.sol/Number.json"));
static BYTECODE_INNER_NUMBER: LazyLock<Vec<u8>> =
    LazyLock::new(|| get_zk_artifact_bytecode("Trace.t.sol/InnerNumber.json"));

fn get_zk_artifact_bytecode<P: AsRef<Path> + std::fmt::Debug>(path: P) -> Vec<u8> {
    #[derive(Deserialize)]
    struct Bytecode {
        object: String,
    }
    #[derive(Deserialize)]
    struct Artifact {
        bytecode: Bytecode,
    }

    let artifact =
        fs::read_json_file::<Artifact>(&Path::new("../../testdata/zk/zkout").join(&path))
            .unwrap_or_else(|err| panic!("failed reading artifact file {path:?}: {err:?}"));

    hex::decode(artifact.bytecode.object)
        .unwrap_or_else(|err| panic!("failed decoding artifact object {path:?}: {err:?}"))
}

#[tokio::test(flavor = "multi_thread")]
async fn test_zk_traces_work_during_call() {
    let runner = TEST_DATA_DEFAULT.runner_zksync();
    let filter = Filter::new("testZkTraceOutputDuringCall", "ZkTraceTest", ".*");

    let results = TestConfig::with_filter(runner, filter).evm_spec(SpecId::SHANGHAI).test();
    let traces =
        &results["zk/Trace.t.sol:ZkTraceTest"].test_results["testZkTraceOutputDuringCall()"].traces;

    assert_execution_trace(
        vec![TraceAssertion {
            kind: Some(CallKind::Call),
            address: Some(ADDRESS_ZK_TRACE_TEST),
            data: Some(SELECTOR_TEST_CALL),
            children: vec![
                TraceAssertion {
                    kind: Some(CallKind::Create),
                    address: Some(ADDRESS_ADDER),
                    output: Some(Bytes::from(LazyLock::force(&BYTECODE_ADDER).to_owned())),
                    ..Default::default()
                },
                TraceAssertion {
                    kind: Some(CallKind::Call),
                    address: Some(ADDRESS_ADDER),
                    data: Some(SELECTOR_ADD),
                    output: Some(VALUE_TEN),
                    children: vec![
                        TraceAssertion {
                            kind: Some(CallKind::Create),
                            address: Some(ADDRESS_NUMBER),
                            output: Some(Bytes::from(LazyLock::force(&BYTECODE_NUMBER).to_owned())),
                            ..Default::default()
                        },
                        TraceAssertion {
                            kind: Some(CallKind::Call),
                            address: Some(ADDRESS_NUMBER),
                            data: Some(SELECTOR_FIVE),
                            output: Some(VALUE_FIVE),
                            children: vec![
                                TraceAssertion {
                                    kind: Some(CallKind::Create),
                                    address: Some(ADDRESS_FIRST_INNER_NUMBER),
                                    output: Some(Bytes::from(
                                        LazyLock::force(&BYTECODE_INNER_NUMBER).to_owned(),
                                    )),
                                    ..Default::default()
                                },
                                TraceAssertion {
                                    kind: Some(CallKind::Call),
                                    address: Some(ADDRESS_FIRST_INNER_NUMBER),
                                    data: Some(SELECTOR_INNER_FIVE),
                                    output: Some(VALUE_FIVE),
                                    ..Default::default()
                                },
                            ],
                        },
                        TraceAssertion {
                            kind: Some(CallKind::Call),
                            address: Some(ADDRESS_NUMBER),
                            data: Some(SELECTOR_FIVE),
                            output: Some(VALUE_FIVE),
                            children: vec![
                                TraceAssertion {
                                    kind: Some(CallKind::Create),
                                    address: Some(ADDRESS_SECOND_INNER_NUMBER),
                                    output: Some(Bytes::from(
                                        LazyLock::force(&BYTECODE_INNER_NUMBER).to_owned(),
                                    )),
                                    ..Default::default()
                                },
                                TraceAssertion {
                                    kind: Some(CallKind::Call),
                                    address: Some(ADDRESS_SECOND_INNER_NUMBER),
                                    data: Some(SELECTOR_INNER_FIVE),
                                    output: Some(VALUE_FIVE),
                                    ..Default::default()
                                },
                            ],
                        },
                    ],
                },
                TraceAssertion {
                    kind: Some(CallKind::StaticCall),
                    address: Some(ADDRESS_CONSOLE),
                    data: Some(VALUE_LOG_UINT_TEN),
                    ..Default::default()
                },
            ],
            ..Default::default()
        }],
        traces,
    )
}

#[tokio::test(flavor = "multi_thread")]
async fn test_zk_traces_work_during_create() {
    let runner = TEST_DATA_DEFAULT.runner_zksync();
    let filter = Filter::new("testZkTraceOutputDuringCreate", "ZkTraceTest", ".*");

    let results = TestConfig::with_filter(runner, filter).evm_spec(SpecId::SHANGHAI).test();
    let traces = results["zk/Trace.t.sol:ZkTraceTest"].test_results
        ["testZkTraceOutputDuringCreate()"]
        .traces
        .as_slice();

    assert_execution_trace(
        vec![TraceAssertion {
            kind: Some(CallKind::Call),
            address: Some(ADDRESS_ZK_TRACE_TEST),
            data: Some(SELECTOR_TEST_CREATE),
            children: vec![TraceAssertion {
                kind: Some(CallKind::Create),
                address: Some(ADDRESS_ADDER),
                output: Some(Bytes::from(LazyLock::force(&BYTECODE_CONSTRUCTOR_ADDER).to_owned())),
                children: vec![
                    TraceAssertion {
                        kind: Some(CallKind::Create),
                        address: Some(ADDRESS_NUMBER),
                        output: Some(Bytes::from(LazyLock::force(&BYTECODE_NUMBER).to_owned())),
                        ..Default::default()
                    },
                    TraceAssertion {
                        kind: Some(CallKind::Call),
                        address: Some(ADDRESS_NUMBER),
                        data: Some(SELECTOR_FIVE),
                        output: Some(VALUE_FIVE),
                        children: vec![
                            TraceAssertion {
                                kind: Some(CallKind::Create),
                                address: Some(ADDRESS_FIRST_INNER_NUMBER),
                                output: Some(Bytes::from(
                                    LazyLock::force(&BYTECODE_INNER_NUMBER).to_owned(),
                                )),
                                ..Default::default()
                            },
                            TraceAssertion {
                                kind: Some(CallKind::Call),
                                address: Some(ADDRESS_FIRST_INNER_NUMBER),
                                data: Some(SELECTOR_INNER_FIVE),
                                output: Some(VALUE_FIVE),
                                ..Default::default()
                            },
                        ],
                    },
                    TraceAssertion {
                        kind: Some(CallKind::Call),
                        address: Some(ADDRESS_NUMBER),
                        data: Some(SELECTOR_FIVE),
                        output: Some(VALUE_FIVE),
                        children: vec![
                            TraceAssertion {
                                kind: Some(CallKind::Create),
                                address: Some(ADDRESS_SECOND_INNER_NUMBER),
                                output: Some(Bytes::from(
                                    LazyLock::force(&BYTECODE_INNER_NUMBER).to_owned(),
                                )),
                                ..Default::default()
                            },
                            TraceAssertion {
                                kind: Some(CallKind::Call),
                                address: Some(ADDRESS_SECOND_INNER_NUMBER),
                                data: Some(SELECTOR_INNER_FIVE),
                                output: Some(VALUE_FIVE),
                                ..Default::default()
                            },
                        ],
                    },
                    TraceAssertion {
                        kind: Some(CallKind::Call),
                        address: Some(ADDRESS_CONSOLE),
                        data: Some(VALUE_LOG_UINT_TEN),
                        ..Default::default()
                    },
                ],
                ..Default::default()
            }],
            ..Default::default()
        }],
        traces,
    )
}

#[derive(Default, Debug)]
struct TraceAssertion {
    kind: Option<CallKind>,
    address: Option<Address>,
    data: Option<Bytes>,
    output: Option<Bytes>,
    children: Vec<TraceAssertion>,
}

/// Assert that the execution trace matches the actual trace.
fn assert_execution_trace(
    expected: Vec<TraceAssertion>,
    traces: &[(TraceKind, SparsedTraceArena)],
) {
    #[allow(dead_code)]
    #[derive(Debug)]
    struct AssertionFailure {
        field: String,
        expected: String,
        actual: String,
        path: Vec<usize>,
    }

    fn assert_recursive(
        expected: &[TraceAssertion],
        actual: &[DecodedTrace],
    ) -> Option<AssertionFailure> {
        for (idx, expected_node) in expected.iter().enumerate() {
            let actual_node = match actual.get(idx) {
                Some(actual) => actual,
                None => {
                    return Some(AssertionFailure {
                        field: "<entry>".to_string(),
                        expected: "<entry>".to_string(),
                        actual: "<none>".to_string(),
                        path: vec![idx],
                    })
                }
            };
            if let Some(kind) = expected_node.kind {
                if kind != actual_node.kind {
                    return Some(AssertionFailure {
                        field: "kind".to_string(),
                        expected: format!("{kind:?}"),
                        actual: format!("{:?}", actual_node.kind),
                        path: vec![idx],
                    });
                }
            }
            if let Some(address) = expected_node.address {
                if address != actual_node.address {
                    return Some(AssertionFailure {
                        field: "address".to_string(),
                        expected: format!("{address:?}"),
                        actual: format!("{:?}", actual_node.address),
                        path: vec![idx],
                    });
                }
            }
            if let Some(data) = &expected_node.data {
                if data != &actual_node.data {
                    return Some(AssertionFailure {
                        field: "data".to_string(),
                        expected: format!("{data:?}"),
                        actual: format!("{:?}", actual_node.data),
                        path: vec![idx],
                    });
                }
            }
            if let Some(output) = &expected_node.output {
                if output != &actual_node.output {
                    return Some(AssertionFailure {
                        field: "output".to_string(),
                        expected: format!("{output:?}"),
                        actual: format!("{:?}", actual_node.output),
                        path: vec![idx],
                    });
                }
            }

            if let Some(mut failure) =
                assert_recursive(&expected_node.children, &actual_node.children)
            {
                failure.path.insert(0, idx);
                return Some(failure)
            }
        }
        None
    }

    let actual = decode_first_execution_trace(traces);
    if let Some(failure) = assert_recursive(&expected, &actual) {
        println!("---");
        println!("{failure:#?}");
        println!("---");
        println!("Trace:");
        let mut actual = &actual;
        for (depth, idx) in failure.path.iter().enumerate() {
            let trace = &actual[*idx];
            println!(
                "{}{:?} {:?} {:?} {:?}",
                "  ".repeat(depth),
                trace.kind,
                trace.address,
                trace.data,
                trace.output
            );

            actual = &trace.children;
        }
        println!("---\n");
        panic!("trace assertion failure");
    }
}

/// Represents the decoded trace.
#[allow(dead_code)]
#[derive(Debug, Default)]
struct DecodedTrace {
    idx: usize,
    depth: usize,
    kind: CallKind,
    address: Address,
    data: Bytes,
    output: Bytes,
    children: Vec<DecodedTrace>,
}

/// Decodes and returns the first execution trace.
fn decode_first_execution_trace(traces: &[(TraceKind, SparsedTraceArena)]) -> Vec<DecodedTrace> {
    fn decode_recursive(nodes: &[CallTraceNode], node: &CallTraceNode) -> DecodedTrace {
        let children =
            node.children.iter().map(|idx| decode_recursive(nodes, &nodes[*idx])).collect_vec();
        DecodedTrace {
            idx: node.idx,
            depth: node.trace.depth,
            kind: node.trace.kind,
            address: node.trace.address,
            data: node.trace.data.clone(),
            output: node.trace.output.clone(),
            children,
        }
    }

    traces
        .iter()
        .find(|(kind, _)| matches!(kind, TraceKind::Execution))
        .map(|(_, trace)| {
            let mut decoded_nodes = vec![];
            let nodes = trace.nodes();
            nodes
                .iter()
                .filter(|node| node.parent.is_none())
                .for_each(|node| decoded_nodes.push(decode_recursive(nodes, node)));

            decoded_nodes
        })
        .unwrap_or_default()
}
