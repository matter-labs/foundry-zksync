//! zksolc input
use super::{
    settings::ZkSolcSettings,
    types::{ErrorType, WarningType},
    ZkSettings,
};
use foundry_compilers::{
    artifacts::{Remapping, Source, Sources},
    compilers::{solc::SolcLanguage, CompilerInput},
    solc,
};
use foundry_compilers_artifacts_solc::serde_helpers::tuple_vec_map;
use semver::Version;
use serde::{Deserialize, Serialize};
use std::{
    borrow::Cow,
    collections::HashSet,
    path::{Path, PathBuf},
};
use tracing::warn;

/// Versioned input for zksolc
#[derive(Debug, Clone, Serialize)]
pub struct ZkSolcVersionedInput {
    /// zksolc json input
    #[serde(flatten)]
    pub input: ZkSolcInput,
    /// solc version to be used along zksolc
    pub solc_version: Version,
    /// zksolc cli settings
    pub cli_settings: solc::CliSettings,
    /// zksolc binary path
    pub zksolc_path: PathBuf,
}

impl CompilerInput for ZkSolcVersionedInput {
    type Settings = ZkSolcSettings;
    type Language = SolcLanguage;

    // WARN: version is the solc version and NOT the zksolc version
    // This is because we use solc's version resolution to figure
    // out what solc to pair zksolc with.
    fn build(
        sources: Sources,
        settings: Self::Settings,
        language: Self::Language,
        version: Version,
    ) -> Self {
        let zksolc_path = settings.zksolc_path();
        let zksolc_version = settings.zksolc_version_ref().clone();
        let ZkSolcSettings { settings, cli_settings, .. } = settings;
        let input =
            ZkSolcInput::new(language, sources, settings, &zksolc_version).sanitized(&version);

        Self { solc_version: version, input, cli_settings, zksolc_path }
    }

    fn language(&self) -> Self::Language {
        self.input.language
    }

    // TODO: This is the solc_version and not the zksolc version. We store this here because
    // the input is not associated with a zksolc version and we use solc's version resolution
    // features to know what solc to use to compile a file with. We should think about
    // how to best honor this api so the value is not confusing.
    fn version(&self) -> &Version {
        &self.solc_version
    }

    fn sources(&self) -> impl Iterator<Item = (&Path, &Source)> {
        self.input.sources.iter().map(|(path, source)| (path.as_path(), source))
    }

    fn compiler_name(&self) -> Cow<'static, str> {
        "zksolc and solc".into()
    }

    fn strip_prefix(&mut self, base: &Path) {
        self.input.strip_prefix(base);
    }
}

/// Input type `zksolc` expects.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ZkSolcInput {
    /// source code language
    pub language: SolcLanguage,
    /// sources to compile
    pub sources: Sources,
    /// compiler settings set by the user
    pub settings: ZkSettings,
    /// suppressed warnings
    // For `zksolc` versions <1.5.7, suppressed warnings / errors were specified on the same level
    // as `settings`. For `zksolc` 1.5.7+, they are specified inside `settings`. Since we want to
    // support both options at the time, we duplicate fields from `settings` here.
    #[serde(default, skip_serializing_if = "HashSet::is_empty")]
    pub suppressed_warnings: HashSet<WarningType>,
    /// suppressed errors
    #[serde(default, skip_serializing_if = "HashSet::is_empty")]
    pub suppressed_errors: HashSet<ErrorType>,
}

/// Default `language` field is set to `"Solidity"`.
impl Default for ZkSolcInput {
    fn default() -> Self {
        Self {
            language: SolcLanguage::Solidity,
            sources: Sources::default(),
            settings: ZkSettings::default(),
            suppressed_warnings: HashSet::default(),
            suppressed_errors: HashSet::default(),
        }
    }
}

impl ZkSolcInput {
    fn new(
        language: SolcLanguage,
        sources: Sources,
        mut settings: ZkSettings,
        zksolc_version: &Version,
    ) -> Self {
        let mut suppressed_warnings = HashSet::default();
        let mut suppressed_errors = HashSet::default();
        // zksolc <= 1.5.6 has suppressed warnings/errors in at the root input level
        if zksolc_version <= &Version::new(1, 5, 6) {
            suppressed_warnings = std::mem::take(&mut settings.suppressed_warnings);
            suppressed_errors = std::mem::take(&mut settings.suppressed_errors);
        }

        if let Some(ref mut metadata) = settings.metadata {
            metadata.sanitize(zksolc_version);
        };

        Self { language, sources, settings, suppressed_warnings, suppressed_errors }
    }

    /// Removes the `base` path from all source files
    pub fn strip_prefix(&mut self, base: impl AsRef<Path>) {
        let base = base.as_ref();
        self.sources = std::mem::take(&mut self.sources)
            .into_iter()
            .map(|(path, s)| (path.strip_prefix(base).map(Into::into).unwrap_or(path), s))
            .collect();

        self.settings.strip_prefix(base);
    }
    /// The flag indicating whether the current [CompilerInput] is
    /// constructed for the yul sources
    pub fn is_yul(&self) -> bool {
        self.language == SolcLanguage::Yul
    }
    /// Consumes the type and returns a [ZkSolcInput::sanitized] version
    pub fn sanitized(mut self, version: &Version) -> Self {
        self.settings.sanitize(version);
        self
    }

    /// Add remappings to settings
    pub fn with_remappings(mut self, remappings: Vec<Remapping>) -> Self {
        if self.language == SolcLanguage::Yul {
            if !remappings.is_empty() {
                warn!("omitting remappings supplied for the yul sources");
            }
        } else {
            self.settings.remappings = remappings;
        }

        self
    }
}

/// A `CompilerInput` representation used for verify
///
/// This type is an alternative `CompilerInput` but uses non-alphabetic ordering of the `sources`
/// and instead emits the (Path -> Source) path in the same order as the pairs in the `sources`
/// `Vec`. This is used over a map, so we can determine the order in which etherscan will display
/// the verified contracts
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StandardJsonCompilerInput {
    /// compiler language
    pub language: SolcLanguage,
    /// sources to compile
    #[serde(with = "tuple_vec_map")]
    pub sources: Vec<(PathBuf, Source)>,
    /// compiler settings
    pub settings: ZkSettings,
}

impl StandardJsonCompilerInput {
    /// new StandardJsonCompilerInput
    pub fn new(sources: Vec<(PathBuf, Source)>, settings: ZkSettings) -> Self {
        Self { language: SolcLanguage::Solidity, sources, settings }
    }
}
