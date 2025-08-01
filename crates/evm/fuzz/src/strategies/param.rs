use super::state::EvmFuzzState;
use alloy_dyn_abi::{DynSolType, DynSolValue};
use alloy_primitives::{Address, B256, I256, U256};
use proptest::prelude::*;
use rand::{SeedableRng, rngs::StdRng};

/// The max length of arrays we fuzz for is 256.
const MAX_ARRAY_LEN: usize = 256;

/// Given a parameter type, returns a strategy for generating values for that type.
///
/// See [`fuzz_param_with_fixtures`] for more information.
pub fn fuzz_param(
    param: &DynSolType,
    no_zksync_reserved_addresses: bool,
) -> BoxedStrategy<DynSolValue> {
    fuzz_param_inner(param, None, no_zksync_reserved_addresses)
}

/// Given a parameter type and configured fixtures for param name, returns a strategy for generating
/// values for that type.
///
/// Fixtures can be currently generated for uint, int, address, bytes and
/// string types and are defined for parameter name.
/// For example, fixtures for parameter `owner` of type `address` can be defined in a function with
/// a `function fixture_owner() public returns (address[] memory)` signature.
///
/// Fixtures are matched on parameter name, hence fixtures defined in
/// `fixture_owner` function can be used in a fuzzed test function with a signature like
/// `function testFuzz_ownerAddress(address owner, uint amount)`.
///
/// Raises an error if all the fixture types are not of the same type as the input parameter.
///
/// Works with ABI Encoder v2 tuples.
pub fn fuzz_param_with_fixtures(
    param: &DynSolType,
    fixtures: Option<&[DynSolValue]>,
    name: &str,
    no_zksync_reserved_addresses: bool,
) -> BoxedStrategy<DynSolValue> {
    fuzz_param_inner(param, fixtures.map(|f| (f, name)), no_zksync_reserved_addresses)
}

fn fuzz_param_inner(
    param: &DynSolType,
    mut fuzz_fixtures: Option<(&[DynSolValue], &str)>,
    no_zksync_reserved_addresses: bool,
) -> BoxedStrategy<DynSolValue> {
    if let Some((fixtures, name)) = fuzz_fixtures
        && !fixtures.iter().all(|f| f.matches(param))
    {
        error!("fixtures for {name:?} do not match type {param}");
        fuzz_fixtures = None;
    }
    let fuzz_fixtures = fuzz_fixtures.map(|(f, _)| f);

    let value = || {
        let default_strategy = DynSolValue::type_strategy(param);
        if let Some(fixtures) = fuzz_fixtures {
            proptest::prop_oneof![
                50 => {
                    let fixtures = fixtures.to_vec();
                    any::<prop::sample::Index>()
                        .prop_map(move |index| index.get(&fixtures).clone())
                },
                50 => default_strategy,
            ]
            .boxed()
        } else {
            default_strategy.boxed()
        }
    };

    match *param {
        DynSolType::Address => value()
            .prop_map(move |value| match value.as_address() {
                Some(addr) => {
                    if no_zksync_reserved_addresses {
                        DynSolValue::Address(foundry_zksync_core::to_safe_address(addr))
                    } else {
                        DynSolValue::Address(addr)
                    }
                }
                None => value,
            })
            .boxed(),
        DynSolType::Int(n @ 8..=256) => super::IntStrategy::new(n, fuzz_fixtures)
            .prop_map(move |x| DynSolValue::Int(x, n))
            .boxed(),
        DynSolType::Uint(n @ 8..=256) => super::UintStrategy::new(n, fuzz_fixtures)
            .prop_map(move |x| DynSolValue::Uint(x, n))
            .boxed(),
        DynSolType::Function | DynSolType::Bool => DynSolValue::type_strategy(param).boxed(),
        DynSolType::Bytes => value(),
        DynSolType::FixedBytes(_size @ 1..=32) => value(),
        DynSolType::String => value()
            .prop_map(move |value| {
                DynSolValue::String(
                    value.as_str().unwrap().trim().trim_end_matches('\0').to_string(),
                )
            })
            .boxed(),
        DynSolType::Tuple(ref params) => params
            .iter()
            .map(|param| fuzz_param_inner(param, None, no_zksync_reserved_addresses))
            .collect::<Vec<_>>()
            .prop_map(DynSolValue::Tuple)
            .boxed(),
        DynSolType::FixedArray(ref param, size) => proptest::collection::vec(
            fuzz_param_inner(param, None, no_zksync_reserved_addresses),
            size,
        )
        .prop_map(DynSolValue::FixedArray)
        .boxed(),
        DynSolType::Array(ref param) => proptest::collection::vec(
            fuzz_param_inner(param, None, no_zksync_reserved_addresses),
            0..MAX_ARRAY_LEN,
        )
        .prop_map(DynSolValue::Array)
        .boxed(),
        _ => panic!("unsupported fuzz param type: {param}"),
    }
}

/// Given a parameter type, returns a strategy for generating values for that type, given some EVM
/// fuzz state.
///
/// Works with ABI Encoder v2 tuples.
pub fn fuzz_param_from_state(
    param: &DynSolType,
    state: &EvmFuzzState,
) -> BoxedStrategy<DynSolValue> {
    let no_zksync_reserved_addresses = state.dictionary_read().no_zksync_reserved_addresses();

    // Value strategy that uses the state.
    let value = || {
        let state = state.clone();
        let param = param.clone();
        // Generate a bias and use it to pick samples or non-persistent values (50 / 50).
        // Use `Index` instead of `Selector` when selecting a value to avoid iterating over the
        // entire dictionary.
        any::<(bool, prop::sample::Index)>().prop_map(move |(bias, index)| {
            let state = state.dictionary_read();
            let values = if bias { state.samples(&param) } else { None }
                .unwrap_or_else(|| state.values())
                .as_slice();
            values[index.index(values.len())]
        })
    };

    // Convert the value based on the parameter type
    match *param {
        DynSolType::Address => {
            let deployed_libs = state.deployed_libs.clone();
            value()
                .prop_map(move |value| {
                    let mut fuzzed_addr = Address::from_word(value);
                    if !deployed_libs.contains(&fuzzed_addr) {
                        if no_zksync_reserved_addresses {
                            DynSolValue::Address(foundry_zksync_core::to_safe_address(fuzzed_addr))
                        } else {
                            DynSolValue::Address(fuzzed_addr)
                        }
                    } else {
                        let mut rng = StdRng::seed_from_u64(0x1337); // use deterministic rng

                        // Do not use addresses of deployed libraries as fuzz input, instead return
                        // a deterministically random address. We cannot filter out this value (via
                        // `prop_filter_map`) as proptest can invoke this closure after test
                        // execution, and returning a `None` will cause it to panic.
                        // See <https://github.com/foundry-rs/foundry/issues/9764> and <https://github.com/foundry-rs/foundry/issues/8639>.
                        loop {
                            fuzzed_addr.randomize_with(&mut rng);
                            if !deployed_libs.contains(&fuzzed_addr) {
                                break;
                            }
                        }
                        DynSolValue::Address(fuzzed_addr)
                    }
                })
                .boxed()
        }
        DynSolType::Function => value()
            .prop_map(move |value| {
                DynSolValue::Function(alloy_primitives::Function::from_word(value))
            })
            .boxed(),
        DynSolType::FixedBytes(size @ 1..=32) => value()
            .prop_map(move |mut v| {
                v[size..].fill(0);
                DynSolValue::FixedBytes(B256::from(v), size)
            })
            .boxed(),
        DynSolType::Bool => DynSolValue::type_strategy(param).boxed(),
        DynSolType::String => DynSolValue::type_strategy(param)
            .prop_map(move |value| {
                DynSolValue::String(
                    value.as_str().unwrap().trim().trim_end_matches('\0').to_string(),
                )
            })
            .boxed(),
        DynSolType::Bytes => {
            value().prop_map(move |value| DynSolValue::Bytes(value.0.into())).boxed()
        }
        DynSolType::Int(n @ 8..=256) => match n / 8 {
            32 => value()
                .prop_map(move |value| DynSolValue::Int(I256::from_raw(value.into()), 256))
                .boxed(),
            1..=31 => value()
                .prop_map(move |value| {
                    // Generate a uintN in the correct range, then shift it to the range of intN
                    // by subtracting 2^(N-1)
                    let uint = U256::from_be_bytes(value.0) % U256::from(1).wrapping_shl(n);
                    let max_int_plus1 = U256::from(1).wrapping_shl(n - 1);
                    let num = I256::from_raw(uint.wrapping_sub(max_int_plus1));
                    DynSolValue::Int(num, n)
                })
                .boxed(),
            _ => unreachable!(),
        },
        DynSolType::Uint(n @ 8..=256) => match n / 8 {
            32 => value()
                .prop_map(move |value| DynSolValue::Uint(U256::from_be_bytes(value.0), 256))
                .boxed(),
            1..=31 => value()
                .prop_map(move |value| {
                    let uint = U256::from_be_bytes(value.0) % U256::from(1).wrapping_shl(n);
                    DynSolValue::Uint(uint, n)
                })
                .boxed(),
            _ => unreachable!(),
        },
        DynSolType::Tuple(ref params) => params
            .iter()
            .map(|p| fuzz_param_from_state(p, state))
            .collect::<Vec<_>>()
            .prop_map(DynSolValue::Tuple)
            .boxed(),
        DynSolType::FixedArray(ref param, size) => {
            proptest::collection::vec(fuzz_param_from_state(param, state), size)
                .prop_map(DynSolValue::FixedArray)
                .boxed()
        }
        DynSolType::Array(ref param) => {
            proptest::collection::vec(fuzz_param_from_state(param, state), 0..MAX_ARRAY_LEN)
                .prop_map(DynSolValue::Array)
                .boxed()
        }
        _ => panic!("unsupported fuzz param type: {param}"),
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        FuzzFixtures,
        strategies::{EvmFuzzState, fuzz_calldata, fuzz_calldata_from_state},
    };
    use foundry_common::abi::get_func;
    use foundry_config::FuzzDictionaryConfig;
    use revm::database::{CacheDB, EmptyDB};

    #[test]
    fn can_fuzz_array() {
        let f = "testArray(uint64[2] calldata values)";
        let func = get_func(f).unwrap();
        let db = CacheDB::new(EmptyDB::default());
        let state = EvmFuzzState::new(&db, FuzzDictionaryConfig::default(), &[], false);
        let strategy = proptest::prop_oneof![
            60 => fuzz_calldata(func.clone(), &FuzzFixtures::default(), false),
            40 => fuzz_calldata_from_state(func, &state),
        ];
        let cfg = proptest::test_runner::Config { failure_persistence: None, ..Default::default() };
        let mut runner = proptest::test_runner::TestRunner::new(cfg);
        let _ = runner.run(&strategy, |_| Ok(()));
    }
}
