//! Build command

use crate::cmd::{
    forge::{
        install::{self},
        watch::WatchArgs,
    },
    Cmd, LoadConfig,
};
use clap::{ArgAction, Parser};
use ethers::solc::{Project, ProjectCompileOutput};
use foundry_common::{
    compile,
    compile::{ProjectCompiler, SkipBuildFilter},
};
use foundry_config::{
    figment::{
        self,
        error::Kind::InvalidType,
        value::{Dict, Map, Value},
        Metadata, Profile, Provider,
    },
    Config,
};
use serde::Serialize;
use std::process::Command;
use std::fs::set_permissions;
use std::os::unix::prelude::PermissionsExt;
use watchexec::config::{InitConfig, RuntimeConfig};
use downloader::{self, Download, Downloader};
mod core;
pub use self::core::CoreBuildArgs;

mod paths;
pub use self::paths::ProjectPathsArgs;

foundry_config::merge_impl_figment_convert!(BuildArgs, args);

/// CLI arguments for `forge build`.
///
/// CLI arguments take the highest precedence in the Config/Figment hierarchy.
/// In order to override them in the foundry `Config` they need to be merged into an existing
/// `figment::Provider`, like `foundry_config::Config` is.
///
/// # Example
///
/// ```
/// use foundry_cli::cmd::forge::build::BuildArgs;
/// use foundry_config::Config;
/// # fn t(args: BuildArgs) {
/// let config = Config::from(&args);
/// # }
/// ```
///
/// `BuildArgs` implements `figment::Provider` in which all config related fields are serialized and
/// then merged into an existing `Config`, effectively overwriting them.
///
/// Some arguments are marked as `#[serde(skip)]` and require manual processing in
/// `figment::Provider` implementation
#[derive(Debug, Clone, Parser, Serialize, Default)]
#[clap(next_help_heading = "Build options", about = None)] // override doc
pub struct BuildArgs {
    #[clap(flatten)]
    #[serde(flatten)]
    pub args: CoreBuildArgs,

    #[clap(help = "Print compiled contract names.", long = "names")]
    #[serde(skip)]
    pub names: bool,

    #[clap(help = "Print compiled contract sizes.", long = "sizes")]
    #[serde(skip)]
    pub sizes: bool,

    #[clap(
        long,
        num_args(1..),
        action = ArgAction::Append,
        help = "Skip building whose names contain SKIP. `test` and `script` are aliases for `.t.sol` and `.s.sol`. (this flag can be used multiple times)")]
    #[serde(skip)]
    pub skip: Option<Vec<SkipBuildFilter>>,

    #[clap(flatten)]
    #[serde(skip)]
    pub watch: WatchArgs,

    #[clap(help_heading = "Compiler options", long, help = "Compile with ZkSync.")]
    #[serde(skip)]
    pub zksync: bool,
}

impl Cmd for BuildArgs {
    type Output = ProjectCompileOutput;
    fn run(self) -> eyre::Result<Self::Output> {
        println!("{:#?}", self);
        let mut config = self.try_load_config_emit_warnings()?;
        let mut project = config.project()?;

        if install::install_missing_dependencies(&mut config, &project, self.args.silent)
            && config.auto_detect_remappings
        {
            // need to re-configure here to also catch additional remappings
            config = self.load_config();
            project = config.project()?;
        }

        if self.zksync {
            compile_zksync(&config, &project);
        } else {
            println!("Morty, Morty, Morty, Morty, Morty, Morty, Morty, Morty, ");
            println!("Morty, Morty, Morty, Morty, Morty, Morty, Morty, Morty, ");
        }

        let filters = self.skip.unwrap_or_default();
        if self.args.silent {
            compile::suppress_compile_with_filter(&project, filters)
        } else {
            let compiler = ProjectCompiler::with_filter(self.names, self.sizes, filters);
            compiler.compile(&project)
        }
    }
}

use std::fs;
pub fn compile_zksync(config: &Config, project: &Project) {
    let zkout_path =
        &format!("{}{}", project.paths.root.display(), "/zksolc");
    fs::create_dir_all(std::path::Path::new( zkout_path));
    // println!("{:#?}, config", config);
    // println!("{:#?} project root", project.paths);
    let base_path = project.paths.root.clone();
    let mut base_path_string = base_path.clone().into_os_string().into_string().unwrap();
    base_path_string.push_str("/cli/src/cmd/forge/build/assets/zksolc-linux-amd64-musl-v1.3.0");
    // println!("{:#?}, base_path", &base_path_string);

    //check for compiler
    let _filepath =
        &format!("{}{}", project.paths.root.display(), "/zksolc/zksolc-linux-amd64-musl-v1.3.0");
    let b = std::path::Path::new(_filepath).exists();
    // println!("{}: {}", _filepath, b);
    // let src_path = &format!("{}{}", project.paths.sources.display(), "/*.sol");
    let counter_path = &format!("{}{}", project.paths.sources.display(), "/Counter.sol");
    let greeter_path = &format!("{}{}", project.paths.sources.display(), "/Greeter.sol");    
    let zksolc_path: &str = &format!("{}{}", project.paths.root.display(), "/zksolc/zksolc-linux-amd64-musl-v1.3.0");
    let output: &str = &format!("{}{}", project.paths.root.display(), "/zksolc");


    if !b {
        let download: Download = Download::new("https://github.com/matter-labs/zksolc-bin/blob/main/linux-amd64/zksolc-linux-amd64-musl-v1.3.0");
        // println!("{:#?} download", download.file_name);
        //get downloader builder
        let mut builder = Downloader::builder();
        //assign download folder
        builder.download_folder(std::path::Path::new(&format!("{}{}", project.paths.root.display(), "/zksolc")));
        //build downloader
        let mut d_loader = builder.build().unwrap();
        //download compiler
        let d_load = d_loader.download(&[download]);
        // println!("{:#?} d_load", d_load);

        let perm = set_permissions(std::path::Path::new(zksolc_path), PermissionsExt::from_mode(0o755)).unwrap();
        

    }

    // "--combined-json", "abi,hashes",
    // , "--output-dir", zkout_path, "--overwrite"
    // "--hashes",
    // println!("{:#?}, src_path", &src_path);
    // println!("{:#?}, zksolc_path", &zksolc_path);

    let output = Command::new("/home/shakes/foundry-zksync/foundry-zksync/cli/src/cmd/forge/build/assets/zksolc-linux-amd64-musl-v1.3.1")
        // .arg("--help")
        .args([greeter_path,  "--abi", "--combined-json", "hashes,bin,abi" , "--output-dir", zkout_path, "--overwrite"])
        .output()
        .expect("failed to execute process");

    println!("{:#?} output", output);
}

impl BuildArgs {
    /// Returns the `Project` for the current workspace
    ///
    /// This loads the `foundry_config::Config` for the current workspace (see
    /// [`utils::find_project_root_path`] and merges the cli `BuildArgs` into it before returning
    /// [`foundry_config::Config::project()`]
    pub fn project(&self) -> eyre::Result<Project> {
        self.args.project()
    }

    /// Returns whether `BuildArgs` was configured with `--watch`
    pub fn is_watch(&self) -> bool {
        self.watch.watch.is_some()
    }

    /// Returns the [`watchexec::InitConfig`] and [`watchexec::RuntimeConfig`] necessary to
    /// bootstrap a new [`watchexe::Watchexec`] loop.
    pub(crate) fn watchexec_config(&self) -> eyre::Result<(InitConfig, RuntimeConfig)> {
        // use the path arguments or if none where provided the `src` dir
        self.watch.watchexec_config(|| {
            let config = Config::from(self);
            vec![config.src, config.test, config.script]
        })
    }
}

// Make this args a `figment::Provider` so that it can be merged into the `Config`
impl Provider for BuildArgs {
    fn metadata(&self) -> Metadata {
        Metadata::named("Build Args Provider")
    }

    fn data(&self) -> Result<Map<Profile, Dict>, figment::Error> {
        let value = Value::serialize(self)?;
        let error = InvalidType(value.to_actual(), "map".into());
        let mut dict = value.into_dict().ok_or(error)?;

        if self.names {
            dict.insert("names".to_string(), true.into());
        }

        if self.sizes {
            dict.insert("sizes".to_string(), true.into());
        }

        Ok(Map::from([(Config::selected_profile(), dict)]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn can_parse_build_filters() {
        let args: BuildArgs = BuildArgs::parse_from(["foundry-cli", "--skip", "tests"]);
        assert_eq!(args.skip, Some(vec![SkipBuildFilter::Tests]));

        let args: BuildArgs = BuildArgs::parse_from(["foundry-cli", "--skip", "scripts"]);
        assert_eq!(args.skip, Some(vec![SkipBuildFilter::Scripts]));

        let args: BuildArgs =
            BuildArgs::parse_from(["foundry-cli", "--skip", "tests", "--skip", "scripts"]);
        assert_eq!(args.skip, Some(vec![SkipBuildFilter::Tests, SkipBuildFilter::Scripts]));

        let args: BuildArgs = BuildArgs::parse_from(["foundry-cli", "--skip", "tests", "scripts"]);
        assert_eq!(args.skip, Some(vec![SkipBuildFilter::Tests, SkipBuildFilter::Scripts]));
    }
}
