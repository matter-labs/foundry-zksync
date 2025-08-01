use super::ProjectPathOpts;
use crate::{opts::CompilerOpts, utils::LoadConfig};
use clap::{Parser, ValueHint};
use eyre::Result;
use foundry_compilers::{
    Project,
    artifacts::{RevertStrings, remappings::Remapping},
    compilers::multi::MultiCompiler,
    utils::canonicalized,
};
use foundry_config::{
    Config, Remappings, figment,
    figment::{
        Figment, Metadata, Profile, Provider,
        error::Kind::InvalidType,
        value::{Dict, Map, Value},
    },
    filter::SkipBuildFilter,
};
use serde::Serialize;
use std::path::PathBuf;

#[derive(Clone, Debug, Default, Serialize, Parser)]
#[command(next_help_heading = "Build options")]
pub struct BuildOpts {
    /// Clear the cache and artifacts folder and recompile.
    #[arg(long, help_heading = "Cache options")]
    #[serde(skip)]
    pub force: bool,

    /// Disable the cache.
    #[arg(long)]
    #[serde(skip)]
    pub no_cache: bool,

    /// Enable dynamic test linking.
    #[arg(long, conflicts_with = "no_cache")]
    #[serde(skip)]
    pub dynamic_test_linking: bool,

    /// Set pre-linked libraries.
    #[arg(long, help_heading = "Linker options", env = "DAPP_LIBRARIES")]
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub libraries: Vec<String>,

    /// Ignore solc warnings by error code.
    #[arg(long, help_heading = "Compiler options", value_name = "ERROR_CODES")]
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub ignored_error_codes: Vec<u64>,

    /// Warnings will trigger a compiler error
    #[arg(long, help_heading = "Compiler options")]
    #[serde(skip)]
    pub deny_warnings: bool,

    /// Do not auto-detect the `solc` version.
    #[arg(long, help_heading = "Compiler options")]
    #[serde(skip)]
    pub no_auto_detect: bool,

    /// Specify the solc version, or a path to a local solc, to build with.
    ///
    /// Valid values are in the format `x.y.z`, `solc:x.y.z` or `path/to/solc`.
    #[arg(
        long = "use",
        alias = "compiler-version",
        help_heading = "Compiler options",
        value_name = "SOLC_VERSION"
    )]
    #[serde(skip)]
    pub use_solc: Option<String>,

    /// Do not access the network.
    ///
    /// Missing solc versions will not be installed.
    #[arg(help_heading = "Compiler options", long)]
    #[serde(skip)]
    pub offline: bool,

    /// Use the Yul intermediate representation compilation pipeline.
    #[arg(long, help_heading = "Compiler options")]
    #[serde(skip)]
    pub via_ir: bool,

    /// Changes compilation to only use literal content and not URLs.
    #[arg(long, help_heading = "Compiler options")]
    #[serde(skip)]
    pub use_literal_content: bool,

    /// Do not append any metadata to the bytecode.
    ///
    /// This is equivalent to setting `bytecode_hash` to `none` and `cbor_metadata` to `false`.
    #[arg(long, help_heading = "Compiler options")]
    #[serde(skip)]
    pub no_metadata: bool,

    /// The path to the contract artifacts folder.
    #[arg(
        long = "out",
        short,
        help_heading = "Project options",
        value_hint = ValueHint::DirPath,
        value_name = "PATH",
    )]
    #[serde(rename = "out", skip_serializing_if = "Option::is_none")]
    pub out_path: Option<PathBuf>,

    /// Revert string configuration.
    ///
    /// Possible values are "default", "strip" (remove),
    /// "debug" (Solidity-generated revert strings) and "verboseDebug"
    #[arg(long, help_heading = "Project options", value_name = "REVERT")]
    #[serde(skip)]
    pub revert_strings: Option<RevertStrings>,

    /// Generate build info files.
    #[arg(long, help_heading = "Project options")]
    #[serde(skip)]
    pub build_info: bool,

    /// Output path to directory that build info files will be written to.
    #[arg(
        long,
        help_heading = "Project options",
        value_hint = ValueHint::DirPath,
        value_name = "PATH",
        requires = "build_info",
    )]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub build_info_path: Option<PathBuf>,

    /// Skip building files whose names contain the given filter.
    ///
    /// `test` and `script` are aliases for `.t.sol` and `.s.sol`.
    #[arg(long, num_args(1..))]
    #[serde(skip)]
    pub skip: Option<Vec<SkipBuildFilter>>,

    #[command(flatten)]
    #[serde(flatten)]
    pub compiler: CompilerOpts,

    #[command(flatten)]
    #[serde(flatten)]
    pub project_paths: ProjectPathOpts,
}

impl BuildOpts {
    /// Returns the `Project` for the current workspace
    ///
    /// This loads the `foundry_config::Config` for the current workspace (see
    /// `find_project_root` and merges the cli `BuildArgs` into it before returning
    /// [`foundry_config::Config::project()`]).
    pub fn project(&self) -> Result<Project<MultiCompiler>> {
        let config = self.load_config()?;
        Ok(config.project()?)
    }

    /// Returns the remappings to add to the config
    #[deprecated(note = "Use ProjectPathsArgs::get_remappings() instead")]
    pub fn get_remappings(&self) -> Vec<Remapping> {
        self.project_paths.get_remappings()
    }
}

// Loads project's figment and merges the build cli arguments into it
impl<'a> From<&'a BuildOpts> for Figment {
    fn from(args: &'a BuildOpts) -> Self {
        let mut figment = if let Some(ref config_path) = args.project_paths.config_path {
            if !config_path.exists() {
                panic!("error: config-path `{}` does not exist", config_path.display())
            }
            if !config_path.ends_with(Config::FILE_NAME) {
                panic!("error: the config-path must be a path to a foundry.toml file")
            }
            let config_path = canonicalized(config_path);
            Config::figment_with_root(config_path.parent().unwrap())
        } else {
            Config::figment_with_root(args.project_paths.project_root())
        };

        // remappings should stack
        let mut remappings = Remappings::new_with_remappings(args.project_paths.get_remappings())
            .with_figment(&figment);
        remappings
            .extend(figment.extract_inner::<Vec<Remapping>>("remappings").unwrap_or_default());
        // override only set values from the CLI for zksync
        let zksync =
            args.compiler.zk.apply_overrides(figment.extract_inner("zksync").unwrap_or_default());

        figment = figment
            .merge(("remappings", remappings.into_inner()))
            .merge(args)
            .merge(("zksync", zksync));

        if let Some(skip) = &args.skip {
            let mut skip = skip.iter().map(|s| s.file_pattern().to_string()).collect::<Vec<_>>();
            skip.extend(figment.extract_inner::<Vec<String>>("skip").unwrap_or_default());
            figment = figment.merge(("skip", skip));
        };

        figment
    }
}

impl Provider for BuildOpts {
    fn metadata(&self) -> Metadata {
        Metadata::named("Core Build Args Provider")
    }

    fn data(&self) -> Result<Map<Profile, Dict>, figment::Error> {
        let value = Value::serialize(self)?;
        let error = InvalidType(value.to_actual(), "map".into());
        let mut dict = value.into_dict().ok_or(error)?;

        if self.no_auto_detect {
            dict.insert("auto_detect_solc".to_string(), false.into());
        }

        if let Some(ref solc) = self.use_solc {
            dict.insert("solc".to_string(), solc.trim_start_matches("solc:").into());
        }

        if self.offline {
            dict.insert("offline".to_string(), true.into());
        }

        if self.deny_warnings {
            dict.insert("deny_warnings".to_string(), true.into());
        }

        if self.via_ir {
            dict.insert("via_ir".to_string(), true.into());
        }

        if self.use_literal_content {
            dict.insert("use_literal_content".to_string(), true.into());
        }

        if self.no_metadata {
            dict.insert("bytecode_hash".to_string(), "none".into());
            dict.insert("cbor_metadata".to_string(), false.into());
        }

        if self.force {
            dict.insert("force".to_string(), self.force.into());
        }

        // we need to ensure no_cache set accordingly
        if self.no_cache {
            dict.insert("cache".to_string(), false.into());
        }

        if self.dynamic_test_linking {
            dict.insert("dynamic_test_linking".to_string(), true.into());
        }

        if self.build_info {
            dict.insert("build_info".to_string(), self.build_info.into());
        }

        if self.compiler.ast {
            dict.insert("ast".to_string(), true.into());
        }

        if let Some(optimize) = self.compiler.optimize {
            dict.insert("optimizer".to_string(), optimize.into());
        }

        if !self.compiler.extra_output.is_empty() {
            let selection: Vec<_> =
                self.compiler.extra_output.iter().map(|s| s.to_string()).collect();
            dict.insert("extra_output".to_string(), selection.into());
        }

        if !self.compiler.extra_output_files.is_empty() {
            let selection: Vec<_> =
                self.compiler.extra_output_files.iter().map(|s| s.to_string()).collect();
            dict.insert("extra_output_files".to_string(), selection.into());
        }

        if let Some(ref revert) = self.revert_strings {
            dict.insert("revert_strings".to_string(), revert.to_string().into());
        }

        Ok(Map::from([(Config::selected_profile(), dict)]))
    }
}
