use alloy_json_abi::{Error, Event, Function};
use alloy_primitives::{hex, map::HashSet};
use foundry_common::{
    abi::{get_error, get_event, get_func},
    fs,
    selectors::{OpenChainClient, SelectorType},
};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::PathBuf, sync::Arc};
use tokio::sync::RwLock;

pub type SingleSignaturesIdentifier = Arc<RwLock<SignaturesIdentifier>>;

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct CachedSignatures {
    pub errors: BTreeMap<String, String>,
    pub events: BTreeMap<String, String>,
    pub functions: BTreeMap<String, String>,
}

impl CachedSignatures {
    #[instrument(target = "evm::traces")]
    pub fn load(cache_path: PathBuf) -> Self {
        let path = cache_path.join("signatures");
        if path.is_file() {
            fs::read_json_file(&path)
                .map_err(
                    |err| warn!(target: "evm::traces", ?path, ?err, "failed to read cache file"),
                )
                .unwrap_or_default()
        } else {
            if let Err(err) = std::fs::create_dir_all(cache_path) {
                warn!(target: "evm::traces", "could not create signatures cache dir: {:?}", err);
            }
            Self::default()
        }
    }
}
/// An identifier that tries to identify functions and events using signatures found at
/// `https://openchain.xyz` or a local cache.
#[derive(Debug)]
pub struct SignaturesIdentifier {
    /// Cached selectors for functions, events and custom errors.
    cached: CachedSignatures,
    /// Location where to save `CachedSignatures`.
    cached_path: Option<PathBuf>,
    /// Selectors that were unavailable during the session.
    unavailable: HashSet<String>,
    /// The OpenChain client to fetch signatures from.
    client: Option<OpenChainClient>,
}

impl SignaturesIdentifier {
    #[instrument(target = "evm::traces")]
    pub fn new(
        cache_path: Option<PathBuf>,
        offline: bool,
    ) -> eyre::Result<SingleSignaturesIdentifier> {
        let client = if !offline { Some(OpenChainClient::new()?) } else { None };

        let identifier = if let Some(cache_path) = cache_path {
            let path = cache_path.join("signatures");
            trace!(target: "evm::traces", ?path, "reading signature cache");
            let cached = CachedSignatures::load(cache_path);
            Self { cached, cached_path: Some(path), unavailable: HashSet::default(), client }
        } else {
            Self {
                cached: Default::default(),
                cached_path: None,
                unavailable: HashSet::default(),
                client,
            }
        };

        Ok(Arc::new(RwLock::new(identifier)))
    }

    #[instrument(target = "evm::traces", skip(self))]
    pub fn save(&self) {
        if let Some(cached_path) = &self.cached_path {
            if let Some(parent) = cached_path.parent() {
                if let Err(err) = std::fs::create_dir_all(parent) {
                    warn!(target: "evm::traces", ?parent, ?err, "failed to create cache");
                }
            }
            if let Err(err) = fs::write_json_file(cached_path, &self.cached) {
                warn!(target: "evm::traces", ?cached_path, ?err, "failed to flush signature cache");
            } else {
                trace!(target: "evm::traces", ?cached_path, "flushed signature cache")
            }
        }
    }
}

impl SignaturesIdentifier {
    async fn identify<T>(
        &mut self,
        selector_type: SelectorType,
        identifiers: impl IntoIterator<Item = impl AsRef<[u8]>>,
        get_type: impl Fn(&str) -> eyre::Result<T>,
    ) -> Vec<Option<T>> {
        let cache = match selector_type {
            SelectorType::Function => &mut self.cached.functions,
            SelectorType::Event => &mut self.cached.events,
            SelectorType::Error => &mut self.cached.errors,
        };

        let hex_identifiers: Vec<String> =
            identifiers.into_iter().map(hex::encode_prefixed).collect();

        if let Some(client) = &self.client {
            let query: Vec<_> = hex_identifiers
                .iter()
                .filter(|v| !cache.contains_key(v.as_str()))
                .filter(|v| !self.unavailable.contains(v.as_str()))
                .collect();

            if let Ok(res) = client.decode_selectors(selector_type, query.clone()).await {
                for (hex_id, selector_result) in query.into_iter().zip(res.into_iter()) {
                    let mut found = false;
                    if let Some(decoded_results) = selector_result {
                        if let Some(decoded_result) = decoded_results.into_iter().next() {
                            cache.insert(hex_id.clone(), decoded_result);
                            found = true;
                        }
                    }
                    if !found {
                        self.unavailable.insert(hex_id.clone());
                    }
                }
            }
        }

        hex_identifiers.iter().map(|v| cache.get(v).and_then(|v| get_type(v).ok())).collect()
    }

    /// Identifies `Function`s from its cache or `https://api.openchain.xyz`
    pub async fn identify_functions(
        &mut self,
        identifiers: impl IntoIterator<Item = impl AsRef<[u8]>>,
    ) -> Vec<Option<Function>> {
        self.identify(SelectorType::Function, identifiers, get_func).await
    }

    /// Identifies `Function` from its cache or `https://api.openchain.xyz`
    pub async fn identify_function(&mut self, identifier: &[u8]) -> Option<Function> {
        self.identify_functions(&[identifier]).await.pop().unwrap()
    }

    /// Identifies `Event`s from its cache or `https://api.openchain.xyz`
    pub async fn identify_events(
        &mut self,
        identifiers: impl IntoIterator<Item = impl AsRef<[u8]>>,
    ) -> Vec<Option<Event>> {
        self.identify(SelectorType::Event, identifiers, get_event).await
    }

    /// Identifies `Event` from its cache or `https://api.openchain.xyz`
    pub async fn identify_event(&mut self, identifier: &[u8]) -> Option<Event> {
        self.identify_events(&[identifier]).await.pop().unwrap()
    }

    /// Identifies `Error`s from its cache or `https://api.openchain.xyz`.
    pub async fn identify_errors(
        &mut self,
        identifiers: impl IntoIterator<Item = impl AsRef<[u8]>>,
    ) -> Vec<Option<Error>> {
        self.identify(SelectorType::Error, identifiers, get_error).await
    }

    /// Identifies `Error` from its cache or `https://api.openchain.xyz`.
    pub async fn identify_error(&mut self, identifier: &[u8]) -> Option<Error> {
        self.identify_errors(&[identifier]).await.pop().unwrap()
    }
}

impl Drop for SignaturesIdentifier {
    fn drop(&mut self) {
        self.save();
    }
}
