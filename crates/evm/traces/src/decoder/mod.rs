use crate::{
    CallTrace, CallTraceArena, CallTraceNode, DecodedCallData,
    debug::DebugTraceIdentifier,
    identifier::{IdentifiedAddress, LocalTraceIdentifier, SignaturesIdentifier, TraceIdentifier},
};
use alloy_dyn_abi::{DecodedEvent, DynSolValue, EventExt, FunctionExt, JsonAbiExt};
use alloy_json_abi::{Error, Event, Function, JsonAbi};
use alloy_primitives::{
    Address, B256, LogData, Selector,
    map::{HashMap, HashSet, hash_map::Entry},
};
use foundry_cheatcodes_spec::Vm;
use foundry_common::{
    ContractsByArtifact, SELECTOR_LEN, abi::get_indexed_event, fmt::format_token,
    get_contract_name, selectors::SelectorKind,
};
use foundry_config::zksync::ZKSYNC_ARTIFACTS_DIR;
use foundry_evm_core::{
    abi::console,
    constants::{
        CALLER, CHEATCODE_ADDRESS, DEFAULT_CREATE2_DEPLOYER, HARDHAT_CONSOLE_ADDRESS,
        TEST_CONTRACT_ADDRESS,
    },
    decode::RevertDecoder,
    precompiles::{
        BLAKE_2F, EC_ADD, EC_MUL, EC_PAIRING, EC_RECOVER, IDENTITY, MOD_EXP, POINT_EVALUATION,
        RIPEMD_160, SHA_256,
    },
};
use itertools::Itertools;
use revm_inspectors::tracing::types::{DecodedCallLog, DecodedCallTrace};
use std::{collections::BTreeMap, sync::OnceLock};

mod precompiles;

/// Build a new [CallTraceDecoder].
#[derive(Default)]
#[must_use = "builders do nothing unless you call `build` on them"]
pub struct CallTraceDecoderBuilder {
    decoder: CallTraceDecoder,
}

impl CallTraceDecoderBuilder {
    /// Create a new builder.
    #[inline]
    pub fn new() -> Self {
        Self { decoder: CallTraceDecoder::new().clone() }
    }

    /// Add known labels to the decoder.
    #[inline]
    pub fn with_labels(mut self, labels: impl IntoIterator<Item = (Address, String)>) -> Self {
        self.decoder.labels.extend(labels);
        self
    }

    /// Add known errors to the decoder.
    #[inline]
    pub fn with_abi(mut self, abi: &JsonAbi) -> Self {
        self.decoder.collect_abi(abi, None);
        self
    }

    /// Add known contracts to the decoder.
    #[inline]
    pub fn with_known_contracts(mut self, contracts: &ContractsByArtifact) -> Self {
        trace!(target: "evm::traces", len=contracts.len(), "collecting known contract ABIs");
        for contract in contracts.values() {
            self.decoder.collect_abi(&contract.abi, None);
        }
        self
    }

    /// Add known contracts to the decoder from a `LocalTraceIdentifier`.
    #[inline]
    pub fn with_local_identifier_abis(self, identifier: &LocalTraceIdentifier<'_>) -> Self {
        self.with_known_contracts(identifier.contracts())
    }

    /// Sets the verbosity level of the decoder.
    #[inline]
    pub fn with_verbosity(mut self, level: u8) -> Self {
        self.decoder.verbosity = level;
        self
    }

    /// Sets the signature identifier for events and functions.
    #[inline]
    pub fn with_signature_identifier(mut self, identifier: SignaturesIdentifier) -> Self {
        self.decoder.signature_identifier = Some(identifier);
        self
    }

    /// Sets the signature identifier for events and functions.
    #[inline]
    pub fn with_label_disabled(mut self, disable_alias: bool) -> Self {
        self.decoder.disable_labels = disable_alias;
        self
    }

    /// Sets the debug identifier for the decoder.
    #[inline]
    pub fn with_debug_identifier(mut self, identifier: DebugTraceIdentifier) -> Self {
        self.decoder.debug_identifier = Some(identifier);
        self
    }

    /// Build the decoder.
    #[inline]
    pub fn build(self) -> CallTraceDecoder {
        self.decoder
    }
}

/// The call trace decoder.
///
/// The decoder collects address labels and ABIs from any number of [TraceIdentifier]s, which it
/// then uses to decode the call trace.
///
/// Note that a call trace decoder is required for each new set of traces, since addresses in
/// different sets might overlap.
#[derive(Clone, Debug, Default)]
pub struct CallTraceDecoder {
    /// Addresses identified to be a specific contract.
    ///
    /// The values are in the form `"<artifact>:<contract>"`.
    pub contracts: HashMap<Address, String>,
    /// Address labels.
    pub labels: HashMap<Address, String>,
    /// Contract addresses that have a receive function.
    pub receive_contracts: HashSet<Address>,
    /// Contract addresses that have fallback functions, mapped to function selectors of that
    /// contract.
    pub fallback_contracts: HashMap<Address, HashSet<Selector>>,
    /// Contract addresses that have do NOT have fallback functions, mapped to function selectors
    /// of that contract.
    pub non_fallback_contracts: HashMap<Address, HashSet<Selector>>,

    /// All known functions.
    pub functions: HashMap<Selector, Vec<Function>>,
    /// All known events.
    ///
    /// Key is: `(topics[0], topics.len() - 1)`.
    pub events: BTreeMap<(B256, usize), Vec<Event>>,
    /// Revert decoder. Contains all known custom errors.
    pub revert_decoder: RevertDecoder,

    /// A signature identifier for events and functions.
    pub signature_identifier: Option<SignaturesIdentifier>,
    /// Verbosity level
    pub verbosity: u8,

    /// Optional identifier of individual trace steps.
    pub debug_identifier: Option<DebugTraceIdentifier>,

    /// Disable showing of labels.
    pub disable_labels: bool,

    /// Addresses that are contracts on the ZkVm
    pub zk_contracts: HashSet<Address>,
}

impl CallTraceDecoder {
    /// Creates a new call trace decoder.
    ///
    /// The call trace decoder always knows how to decode calls to the cheatcode address, as well
    /// as DSTest-style logs.
    pub fn new() -> &'static Self {
        // If you want to take arguments in this function, assign them to the fields of the cloned
        // lazy instead of removing it
        static INIT: OnceLock<CallTraceDecoder> = OnceLock::new();
        INIT.get_or_init(Self::init)
    }

    fn init() -> Self {
        Self {
            contracts: Default::default(),
            labels: HashMap::from_iter([
                (CHEATCODE_ADDRESS, "VM".to_string()),
                (HARDHAT_CONSOLE_ADDRESS, "console".to_string()),
                (DEFAULT_CREATE2_DEPLOYER, "Create2Deployer".to_string()),
                (CALLER, "DefaultSender".to_string()),
                (TEST_CONTRACT_ADDRESS, "DefaultTestContract".to_string()),
                (EC_RECOVER, "ECRecover".to_string()),
                (SHA_256, "SHA-256".to_string()),
                (RIPEMD_160, "RIPEMD-160".to_string()),
                (IDENTITY, "Identity".to_string()),
                (MOD_EXP, "ModExp".to_string()),
                (EC_ADD, "ECAdd".to_string()),
                (EC_MUL, "ECMul".to_string()),
                (EC_PAIRING, "ECPairing".to_string()),
                (BLAKE_2F, "Blake2F".to_string()),
                (POINT_EVALUATION, "PointEvaluation".to_string()),
            ]),
            receive_contracts: Default::default(),
            fallback_contracts: Default::default(),
            non_fallback_contracts: Default::default(),

            functions: console::hh::abi::functions()
                .into_values()
                .chain(Vm::abi::functions().into_values())
                .flatten()
                .map(|func| (func.selector(), vec![func]))
                .collect(),
            events: console::ds::abi::events()
                .into_values()
                .flatten()
                .map(|event| ((event.selector(), indexed_inputs(&event)), vec![event]))
                .collect(),
            revert_decoder: Default::default(),

            signature_identifier: None,
            verbosity: 0,

            debug_identifier: None,

            disable_labels: false,

            zk_contracts: Default::default(),
        }
    }

    /// Clears all known addresses.
    pub fn clear_addresses(&mut self) {
        self.contracts.clear();

        let default_labels = &Self::new().labels;
        if self.labels.len() > default_labels.len() {
            self.labels.clone_from(default_labels);
        }

        self.receive_contracts.clear();
        self.fallback_contracts.clear();
    }

    /// Identify unknown addresses in the specified call trace using the specified identifier.
    ///
    /// Unknown contracts are contracts that either lack a label or an ABI.
    pub fn identify(&mut self, arena: &CallTraceArena, identifier: &mut impl TraceIdentifier) {
        self.collect_identified_addresses(self.identify_addresses(arena, identifier));
    }

    /// Identify unknown addresses in the specified call trace using the specified identifier.
    ///
    /// Unknown contracts are contracts that either lack a label or an ABI.
    pub fn identify_addresses<'a>(
        &self,
        arena: &CallTraceArena,
        identifier: &'a mut impl TraceIdentifier,
    ) -> Vec<IdentifiedAddress<'a>> {
        let nodes = arena.nodes().iter().filter(|node| {
            let address = &node.trace.address;
            !self.labels.contains_key(address) || !self.contracts.contains_key(address)
        });
        identifier.identify_addresses(&nodes.collect::<Vec<_>>())
    }

    /// Adds a single event to the decoder.
    pub fn push_event(&mut self, event: Event) {
        self.events.entry((event.selector(), indexed_inputs(&event))).or_default().push(event);
    }

    /// Adds a single function to the decoder.
    pub fn push_function(&mut self, function: Function) {
        match self.functions.entry(function.selector()) {
            Entry::Occupied(entry) => {
                // This shouldn't happen that often.
                if entry.get().contains(&function) {
                    return;
                }
                trace!(target: "evm::traces", selector=%entry.key(), new=%function.signature(), "duplicate function selector");
                entry.into_mut().push(function);
            }
            Entry::Vacant(entry) => {
                entry.insert(vec![function]);
            }
        }
    }

    /// Adds a single error to the decoder.
    pub fn push_error(&mut self, error: Error) {
        self.revert_decoder.push_error(error);
    }

    pub fn without_label(&mut self, disable: bool) {
        self.disable_labels = disable;
    }

    fn collect_identified_addresses(&mut self, mut addrs: Vec<IdentifiedAddress<'_>>) {
        addrs.sort_by_key(|identity| identity.address);
        addrs.dedup_by_key(|identity| identity.address);
        if addrs.is_empty() {
            return;
        }

        trace!(target: "evm::traces", len=addrs.len(), "collecting address identities");
        for IdentifiedAddress { address, label, contract, abi, artifact_id } in addrs {
            let _span = trace_span!(target: "evm::traces", "identity", ?contract, ?label).entered();

            if let Some(contract) = contract {
                self.contracts.entry(address).or_insert(contract);
            }

            if let Some(label) = label {
                self.labels.entry(address).or_insert(label);
            }

            if let Some(abi) = abi {
                self.collect_abi(&abi, Some(address));
            }

            if let Some(artifact_id) = artifact_id {
                if artifact_id.path.to_string_lossy().contains(ZKSYNC_ARTIFACTS_DIR) {
                    self.zk_contracts.insert(address);
                }
            }
        }
    }

    fn collect_abi(&mut self, abi: &JsonAbi, address: Option<Address>) {
        let len = abi.len();
        if len == 0 {
            return;
        }
        trace!(target: "evm::traces", len, ?address, "collecting ABI");
        for function in abi.functions() {
            self.push_function(function.clone());
        }
        for event in abi.events() {
            self.push_event(event.clone());
        }
        for error in abi.errors() {
            self.push_error(error.clone());
        }
        if let Some(address) = address {
            if abi.receive.is_some() {
                self.receive_contracts.insert(address);
            }

            if abi.fallback.is_some() {
                self.fallback_contracts
                    .insert(address, abi.functions().map(|f| f.selector()).collect());
            } else {
                self.non_fallback_contracts
                    .insert(address, abi.functions().map(|f| f.selector()).collect());
            }
        }
    }

    /// Populates the traces with decoded data by mutating the
    /// [CallTrace] in place. See [CallTraceDecoder::decode_function] and
    /// [CallTraceDecoder::decode_event] for more details.
    pub async fn populate_traces(&self, traces: &mut Vec<CallTraceNode>) {
        for node in traces {
            node.trace.decoded = self.decode_function(&node.trace).await;
            for log in &mut node.logs {
                log.decoded = self.decode_event(&log.raw_log).await;
            }

            if let Some(debug) = self.debug_identifier.as_ref()
                && let Some(identified) = self.contracts.get(&node.trace.address)
            {
                debug.identify_node_steps(node, get_contract_name(identified))
            }
        }
    }

    /// Decodes a call trace.
    pub async fn decode_function(&self, trace: &CallTrace) -> DecodedCallTrace {
        let label =
            if self.disable_labels { None } else { self.labels.get(&trace.address).cloned() };

        if trace.kind.is_any_create() {
            return DecodedCallTrace { label, ..Default::default() };
        }

        if let Some(trace) = precompiles::decode(trace, 1) {
            return trace;
        }

        let cdata = &trace.data;
        if trace.address == DEFAULT_CREATE2_DEPLOYER {
            return DecodedCallTrace {
                label,
                call_data: Some(DecodedCallData { signature: "create2".to_string(), args: vec![] }),
                return_data: self.default_return_data(trace),
            };
        }

        if is_abi_call_data(cdata) {
            let selector = Selector::try_from(&cdata[..SELECTOR_LEN]).unwrap();
            let mut functions = Vec::new();
            let functions = match self.functions.get(&selector) {
                Some(fs) => fs,
                None => {
                    if let Some(identifier) = &self.signature_identifier
                        && let Some(function) = identifier.identify_function(selector).await
                    {
                        functions.push(function);
                    }
                    &functions
                }
            };

            // Check if unsupported fn selector: calldata dooes NOT point to one of its selectors +
            // non-fallback contract + no receive
            if let Some(contract_selectors) = self.non_fallback_contracts.get(&trace.address)
                && !contract_selectors.contains(&selector)
                && (!cdata.is_empty() || !self.receive_contracts.contains(&trace.address))
            {
                let return_data = if !trace.success {
                    let revert_msg = self.revert_decoder.decode(&trace.output, Some(trace.status));

                    if trace.output.is_empty() || revert_msg.contains("EvmError: Revert") {
                        Some(format!(
                            "unrecognized function selector {} for contract {}, which has no fallback function.",
                            selector, trace.address
                        ))
                    } else {
                        Some(revert_msg)
                    }
                } else {
                    None
                };

                if let Some(func) = functions.first() {
                    return DecodedCallTrace {
                        label,
                        call_data: Some(self.decode_function_input(trace, func)),
                        return_data,
                    };
                } else {
                    return DecodedCallTrace {
                        label,
                        call_data: self.fallback_call_data(trace),
                        return_data,
                    };
                };
            }

            let [func, ..] = &functions[..] else {
                return DecodedCallTrace {
                    label,
                    call_data: self.fallback_call_data(trace),
                    return_data: self.default_return_data(trace),
                };
            };

            // If traced contract is a fallback contract, check if it has the decoded function.
            // If not, then replace call data signature with `fallback`.
            let mut call_data = self.decode_function_input(trace, func);
            if let Some(fallback_functions) = self.fallback_contracts.get(&trace.address)
                && !fallback_functions.contains(&selector)
                && let Some(cd) = self.fallback_call_data(trace)
            {
                call_data.signature = cd.signature;
            }

            DecodedCallTrace {
                label,
                call_data: Some(call_data),
                return_data: self.decode_function_output(trace, functions),
            }
        } else {
            DecodedCallTrace {
                label,
                call_data: self.fallback_call_data(trace),
                return_data: self.default_return_data(trace),
            }
        }
    }

    /// Decodes a function's input into the given trace.
    fn decode_function_input(&self, trace: &CallTrace, func: &Function) -> DecodedCallData {
        let mut args = None;
        if trace.data.len() >= SELECTOR_LEN {
            if trace.address == CHEATCODE_ADDRESS {
                // Try to decode cheatcode inputs in a more custom way
                if let Some(v) = self.decode_cheatcode_inputs(func, &trace.data) {
                    args = Some(v);
                }
            }

            if args.is_none()
                && let Ok(v) = func.abi_decode_input(&trace.data[SELECTOR_LEN..])
            {
                args = Some(v.iter().map(|value| self.format_value(value)).collect());
            }
        }

        DecodedCallData { signature: func.signature(), args: args.unwrap_or_default() }
    }

    /// Custom decoding for cheatcode inputs.
    fn decode_cheatcode_inputs(&self, func: &Function, data: &[u8]) -> Option<Vec<String>> {
        match func.name.as_str() {
            "expectRevert" => Some(vec![self.revert_decoder.decode(data, None)]),
            "addr" | "createWallet" | "deriveKey" | "rememberKey" => {
                // Redact private key in all cases
                Some(vec!["<pk>".to_string()])
            }
            "broadcast" | "startBroadcast" => {
                // Redact private key if defined
                // broadcast(uint256) / startBroadcast(uint256)
                if !func.inputs.is_empty() && func.inputs[0].ty == "uint256" {
                    Some(vec!["<pk>".to_string()])
                } else {
                    None
                }
            }
            "getNonce" => {
                // Redact private key if defined
                // getNonce(Wallet)
                if !func.inputs.is_empty() && func.inputs[0].ty == "tuple" {
                    Some(vec!["<pk>".to_string()])
                } else {
                    None
                }
            }
            "sign" | "signP256" => {
                let mut decoded = func.abi_decode_input(&data[SELECTOR_LEN..]).ok()?;

                // Redact private key and replace in trace
                // sign(uint256,bytes32) / signP256(uint256,bytes32) / sign(Wallet,bytes32)
                if !decoded.is_empty() &&
                    (func.inputs[0].ty == "uint256" || func.inputs[0].ty == "tuple")
                {
                    decoded[0] = DynSolValue::String("<pk>".to_string());
                }

                Some(decoded.iter().map(format_token).collect())
            }
            "signDelegation" | "signAndAttachDelegation" => {
                let mut decoded = func.abi_decode_input(&data[SELECTOR_LEN..]).ok()?;
                // Redact private key and replace in trace for
                // signAndAttachDelegation(address implementation, uint256 privateKey)
                // signDelegation(address implementation, uint256 privateKey)
                decoded[1] = DynSolValue::String("<pk>".to_string());
                Some(decoded.iter().map(format_token).collect())
            }
            "parseJson" |
            "parseJsonUint" |
            "parseJsonUintArray" |
            "parseJsonInt" |
            "parseJsonIntArray" |
            "parseJsonString" |
            "parseJsonStringArray" |
            "parseJsonAddress" |
            "parseJsonAddressArray" |
            "parseJsonBool" |
            "parseJsonBoolArray" |
            "parseJsonBytes" |
            "parseJsonBytesArray" |
            "parseJsonBytes32" |
            "parseJsonBytes32Array" |
            "writeJson" |
            // `keyExists` is being deprecated in favor of `keyExistsJson`. It will be removed in future versions.
            "keyExists" |
            "keyExistsJson" |
            "serializeBool" |
            "serializeUint" |
            "serializeUintToHex" |
            "serializeInt" |
            "serializeAddress" |
            "serializeBytes32" |
            "serializeString" |
            "serializeBytes" => {
                if self.verbosity >= 5 {
                    None
                } else {
                    let mut decoded = func.abi_decode_input(&data[SELECTOR_LEN..]).ok()?;
                    let token = if func.name.as_str() == "parseJson" ||
                        // `keyExists` is being deprecated in favor of `keyExistsJson`. It will be removed in future versions.
                        func.name.as_str() == "keyExists" ||
                        func.name.as_str() == "keyExistsJson"
                    {
                        "<JSON file>"
                    } else {
                        "<stringified JSON>"
                    };
                    decoded[0] = DynSolValue::String(token.to_string());
                    Some(decoded.iter().map(format_token).collect())
                }
            }
            s if s.contains("Toml") => {
                if self.verbosity >= 5 {
                    None
                } else {
                    let mut decoded = func.abi_decode_input(&data[SELECTOR_LEN..]).ok()?;
                    let token = if func.name.as_str() == "parseToml" ||
                        func.name.as_str() == "keyExistsToml"
                    {
                        "<TOML file>"
                    } else {
                        "<stringified TOML>"
                    };
                    decoded[0] = DynSolValue::String(token.to_string());
                    Some(decoded.iter().map(format_token).collect())
                }
            }
            "createFork" |
            "createSelectFork" |
            "rpc" => {
                let mut decoded = func.abi_decode_input(&data[SELECTOR_LEN..]).ok()?;

                // Redact RPC URL except if referenced by an alias
                if !decoded.is_empty() && func.inputs[0].ty == "string" {
                    let url_or_alias = decoded[0].as_str().unwrap_or_default();

                    if url_or_alias.starts_with("http") || url_or_alias.starts_with("ws") {
                        decoded[0] = DynSolValue::String("<rpc url>".to_string());
                    }
                } else {
                    return None;
                }

                Some(decoded.iter().map(format_token).collect())
            }
            _ => None,
        }
    }

    /// Decodes a function's output into the given trace.
    fn decode_function_output(&self, trace: &CallTrace, funcs: &[Function]) -> Option<String> {
        if !trace.success {
            return self.default_return_data(trace);
        }

        if trace.address == CHEATCODE_ADDRESS
            && let Some(decoded) = funcs.iter().find_map(|func| self.decode_cheatcode_outputs(func))
        {
            return Some(decoded);
        }

        if let Some(values) =
            funcs.iter().find_map(|func| func.abi_decode_output(&trace.output).ok())
        {
            // Functions coming from an external database do not have any outputs specified,
            // and will lead to returning an empty list of values.
            if values.is_empty() {
                return None;
            }

            return Some(
                values.iter().map(|value| self.format_value(value)).format(", ").to_string(),
            );
        }

        None
    }

    /// Custom decoding for cheatcode outputs.
    fn decode_cheatcode_outputs(&self, func: &Function) -> Option<String> {
        match func.name.as_str() {
            s if s.starts_with("env") => Some("<env var value>"),
            "createWallet" | "deriveKey" => Some("<pk>"),
            "promptSecret" | "promptSecretUint" => Some("<secret>"),
            "parseJson" if self.verbosity < 5 => Some("<encoded JSON value>"),
            "readFile" if self.verbosity < 5 => Some("<file>"),
            "rpcUrl" | "rpcUrls" | "rpcUrlStructs" => Some("<rpc url>"),
            _ => None,
        }
        .map(Into::into)
    }

    #[track_caller]
    fn fallback_call_data(&self, trace: &CallTrace) -> Option<DecodedCallData> {
        let cdata = &trace.data;
        let signature = if cdata.is_empty() && self.receive_contracts.contains(&trace.address) {
            "receive()"
        } else if self.fallback_contracts.contains_key(&trace.address) {
            "fallback()"
        } else {
            return None;
        }
        .to_string();
        let args = if cdata.is_empty() { Vec::new() } else { vec![cdata.to_string()] };
        Some(DecodedCallData { signature, args })
    }

    /// The default decoded return data for a trace.
    fn default_return_data(&self, trace: &CallTrace) -> Option<String> {
        (!trace.success).then(|| self.revert_decoder.decode(&trace.output, Some(trace.status)))
    }

    /// Decodes an event.
    pub async fn decode_event(&self, log: &LogData) -> DecodedCallLog {
        let &[t0, ..] = log.topics() else { return DecodedCallLog { name: None, params: None } };

        let mut events = Vec::new();
        let events = match self.events.get(&(t0, log.topics().len() - 1)) {
            Some(es) => es,
            None => {
                if let Some(identifier) = &self.signature_identifier
                    && let Some(event) = identifier.identify_event(t0).await
                {
                    events.push(get_indexed_event(event, log));
                }
                &events
            }
        };
        for event in events {
            if let Ok(decoded) = event.decode_log(log) {
                let params = reconstruct_params(event, &decoded);
                return DecodedCallLog {
                    name: Some(event.name.clone()),
                    params: Some(
                        params
                            .into_iter()
                            .zip(event.inputs.iter())
                            .map(|(param, input)| {
                                // undo patched names
                                let name = input.name.clone();
                                (name, self.format_value(&param))
                            })
                            .collect(),
                    ),
                };
            }
        }

        DecodedCallLog { name: None, params: None }
    }

    /// Prefetches function and event signatures into the identifier cache
    pub async fn prefetch_signatures(&self, nodes: &[CallTraceNode]) {
        let Some(identifier) = &self.signature_identifier else { return };
        let events = nodes
            .iter()
            .flat_map(|node| {
                node.logs
                    .iter()
                    .map(|log| log.raw_log.topics())
                    .filter(|&topics| {
                        if let Some(&first) = topics.first()
                            && self.events.contains_key(&(first, topics.len() - 1))
                        {
                            return false;
                        }
                        true
                    })
                    .filter_map(|topics| topics.first())
            })
            .copied();
        let functions = nodes
            .iter()
            .filter(|&n| {
                // Ignore known addresses.
                if n.trace.address == DEFAULT_CREATE2_DEPLOYER
                    || n.is_precompile()
                    || precompiles::is_known_precompile(n.trace.address, 1)
                {
                    return false;
                }
                // Ignore non-ABI calldata.
                if n.trace.kind.is_any_create() || !is_abi_call_data(&n.trace.data) {
                    return false;
                }
                true
            })
            .filter_map(|n| n.trace.data.first_chunk().map(Selector::from))
            .filter(|selector| !self.functions.contains_key(selector));
        let selectors = events
            .map(SelectorKind::Event)
            .chain(functions.map(SelectorKind::Function))
            .unique()
            .collect::<Vec<_>>();
        let _ = identifier.identify(&selectors).await;
    }

    /// Pretty-prints a value.
    fn format_value(&self, value: &DynSolValue) -> String {
        if let DynSolValue::Address(addr) = value
            && let Some(label) = self.labels.get(addr)
        {
            return format!("{label}: [{addr}]");
        }
        format_token(value)
    }
}

/// Returns `true` if the given function calldata (including function selector) is ABI-encoded.
///
/// This is a simple heuristic to avoid fetching non ABI-encoded selectors.
fn is_abi_call_data(data: &[u8]) -> bool {
    match data.len().cmp(&SELECTOR_LEN) {
        std::cmp::Ordering::Less => false,
        std::cmp::Ordering::Equal => true,
        std::cmp::Ordering::Greater => is_abi_data(&data[SELECTOR_LEN..]),
    }
}

/// Returns `true` if the given data is ABI-encoded.
///
/// See [`is_abi_call_data`] for more details.
fn is_abi_data(data: &[u8]) -> bool {
    let rem = data.len() % 32;
    if rem == 0 || data.is_empty() {
        return true;
    }
    // If the length is not a multiple of 32, also accept when the last remainder bytes are all 0.
    data[data.len() - rem..].iter().all(|byte| *byte == 0)
}

/// Restore the order of the params of a decoded event,
/// as Alloy returns the indexed and unindexed params separately.
fn reconstruct_params(event: &Event, decoded: &DecodedEvent) -> Vec<DynSolValue> {
    let mut indexed = 0;
    let mut unindexed = 0;
    let mut inputs = vec![];
    for input in &event.inputs {
        // Prevent panic of event `Transfer(from, to)` decoded with a signature
        // `Transfer(address indexed from, address indexed to, uint256 indexed tokenId)` by making
        // sure the event inputs is not higher than decoded indexed / un-indexed values.
        if input.indexed && indexed < decoded.indexed.len() {
            inputs.push(decoded.indexed[indexed].clone());
            indexed += 1;
        } else if unindexed < decoded.body.len() {
            inputs.push(decoded.body[unindexed].clone());
            unindexed += 1;
        }
    }

    inputs
}

fn indexed_inputs(event: &Event) -> usize {
    event.inputs.iter().filter(|param| param.indexed).count()
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_primitives::hex;

    #[test]
    fn test_should_redact() {
        let decoder = CallTraceDecoder::new();

        // [function_signature, data, expected]
        let cheatcode_input_test_cases = vec![
            // Should redact private key from traces in all cases:
            ("addr(uint256)", vec![], Some(vec!["<pk>".to_string()])),
            ("createWallet(string)", vec![], Some(vec!["<pk>".to_string()])),
            ("createWallet(uint256)", vec![], Some(vec!["<pk>".to_string()])),
            ("deriveKey(string,uint32)", vec![], Some(vec!["<pk>".to_string()])),
            ("deriveKey(string,string,uint32)", vec![], Some(vec!["<pk>".to_string()])),
            ("deriveKey(string,uint32,string)", vec![], Some(vec!["<pk>".to_string()])),
            ("deriveKey(string,string,uint32,string)", vec![], Some(vec!["<pk>".to_string()])),
            ("rememberKey(uint256)", vec![], Some(vec!["<pk>".to_string()])),
            //
            // Should redact private key from traces in specific cases with exceptions:
            ("broadcast(uint256)", vec![], Some(vec!["<pk>".to_string()])),
            ("broadcast()", vec![], None), // Ignore: `private key` is not passed.
            ("startBroadcast(uint256)", vec![], Some(vec!["<pk>".to_string()])),
            ("startBroadcast()", vec![], None), // Ignore: `private key` is not passed.
            ("getNonce((address,uint256,uint256,uint256))", vec![], Some(vec!["<pk>".to_string()])),
            ("getNonce(address)", vec![], None), // Ignore: `address` is public.
            //
            // Should redact private key and replace in trace in cases:
            (
                "sign(uint256,bytes32)",
                hex!(
                    "
                    e341eaa4
                    7c852118294e51e653712a81e05800f419141751be58f605c371e15141b007a6
                    0000000000000000000000000000000000000000000000000000000000000000
                "
                )
                .to_vec(),
                Some(vec![
                    "\"<pk>\"".to_string(),
                    "0x0000000000000000000000000000000000000000000000000000000000000000"
                        .to_string(),
                ]),
            ),
            (
                "signP256(uint256,bytes32)",
                hex!(
                    "
                    83211b40
                    7c852118294e51e653712a81e05800f419141751be58f605c371e15141b007a6
                    0000000000000000000000000000000000000000000000000000000000000000
                "
                )
                .to_vec(),
                Some(vec![
                    "\"<pk>\"".to_string(),
                    "0x0000000000000000000000000000000000000000000000000000000000000000"
                        .to_string(),
                ]),
            ),
            (
                // cast calldata "createFork(string)" "https://eth-mainnet.g.alchemy.com/v2/api_key"
                "createFork(string)",
                hex!(
                    "
                    31ba3498
                    0000000000000000000000000000000000000000000000000000000000000020
                    000000000000000000000000000000000000000000000000000000000000002c
                    68747470733a2f2f6574682d6d61696e6e65742e672e616c6368656d792e636f
                    6d2f76322f6170695f6b65790000000000000000000000000000000000000000
                    "
                )
                .to_vec(),
                Some(vec!["\"<rpc url>\"".to_string()]),
            ),
            (
                // cast calldata "createFork(string)" "wss://eth-mainnet.g.alchemy.com/v2/api_key"
                "createFork(string)",
                hex!(
                    "
                    31ba3498
                    0000000000000000000000000000000000000000000000000000000000000020
                    000000000000000000000000000000000000000000000000000000000000002a
                    7773733a2f2f6574682d6d61696e6e65742e672e616c6368656d792e636f6d2f
                    76322f6170695f6b657900000000000000000000000000000000000000000000
                    "
                )
                .to_vec(),
                Some(vec!["\"<rpc url>\"".to_string()]),
            ),
            (
                // cast calldata "createFork(string)" "mainnet"
                "createFork(string)",
                hex!(
                    "
                    31ba3498
                    0000000000000000000000000000000000000000000000000000000000000020
                    0000000000000000000000000000000000000000000000000000000000000007
                    6d61696e6e657400000000000000000000000000000000000000000000000000
                    "
                )
                .to_vec(),
                Some(vec!["\"mainnet\"".to_string()]),
            ),
            (
                // cast calldata "createFork(string,uint256)" "https://eth-mainnet.g.alchemy.com/v2/api_key" 1
                "createFork(string,uint256)",
                hex!(
                    "
                    6ba3ba2b
                    0000000000000000000000000000000000000000000000000000000000000040
                    0000000000000000000000000000000000000000000000000000000000000001
                    000000000000000000000000000000000000000000000000000000000000002c
                    68747470733a2f2f6574682d6d61696e6e65742e672e616c6368656d792e636f
                    6d2f76322f6170695f6b65790000000000000000000000000000000000000000
                "
                )
                .to_vec(),
                Some(vec!["\"<rpc url>\"".to_string(), "1".to_string()]),
            ),
            (
                // cast calldata "createFork(string,uint256)" "mainnet" 1
                "createFork(string,uint256)",
                hex!(
                    "
                    6ba3ba2b
                    0000000000000000000000000000000000000000000000000000000000000040
                    0000000000000000000000000000000000000000000000000000000000000001
                    0000000000000000000000000000000000000000000000000000000000000007
                    6d61696e6e657400000000000000000000000000000000000000000000000000
                "
                )
                .to_vec(),
                Some(vec!["\"mainnet\"".to_string(), "1".to_string()]),
            ),
            (
                // cast calldata "createFork(string,bytes32)" "https://eth-mainnet.g.alchemy.com/v2/api_key" 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff
                "createFork(string,bytes32)",
                hex!(
                    "
                    7ca29682
                    0000000000000000000000000000000000000000000000000000000000000040
                    ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff
                    000000000000000000000000000000000000000000000000000000000000002c
                    68747470733a2f2f6574682d6d61696e6e65742e672e616c6368656d792e636f
                    6d2f76322f6170695f6b65790000000000000000000000000000000000000000
                "
                )
                .to_vec(),
                Some(vec![
                    "\"<rpc url>\"".to_string(),
                    "0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff"
                        .to_string(),
                ]),
            ),
            (
                // cast calldata "createFork(string,bytes32)" "mainnet"
                // 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff
                "createFork(string,bytes32)",
                hex!(
                    "
                    7ca29682
                    0000000000000000000000000000000000000000000000000000000000000040
                    ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff
                    0000000000000000000000000000000000000000000000000000000000000007
                    6d61696e6e657400000000000000000000000000000000000000000000000000
                "
                )
                .to_vec(),
                Some(vec![
                    "\"mainnet\"".to_string(),
                    "0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff"
                        .to_string(),
                ]),
            ),
            (
                // cast calldata "createSelectFork(string)" "https://eth-mainnet.g.alchemy.com/v2/api_key"
                "createSelectFork(string)",
                hex!(
                    "
                    98680034
                    0000000000000000000000000000000000000000000000000000000000000020
                    000000000000000000000000000000000000000000000000000000000000002c
                    68747470733a2f2f6574682d6d61696e6e65742e672e616c6368656d792e636f
                    6d2f76322f6170695f6b65790000000000000000000000000000000000000000
                    "
                )
                .to_vec(),
                Some(vec!["\"<rpc url>\"".to_string()]),
            ),
            (
                // cast calldata "createSelectFork(string)" "mainnet"
                "createSelectFork(string)",
                hex!(
                    "
                    98680034
                    0000000000000000000000000000000000000000000000000000000000000020
                    0000000000000000000000000000000000000000000000000000000000000007
                    6d61696e6e657400000000000000000000000000000000000000000000000000
                    "
                )
                .to_vec(),
                Some(vec!["\"mainnet\"".to_string()]),
            ),
            (
                // cast calldata "createSelectFork(string,uint256)" "https://eth-mainnet.g.alchemy.com/v2/api_key" 1
                "createSelectFork(string,uint256)",
                hex!(
                    "
                    71ee464d
                    0000000000000000000000000000000000000000000000000000000000000040
                    0000000000000000000000000000000000000000000000000000000000000001
                    000000000000000000000000000000000000000000000000000000000000002c
                    68747470733a2f2f6574682d6d61696e6e65742e672e616c6368656d792e636f
                    6d2f76322f6170695f6b65790000000000000000000000000000000000000000
                "
                )
                .to_vec(),
                Some(vec!["\"<rpc url>\"".to_string(), "1".to_string()]),
            ),
            (
                // cast calldata "createSelectFork(string,uint256)" "mainnet" 1
                "createSelectFork(string,uint256)",
                hex!(
                    "
                    71ee464d
                    0000000000000000000000000000000000000000000000000000000000000040
                    0000000000000000000000000000000000000000000000000000000000000001
                    0000000000000000000000000000000000000000000000000000000000000007
                    6d61696e6e657400000000000000000000000000000000000000000000000000
                "
                )
                .to_vec(),
                Some(vec!["\"mainnet\"".to_string(), "1".to_string()]),
            ),
            (
                // cast calldata "createSelectFork(string,bytes32)" "https://eth-mainnet.g.alchemy.com/v2/api_key" 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff
                "createSelectFork(string,bytes32)",
                hex!(
                    "
                    84d52b7a
                    0000000000000000000000000000000000000000000000000000000000000040
                    ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff
                    000000000000000000000000000000000000000000000000000000000000002c
                    68747470733a2f2f6574682d6d61696e6e65742e672e616c6368656d792e636f
                    6d2f76322f6170695f6b65790000000000000000000000000000000000000000
                "
                )
                .to_vec(),
                Some(vec![
                    "\"<rpc url>\"".to_string(),
                    "0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff"
                        .to_string(),
                ]),
            ),
            (
                // cast calldata "createSelectFork(string,bytes32)" "mainnet"
                // 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff
                "createSelectFork(string,bytes32)",
                hex!(
                    "
                    84d52b7a
                    0000000000000000000000000000000000000000000000000000000000000040
                    ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff
                    0000000000000000000000000000000000000000000000000000000000000007
                    6d61696e6e657400000000000000000000000000000000000000000000000000
                "
                )
                .to_vec(),
                Some(vec![
                    "\"mainnet\"".to_string(),
                    "0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff"
                        .to_string(),
                ]),
            ),
            (
                // cast calldata "rpc(string,string,string)" "https://eth-mainnet.g.alchemy.com/v2/api_key" "eth_getBalance" "[\"0x551e7784778ef8e048e495df49f2614f84a4f1dc\",\"0x0\"]"
                "rpc(string,string,string)",
                hex!(
                    "
                    0199a220
                    0000000000000000000000000000000000000000000000000000000000000060
                    00000000000000000000000000000000000000000000000000000000000000c0
                    0000000000000000000000000000000000000000000000000000000000000100
                    000000000000000000000000000000000000000000000000000000000000002c
                    68747470733a2f2f6574682d6d61696e6e65742e672e616c6368656d792e636f
                    6d2f76322f6170695f6b65790000000000000000000000000000000000000000
                    000000000000000000000000000000000000000000000000000000000000000e
                    6574685f67657442616c616e6365000000000000000000000000000000000000
                    0000000000000000000000000000000000000000000000000000000000000034
                    5b22307835353165373738343737386566386530343865343935646634396632
                    363134663834613466316463222c22307830225d000000000000000000000000
                "
                )
                .to_vec(),
                Some(vec![
                    "\"<rpc url>\"".to_string(),
                    "\"eth_getBalance\"".to_string(),
                    "\"[\\\"0x551e7784778ef8e048e495df49f2614f84a4f1dc\\\",\\\"0x0\\\"]\""
                        .to_string(),
                ]),
            ),
            (
                // cast calldata "rpc(string,string,string)" "mainnet" "eth_getBalance"
                // "[\"0x551e7784778ef8e048e495df49f2614f84a4f1dc\",\"0x0\"]"
                "rpc(string,string,string)",
                hex!(
                    "
                    0199a220
                    0000000000000000000000000000000000000000000000000000000000000060
                    00000000000000000000000000000000000000000000000000000000000000a0
                    00000000000000000000000000000000000000000000000000000000000000e0
                    0000000000000000000000000000000000000000000000000000000000000007
                    6d61696e6e657400000000000000000000000000000000000000000000000000
                    000000000000000000000000000000000000000000000000000000000000000e
                    6574685f67657442616c616e6365000000000000000000000000000000000000
                    0000000000000000000000000000000000000000000000000000000000000034
                    5b22307835353165373738343737386566386530343865343935646634396632
                    363134663834613466316463222c22307830225d000000000000000000000000
                "
                )
                .to_vec(),
                Some(vec![
                    "\"mainnet\"".to_string(),
                    "\"eth_getBalance\"".to_string(),
                    "\"[\\\"0x551e7784778ef8e048e495df49f2614f84a4f1dc\\\",\\\"0x0\\\"]\""
                        .to_string(),
                ]),
            ),
        ];

        // [function_signature, expected]
        let cheatcode_output_test_cases = vec![
            // Should redact private key on output in all cases:
            ("createWallet(string)", Some("<pk>".to_string())),
            ("deriveKey(string,uint32)", Some("<pk>".to_string())),
            // Should redact RPC URL if defined, except if referenced by an alias:
            ("rpcUrl(string)", Some("<rpc url>".to_string())),
            ("rpcUrls()", Some("<rpc url>".to_string())),
            ("rpcUrlStructs()", Some("<rpc url>".to_string())),
        ];

        for (function_signature, data, expected) in cheatcode_input_test_cases {
            let function = Function::parse(function_signature).unwrap();
            let result = decoder.decode_cheatcode_inputs(&function, &data);
            assert_eq!(result, expected, "Input case failed for: {function_signature}");
        }

        for (function_signature, expected) in cheatcode_output_test_cases {
            let function = Function::parse(function_signature).unwrap();
            let result = Some(decoder.decode_cheatcode_outputs(&function).unwrap_or_default());
            assert_eq!(result, expected, "Output case failed for: {function_signature}");
        }
    }
}
