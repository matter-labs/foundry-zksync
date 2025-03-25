//! foundry-compilers trait implementations for zksolc
use self::input::{ZkSolcInput, ZkSolcVersionedInput};
use crate::artifacts::{contract::Contract, error::Error, CompilerOutput as ZkCompilerOutput};
use alloy_json_abi::JsonAbi;
use foundry_compilers::{
    error::{Result, SolcError},
    resolver::parse::SolData,
    solc::SolcCompiler,
    CompilationError, Compiler, CompilerContract, CompilerOutput, CompilerVersion,
};
use foundry_compilers_artifacts_solc::{
    error::SourceLocation, BytecodeObject, Severity, SolcLanguage,
};

use itertools::Itertools;
use semver::Version;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
    str::FromStr,
};

use std::{
    fs::{self, create_dir_all, set_permissions, File},
    io::Write,
};
use tracing::{debug, instrument, trace};

#[cfg(target_family = "unix")]
use std::os::unix::fs::PermissionsExt;

pub mod input;
pub mod settings;
mod types;
pub use settings::{ZkSettings, ZkSolcSettings};
pub use types::{ErrorType, WarningType};

/// ZKsync solc release used for all ZKsync solc versions
pub const ZKSYNC_SOLC_RELEASE: Version = Version::new(1, 0, 1);

/// Get zksolc versions that are specifically not supported
pub const ZKSOLC_UNSUPPORTED_VERSIONS: [Version; 1] = [Version::new(1, 5, 9)];

#[cfg(test)]
macro_rules! take_solc_installer_lock {
    ($lock:ident) => {
        let lock_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(".lock");
        let lock_file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(lock_path)
            .unwrap();
        let mut lock = fd_lock::RwLock::new(lock_file);
        let $lock = lock.write().unwrap();
    };
}

impl CompilerContract for Contract {
    fn abi_ref(&self) -> Option<&JsonAbi> {
        self.abi.as_ref()
    }
    fn bin_ref(&self) -> Option<&BytecodeObject> {
        if let Some(ref eravm) = self.eravm {
            eravm.bytecode_ref()
        } else {
            None
        }
    }
    fn bin_runtime_ref(&self) -> Option<&BytecodeObject> {
        if let Some(ref eravm) = self.eravm {
            eravm.bytecode_ref()
        } else {
            None
        }
    }
}

#[derive(Debug, Clone, Serialize)]
enum ZkSolcOS {
    LinuxAMD64,
    LinuxARM64,
    MacAMD,
    MacARM,
}

fn get_operating_system() -> Result<ZkSolcOS> {
    match std::env::consts::OS {
        "linux" => match std::env::consts::ARCH {
            "aarch64" => Ok(ZkSolcOS::LinuxARM64),
            _ => Ok(ZkSolcOS::LinuxAMD64),
        },
        "macos" | "darwin" => match std::env::consts::ARCH {
            "aarch64" => Ok(ZkSolcOS::MacARM),
            _ => Ok(ZkSolcOS::MacAMD),
        },
        _ => Err(SolcError::msg(format!("Unsupported operating system {}", std::env::consts::OS))),
    }
}

impl ZkSolcOS {
    fn get_zksolc_prefix(&self) -> &str {
        match self {
            Self::LinuxAMD64 => "zksolc-linux-amd64-musl-",
            Self::LinuxARM64 => "zksolc-linux-arm64-musl-",
            Self::MacAMD => "zksolc-macosx-amd64-",
            Self::MacARM => "zksolc-macosx-arm64-",
        }
    }

    fn get_solc_prefix(&self) -> &str {
        match self {
            Self::LinuxAMD64 => "solc-linux-amd64-",
            Self::LinuxARM64 => "solc-linux-arm64-",
            Self::MacAMD => "solc-macosx-amd64-",
            Self::MacARM => "solc-macosx-arm64-",
        }
    }
}

/// ZkSolc compiler
#[derive(Debug, Clone, Default)]
pub struct ZkSolcCompiler {
    /// solc compiler to use along zksolc
    pub solc: SolcCompiler,
}

impl Compiler for ZkSolcCompiler {
    type Input = ZkSolcVersionedInput;
    type CompilationError = Error;
    type CompilerContract = Contract;
    type ParsedSource = SolData;
    type Settings = ZkSolcSettings;
    type Language = SolcLanguage;

    fn compile(
        &self,
        input: &Self::Input,
    ) -> Result<CompilerOutput<Self::CompilationError, Self::CompilerContract>> {
        let zksolc = self.zksolc(input)?;

        let mut zk_output = zksolc.compile(&input.input)?;

        let mut metadata = BTreeMap::new();
        if let Some(solc_version) = zk_output.version.take() {
            metadata.insert("solcVersion".to_string(), solc_version.into());
        }
        if let Some(solc_long_version) = zk_output.long_version.take() {
            metadata.insert("solcLongVersion".to_string(), solc_long_version.into());
        }
        if let Some(zk_version) = zk_output.zk_version.take() {
            metadata.insert("zksolcVersion".to_string(), zk_version.into());
        }
        if let Some(zksync_solc_version) = zk_output.zksync_solc_version {
            metadata
                .insert("zksyncSolcVersion".to_string(), zksync_solc_version.to_string().into());
        }

        Ok(CompilerOutput {
            sources: zk_output.sources,
            errors: zk_output.errors,
            contracts: zk_output.contracts,
            metadata,
        })
    }

    // NOTE: This is used in the context of matching source files to compiler version so
    // the solc versions are returned
    fn available_versions(&self, _language: &Self::Language) -> Vec<CompilerVersion> {
        match &self.solc {
            SolcCompiler::Specific(solc) => vec![CompilerVersion::Installed(Version::new(
                solc.version.major,
                solc.version.minor,
                solc.version.patch,
            ))],
            SolcCompiler::AutoDetect => {
                let mut all_versions = ZkSolc::solc_installed_versions()
                    .into_iter()
                    .map(CompilerVersion::Installed)
                    .collect::<Vec<_>>();
                let mut uniques = all_versions
                    .iter()
                    .map(|v| {
                        let v = v.as_ref();
                        (v.major, v.minor, v.patch)
                    })
                    .collect::<std::collections::HashSet<_>>();
                all_versions.extend(
                    ZkSolc::solc_available_versions()
                        .into_iter()
                        .filter(|v| uniques.insert((v.major, v.minor, v.patch)))
                        .map(CompilerVersion::Remote),
                );
                all_versions.sort_unstable();
                all_versions
            }
        }
    }
}

impl ZkSolcCompiler {
    /// Get zksolc command wrapper
    pub fn zksolc(&self, input: &ZkSolcVersionedInput) -> Result<ZkSolc> {
        let solc = match &self.solc {
            SolcCompiler::Specific(solc) => Some(solc.solc.clone()),
            SolcCompiler::AutoDetect => {
                #[cfg(test)]
                take_solc_installer_lock!(_lock);

                let solc_version_without_metadata = format!(
                    "{}.{}.{}",
                    input.solc_version.major, input.solc_version.minor, input.solc_version.patch
                );
                let maybe_solc =
                    ZkSolc::find_solc_installed_version(&solc_version_without_metadata)?;
                if let Some(solc) = maybe_solc {
                    Some(solc)
                } else {
                    {
                        let installed_solc_path =
                            ZkSolc::solc_blocking_install(&solc_version_without_metadata)?;
                        Some(installed_solc_path)
                    }
                }
            }
        };

        let mut zksolc = ZkSolc::new(input.zksolc_path.clone(), solc)?;

        zksolc.base_path.clone_from(&input.cli_settings.base_path);
        zksolc.allow_paths.clone_from(&input.cli_settings.allow_paths);
        zksolc.include_paths.clone_from(&input.cli_settings.include_paths);

        Ok(zksolc)
    }
}

/// Version metadata. Will include `zksync_version` if compiler is zksync solc.
#[derive(Debug, Clone, Eq, PartialEq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct SolcVersionInfo {
    /// The solc compiler version (e.g: 0.8.20)
    pub version: Version,
    /// The full zksync solc compiler version (e.g: 0.8.20-1.0.1)
    pub zksync_version: Option<Version>,
}

/// Given a solc path, get both the solc semver and optional zkSync version.
pub fn get_solc_version_info(path: &Path) -> Result<SolcVersionInfo, SolcError> {
    let mut cmd = Command::new(path);
    cmd.arg("--version").stdin(Stdio::piped()).stderr(Stdio::piped()).stdout(Stdio::piped());
    debug!(?cmd, "getting Solc versions");

    let output = cmd.output().map_err(|e| SolcError::io(e, path))?;
    trace!(?output);

    if !output.status.success() {
        return Err(SolcError::solc_output(&output));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<&str> = stdout.lines().filter(|l| !l.trim().is_empty()).collect();

    // Get solc version from second line
    let version = lines.get(1).ok_or_else(|| SolcError::msg("Version not found in Solc output"))?;
    let version =
        Version::from_str(&version.trim_start_matches("Version: ").replace(".g++", ".gcc"))?;

    // Check for ZKsync version in the last line
    let zksync_version = lines.last().and_then(|line| {
        if line.starts_with("ZKsync") {
            let version_str = line.trim_start_matches("ZKsync:").trim();
            Version::parse(version_str).ok()
        } else {
            None
        }
    });

    Ok(SolcVersionInfo { version, zksync_version })
}

/// Abstraction over `zksolc` command line utility
///
/// Supports sync and async functions.
///
/// By default the zksolc path is configured as follows, with descending priority:
///   1. `ZKSOLC_PATH` environment variable
///   2. `zksolc` otherwise
#[derive(Debug, Clone, Eq, PartialEq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ZkSolc {
    /// Path to the `zksolc` executable
    pub zksolc: PathBuf,
    /// Value for --base path
    pub base_path: Option<PathBuf>,
    /// Value for --allow-paths arg.
    pub allow_paths: BTreeSet<PathBuf>,
    /// Value for --include-paths arg.
    pub include_paths: BTreeSet<PathBuf>,
    /// Value for --solc arg
    pub solc: Option<PathBuf>,
    /// Version data for solc
    pub solc_version_info: SolcVersionInfo,
}

impl ZkSolc {
    /// A new instance which points to `zksolc`
    pub fn new(path: PathBuf, solc: Option<PathBuf>) -> Result<Self> {
        let default_solc_path = PathBuf::from("solc");
        let solc_path = solc.as_ref().unwrap_or(&default_solc_path);
        let solc_version_info = get_solc_version_info(solc_path)?;
        Ok(Self {
            zksolc: path,
            base_path: None,
            allow_paths: Default::default(),
            include_paths: Default::default(),
            solc,
            solc_version_info,
        })
    }

    /// Get zksolc path for a given version
    pub fn get_path_for_version(version: &Version) -> Result<PathBuf> {
        let maybe_zksolc = Self::find_installed_version(version)?;

        let path =
            if let Some(zksolc) = maybe_zksolc { zksolc } else { Self::blocking_install(version)? };

        Ok(path)
    }

    /// Invokes `zksolc --version` and parses the output as a SemVer [`Version`].
    pub fn get_version_for_path(path: &Path) -> Result<Version> {
        let mut cmd = Command::new(path);
        cmd.arg("--version").stdin(Stdio::piped()).stderr(Stdio::piped()).stdout(Stdio::piped());
        debug!(?cmd, "getting ZkSolc version");
        let output = cmd.output().map_err(map_io_err(path))?;
        trace!(?output);
        let version = version_from_output(output)?;
        debug!(%version);
        Ok(version)
    }

    /// Sets zksolc's base path
    pub fn with_base_path(mut self, base_path: impl Into<PathBuf>) -> Self {
        self.base_path = Some(base_path.into());
        self
    }

    /// Compiles with `--standard-json` and deserializes the output as [`CompilerOutput`].
    pub fn compile(&self, input: &ZkSolcInput) -> Result<ZkCompilerOutput> {
        let output = self.compile_output(input)?;
        // Only run UTF-8 validation once.
        let output = std::str::from_utf8(&output).map_err(|_| SolcError::InvalidUtf8)?;

        let mut compiler_output: ZkCompilerOutput = serde_json::from_str(output)?;

        // Sanitize contract names that are source file paths, using the file name without
        // path or .yul extension. This happens in zksolc versions older than 1.5.9 and
        // creates issues.
        // See: https://github.com/matter-labs/era-compiler-solidity/issues/243
        if input.is_yul() {
            for contracts in compiler_output.contracts.values_mut() {
                let contract_names = contracts.keys().cloned().collect::<Vec<String>>();
                for name in contract_names {
                    if name.ends_with(".yul") {
                        let sanitized_name = name
                            .split('/')
                            .next_back()
                            .and_then(|name| name.strip_suffix(".yul"))
                            .expect("Error sanitizing path into name");
                        // Removing and inserting should be fine because there cannot be
                        // two contracts named the same in a source file output
                        let contract = contracts.remove(&name).expect("Error replacing yul key");
                        contracts.insert(sanitized_name.into(), contract);
                    }
                }
            }
        }
        // Add zksync version so that there's some way to identify if zksync solc was used
        // by looking at build info
        compiler_output.zksync_solc_version = self.solc_version_info.zksync_version.clone();
        Ok(compiler_output)
    }

    /// Get installed versions of zksync solc
    pub fn solc_installed_versions() -> Vec<Version> {
        if let Ok(dir) = Self::compilers_dir() {
            let os = get_operating_system().unwrap();
            let solc_prefix = os.get_solc_prefix();
            let mut versions: Vec<Version> = walkdir::WalkDir::new(dir)
                .max_depth(1)
                .into_iter()
                .filter_map(std::result::Result::ok)
                .filter(|e| e.file_type().is_file())
                .filter_map(|e| e.file_name().to_str().map(|s| s.to_string()))
                .filter(|e| e.ends_with(&ZKSYNC_SOLC_RELEASE.to_string()))
                .filter_map(|e| {
                    e.strip_prefix(solc_prefix)
                        .and_then(|s| s.split('-').next())
                        .and_then(|s| Version::parse(s).ok())
                })
                .collect();
            versions.sort();
            versions
        } else {
            vec![]
        }
    }

    /// Get supported zksolc versions
    pub fn zksolc_supported_versions() -> Vec<Version> {
        let mut ret = vec![];
        let version_ranges = vec![(1, 5, 6..=11)];

        for (major, minor, patch_range) in version_ranges {
            for patch in patch_range {
                let v = Version::new(major, minor, patch);
                if !ZKSOLC_UNSUPPORTED_VERSIONS.contains(&v) {
                    ret.push(v);
                }
            }
        }

        ret
    }

    /// Get zksolc minimum supported version
    pub fn zksolc_minimum_supported_version() -> Version {
        ZkSolc::zksolc_supported_versions().remove(0)
    }

    /// Get zksolc minimum supported version
    pub fn zksolc_latest_supported_version() -> Version {
        ZkSolc::zksolc_supported_versions().pop().expect("No supported zksolc versions")
    }

    /// Get available zksync solc versions
    pub fn solc_available_versions() -> Vec<Version> {
        let mut ret = vec![];
        let version_ranges =
            vec![(0, 4, 12..=26), (0, 5, 0..=17), (0, 6, 0..=12), (0, 7, 0..=6), (0, 8, 0..=28)];
        for (major, minor, patch_range) in version_ranges {
            for patch in patch_range {
                ret.push(Version::new(major, minor, patch));
            }
        }

        ret
    }

    /// Compiles with `--standard-json` and returns the raw `stdout` output.
    #[instrument(name = "compile", level = "debug", skip_all)]
    pub fn compile_output(&self, input: &ZkSolcInput) -> Result<Vec<u8>> {
        let mut cmd = Command::new(&self.zksolc);

        if !self.allow_paths.is_empty() {
            cmd.arg("--allow-paths");
            cmd.arg(self.allow_paths.iter().map(|p| p.display()).join(","));
        }

        if let Some(base_path) = &self.base_path {
            for path in self.include_paths.iter().filter(|p| p.as_path() != base_path.as_path()) {
                cmd.arg("--include-path").arg(path);
            }

            cmd.arg("--base-path").arg(base_path);

            cmd.current_dir(base_path);
        }

        // don't pass solc argument in yul mode (avoid verification)
        if !input.is_yul() {
            if let Some(solc) = &self.solc {
                cmd.arg("--solc").arg(solc);
            }
        }

        cmd.arg("--standard-json");
        cmd.stdin(Stdio::piped()).stderr(Stdio::piped()).stdout(Stdio::piped());

        trace!(input=%serde_json::to_string(input).unwrap_or_else(|e| e.to_string()));
        debug!(?cmd, "compiling");

        let mut child = cmd.spawn().map_err(map_io_err(&self.zksolc))?;
        debug!("spawned");

        let stdin = child.stdin.as_mut().unwrap();
        serde_json::to_writer(stdin, input)?;
        debug!("wrote JSON input to stdin");

        let output = child.wait_with_output().map_err(map_io_err(&self.zksolc))?;
        debug!(%output.status, output.stderr = ?String::from_utf8_lossy(&output.stderr), "finished");

        compile_output(output)
    }

    fn compilers_dir() -> Result<PathBuf> {
        let mut compilers_dir = dirs::home_dir()
            .ok_or(SolcError::msg("Could not build SolcManager - homedir not found"))?;
        compilers_dir.push(".zksync");
        Ok(compilers_dir)
    }

    fn compiler_path(version: &Version) -> Result<PathBuf> {
        let os = get_operating_system()?;
        Ok(Self::compilers_dir()?.join(format!("{}v{}", os.get_zksolc_prefix(), version)))
    }

    fn solc_path(version_str: &str) -> Result<PathBuf> {
        let os = get_operating_system()?;
        Ok(Self::compilers_dir()?.join(format!(
            "{}{}-{}",
            os.get_solc_prefix(),
            version_str,
            ZKSYNC_SOLC_RELEASE
        )))
    }

    /// Install zksolc version and block the thread
    pub fn blocking_install(version: &Version) -> Result<PathBuf> {
        let os = get_operating_system()?;
        let compiler_prefix = os.get_zksolc_prefix();
        let download_url = if version.pre.is_empty() {
            format!(
                "https://github.com/matter-labs/zksolc-bin/releases/download/v{version}/{compiler_prefix}v{version}",
            )
        } else {
            let pre = version.pre.as_str();
            // Use version as string without pre-release and build metadata
            let version_str = version.to_string();
            let version_str = version_str.split('-').next().unwrap();
            // Matter Labs uses a different repositiry for pre-releases
            format!(
                "https://github.com/matter-labs/era-compiler-solidity/releases/download/{pre}/zksolc-{compiler_prefix}v{version_str}",
            )
        };
        let compilers_dir = Self::compilers_dir()?;
        if !compilers_dir.exists() {
            create_dir_all(compilers_dir)
                .map_err(|e| SolcError::msg(format!("Could not create compilers path: {e}")))?;
        }
        let compiler_path = Self::compiler_path(version)?;
        let lock_path = lock_file_path("zksolc", &version.to_string());

        let label = format!("zksolc-{version}");
        let install = compiler_blocking_install(compiler_path, lock_path, &download_url, &label);

        match install {
            Ok(path) => {
                //crate::report::solc_installation_success(version);
                Ok(path)
            }
            Err(err) => {
                //crate::report::solc_installation_error(version, &err.to_string());
                Err(err)
            }
        }
    }

    /// Install zksync solc version and block the thread
    pub fn solc_blocking_install(version_str: &str) -> Result<PathBuf> {
        let os = get_operating_system()?;
        let solc_os_namespace = os.get_solc_prefix();
        let download_url = format!(
            "https://github.com/matter-labs/era-solidity/releases/download/{version_str}-{ZKSYNC_SOLC_RELEASE}/{solc_os_namespace}{version_str}-{ZKSYNC_SOLC_RELEASE}",
        );

        let compilers_dir = Self::compilers_dir()?;
        if !compilers_dir.exists() {
            create_dir_all(compilers_dir)
                .map_err(|e| SolcError::msg(format!("Could not create compilers path: {e}")))?;
        }
        let solc_path = Self::solc_path(version_str)?;
        let lock_path = lock_file_path("solc", version_str);

        let label = format!("solc-{version_str}");
        compiler_blocking_install(solc_path, lock_path, &download_url, &label)
    }

    /// Get path for installed zksolc version. Returns `Ok(None)` if not installed
    pub fn find_installed_version(version: &Version) -> Result<Option<PathBuf>> {
        let zksolc = Self::compiler_path(version)?;

        if !zksolc.is_file() {
            return Ok(None);
        }
        Ok(Some(zksolc))
    }

    /// Get path for installed ZKsync solc version. Returns `Ok(None)` if not installed
    pub fn find_solc_installed_version(version_str: &str) -> Result<Option<PathBuf>> {
        let solc = Self::solc_path(version_str)?;

        if !solc.is_file() {
            return Ok(None);
        }
        Ok(Some(solc))
    }
}

fn map_io_err(zksolc_path: &Path) -> impl FnOnce(std::io::Error) -> SolcError + '_ {
    move |err| SolcError::io(err, zksolc_path)
}

fn compile_output(output: Output) -> Result<Vec<u8>> {
    if output.status.success() {
        Ok(output.stdout)
    } else {
        Err(SolcError::solc_output(&output))
    }
}

fn version_from_output(output: Output) -> Result<Version> {
    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let version = stdout
            .lines()
            .filter(|l| !l.trim().is_empty())
            .next_back()
            .ok_or_else(|| SolcError::msg("Version not found in zksolc output"))?;

        version
            .split_whitespace()
            .find_map(|s| {
                let trimmed = s.trim_start_matches('v');
                Version::from_str(trimmed).ok()
            })
            .ok_or_else(|| SolcError::msg("Unable to retrieve version from zksolc output"))
    } else {
        Err(SolcError::solc_output(&output))
    }
}

impl AsRef<Path> for ZkSolc {
    fn as_ref(&self) -> &Path {
        &self.zksolc
    }
}

impl CompilationError for Error {
    fn is_warning(&self) -> bool {
        self.severity.is_warning()
    }
    fn is_error(&self) -> bool {
        self.severity.is_error()
    }

    fn source_location(&self) -> Option<SourceLocation> {
        self.source_location.clone()
    }

    fn severity(&self) -> Severity {
        self.severity
    }

    fn error_code(&self) -> Option<u64> {
        self.error_code
    }
}

fn compiler_blocking_install(
    compiler_path: PathBuf,
    lock_path: PathBuf,
    download_url: &str,
    label: &str,
) -> Result<PathBuf> {
    use foundry_compilers::utils::RuntimeOrHandle;
    trace!("blocking installing {label}");
    //trace!("blocking installing {label}");
    // An async block is used because the underlying `reqwest::blocking::Client` does not behave
    // well inside of a Tokio runtime. See: https://github.com/seanmonstar/reqwest/issues/1017
    RuntimeOrHandle::new().block_on(async {
        let client = reqwest::Client::new();
        let response = client
            .get(download_url)
            .send()
            .await
            .map_err(|e| SolcError::msg(format!("Failed to download {label} file: {e}")))?;

        if response.status().is_success() {
            let content = response
                .bytes()
                .await
                .map_err(|e| SolcError::msg(format!("failed to download {label} file: {e}")))?;
            trace!("downloaded {label}");

            // lock file to indicate that installation of this compiler version will be in progress.
            // wait until lock file is released, possibly by another parallel thread trying to
            // install the same compiler version.
            trace!("try to get lock for {label}");
            let _lock = try_lock_file(lock_path)?;
            trace!("got lock for {label}");

            // Only write to file if it is not there. The check is done after acquiring the lock
            // to ensure the thread remains blocked until the required compiler is
            // fully installed
            if !compiler_path.exists() {
                trace!("creating binary for {label}");
                //trace!("creating binary for {label}");
                let mut output_file = File::create(&compiler_path).map_err(|e| {
                    SolcError::msg(format!("Failed to create output {label} file: {e}"))
                })?;

                output_file.write_all(&content).map_err(|e| {
                    SolcError::msg(format!("Failed to write the downloaded {label} file: {e}"))
                })?;

                set_permissions(&compiler_path, PermissionsExt::from_mode(0o755)).map_err(|e| {
                    SolcError::msg(format!("Failed to set {label} permissions: {e}"))
                })?;
            } else {
                trace!("found binary for {label}");
            }
        } else {
            return Err(SolcError::msg(format!(
                "Failed to download {label} file: status code {}",
                response.status()
            )));
        }
        trace!("{label} installation completed");
        Ok(compiler_path)
    })
}

/// Creates the file and locks it exclusively, this will block if the file is currently locked
fn try_lock_file(lock_path: PathBuf) -> Result<LockFile> {
    use fs4::FileExt;
    let _lock_file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(true)
        .read(true)
        .write(true)
        .open(&lock_path)
        .map_err(|_| SolcError::msg("Error creating lock file"))?;
    _lock_file.lock_exclusive().map_err(|_| SolcError::msg("Error taking the lock"))?;
    Ok(LockFile { lock_path, _lock_file })
}

/// Represents a lockfile that's removed once dropped
struct LockFile {
    _lock_file: File,
    lock_path: PathBuf,
}

impl Drop for LockFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.lock_path);
    }
}

/// Returns the lockfile to use for a specific file
fn lock_file_path(compiler: &str, version: &str) -> PathBuf {
    ZkSolc::compilers_dir()
        .expect("could not detect zksolc compilers directory")
        .join(format!(".lock-{compiler}-{version}"))
}

#[cfg(test)]
mod tests {
    use similar_asserts::assert_eq;

    use foundry_compilers::solc::Solc;

    use super::*;

    fn zksolc() -> ZkSolc {
        let zksolc_path =
            ZkSolc::get_path_for_version(&ZkSolc::zksolc_latest_supported_version()).unwrap();
        let solc_version = "0.8.27";

        take_solc_installer_lock!(_lock);
        let maybe_solc = ZkSolc::find_solc_installed_version(solc_version).unwrap();
        let solc_path = if let Some(solc) = maybe_solc {
            solc
        } else {
            ZkSolc::solc_blocking_install(solc_version).unwrap()
        };
        ZkSolc::new(zksolc_path, Some(solc_path)).unwrap()
    }

    fn vanilla_solc() -> Solc {
        if let Some(solc) = Solc::find_svm_installed_version(&Version::new(0, 8, 18)).unwrap() {
            solc
        } else {
            Solc::blocking_install(&Version::new(0, 8, 18)).unwrap()
        }
    }

    #[test]
    fn zksolc_version_works() {
        ZkSolc::get_version_for_path(&zksolc().zksolc).unwrap();
    }

    #[test]
    fn get_solc_type_and_version_works_for_zksync_solc() {
        let zksolc = zksolc();
        let solc = zksolc.solc.unwrap();
        let solc_v = get_solc_version_info(&solc).unwrap();
        let zksync_v = solc_v.zksync_version.unwrap();
        let prerelease = Version::parse(zksync_v.pre.as_str()).unwrap();
        assert_eq!(solc_v.version.minor, 8);
        assert_eq!(prerelease, ZKSYNC_SOLC_RELEASE);
    }

    #[test]
    fn get_solc_type_and_version_works_for_vanilla_solc() {
        let solc = vanilla_solc();
        let solc_v = get_solc_version_info(&solc.solc).unwrap();
        assert_eq!(solc_v.version.minor, 8);
        assert!(solc_v.zksync_version.is_none());
    }

    #[test]
    fn zksolc_compile_works() {
        let input = include_str!("../../../test-data/in/compiler-in-1.json");
        let input: ZkSolcInput = serde_json::from_str(input).unwrap();
        let out = zksolc().compile(&input).unwrap();
        assert!(!out.has_error());
    }

    #[test]
    fn zksolc_can_compile_with_remapped_links() {
        let input: ZkSolcInput =
            serde_json::from_str(include_str!("../../../test-data/library-remapping-in.json"))
                .unwrap();
        let out = zksolc().compile(&input).unwrap();
        let (_, mut contracts) = out.split();
        let contract = contracts.remove("LinkTest").unwrap();
        let bytecode = contract.bytecode().unwrap().object;
        assert!(!bytecode.is_unlinked());
    }

    #[test]
    fn zksolc_can_compile_with_remapped_links_temp_dir() {
        let input: ZkSolcInput =
            serde_json::from_str(include_str!("../../../test-data/library-remapping-in-2.json"))
                .unwrap();
        let out = zksolc().compile(&input).unwrap();
        let (_, mut contracts) = out.split();
        let contract = contracts.remove("LinkTest").unwrap();
        let bytecode = contract.bytecode().unwrap().object;
        assert!(!bytecode.is_unlinked());
    }
}
