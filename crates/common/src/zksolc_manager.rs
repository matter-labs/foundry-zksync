#![allow(missing_docs)]
//! The `ZkSolcManager` module manages the downloading and setup of the `zksolc` Solidity
/// compiler. This module provides functionalities to interact with different versions of the
/// zksolc compiler as well as supporting different operating systems.
///
/// This module consists of several key components:
///
/// * `ZkSolcVersion`: This enum represents the available versions of the zksolc compiler. The
///   `parse_version` function accepts a string version and returns the corresponding
///   `ZkSolcVersion` variant.
///
/// * `ZkSolcOS`: This enum represents the supported operating systems for the zksolc compiler.
///   `get_operating_system` function determines the current operating system and returns the
///   corresponding `ZkSolcOS` variant.
///
/// * `ZkSolcManagerOpts`: This structure holds options for creating a `ZkSolcManagerBuilder`,
///   which includes the version of the compiler to be used.
///
/// * `ZkSolcManagerBuilder`: This structure is used to construct a `ZkSolcManager`. It holds
///   options like compilers path, version, compiler name, and download URL. It has a `build`
///   function which constructs a `ZkSolcManager` instance.
///
/// * `ZkSolcManager`: This structure manages a particular version of the zksolc compiler. It
///   includes functions to get the full compiler name, check if the compiler exists, setup the
///   compilers directory, and download the compiler if necessary.
///
/// This module abstracts the details of managing the zksolc compiler, making it easier for
/// developers to use different versions of the compiler without dealing with the details of
/// downloading, setting up, and switching between versions. It is part of a larger framework
/// for managing and interacting with zkSync contracts.
use anyhow::{anyhow, Context, Error, Result};
use dirs;
use reqwest::blocking::Client;
use serde::Serialize;
use std::{fmt, fs, fs::File, io::copy, os::unix::prelude::PermissionsExt, path::PathBuf};
use url::Url;

const ZKSOLC_DOWNLOAD_BASE_URL: &str =
    "https://github.com/matter-labs/zksolc-bin/releases/download/";

#[derive(Debug, Clone, Serialize)]
pub enum ZkSolcVersion {
    V135,
    V136,
    V137,
    V138,
    V139,
    V1310,
    V1311,
    V1313,
    V1314,
    V1316,
    V1317,
    V1318,
    V1319,
    V1321,
}

pub const DEFAULT_ZKSOLC_VERSION: &str = "v1.3.21";

fn parse_version(version: &str) -> Result<ZkSolcVersion> {
    match version {
        "v1.3.5" => Ok(ZkSolcVersion::V135),
        "v1.3.6" => Ok(ZkSolcVersion::V136),
        "v1.3.7" => Ok(ZkSolcVersion::V137),
        "v1.3.8" => Ok(ZkSolcVersion::V138),
        "v1.3.9" => Ok(ZkSolcVersion::V139),
        "v1.3.10" => Ok(ZkSolcVersion::V1310),
        "v1.3.11" => Ok(ZkSolcVersion::V1311),
        "v1.3.13" => Ok(ZkSolcVersion::V1313),
        "v1.3.14" => Ok(ZkSolcVersion::V1314),
        "v1.3.16" => Ok(ZkSolcVersion::V1316),
        "v1.3.17" => Ok(ZkSolcVersion::V1317),
        "v1.3.18" => Ok(ZkSolcVersion::V1318),
        "v1.3.19" => Ok(ZkSolcVersion::V1319),
        "v1.3.21" => Ok(ZkSolcVersion::V1321),
        _ => Err(Error::msg(
            "ZkSolc compiler version not supported. Proper version format: 'v1.3.x'",
        )),
    }
}

impl ZkSolcVersion {
    fn get_version(&self) -> &str {
        match self {
            ZkSolcVersion::V135 => "v1.3.5",
            ZkSolcVersion::V136 => "v1.3.6",
            ZkSolcVersion::V137 => "v1.3.7",
            ZkSolcVersion::V138 => "v1.3.8",
            ZkSolcVersion::V139 => "v1.3.9",
            ZkSolcVersion::V1310 => "v1.3.10",
            ZkSolcVersion::V1311 => "v1.3.11",
            ZkSolcVersion::V1313 => "v1.3.13",
            ZkSolcVersion::V1314 => "v1.3.14",
            ZkSolcVersion::V1316 => "v1.3.16",
            ZkSolcVersion::V1317 => "v1.3.17",
            ZkSolcVersion::V1318 => "v1.3.18",
            ZkSolcVersion::V1319 => "v1.3.19",
            ZkSolcVersion::V1321 => "v1.3.21",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
enum ZkSolcOS {
    Linux,
    MacAMD,
    MacARM,
}

fn get_operating_system() -> Result<ZkSolcOS> {
    match std::env::consts::OS {
        "linux" => Ok(ZkSolcOS::Linux),
        "macos" | "darwin" => match std::env::consts::ARCH {
            "aarch64" => Ok(ZkSolcOS::MacARM),
            _ => Ok(ZkSolcOS::MacAMD),
        },
        _ => Err(Error::msg(format!("Unsupported operating system {}", std::env::consts::OS))),
    }
}

impl ZkSolcOS {
    fn get_compiler(&self) -> &str {
        match self {
            ZkSolcOS::Linux => "zksolc-linux-amd64-musl-",
            ZkSolcOS::MacAMD => "zksolc-macosx-amd64-",
            ZkSolcOS::MacARM => "zksolc-macosx-arm64-",
        }
    }

    fn get_download_uri(&self) -> &str {
        match self {
            ZkSolcOS::Linux => "linux-amd64-musl",
            ZkSolcOS::MacAMD => "macosx-amd64",
            ZkSolcOS::MacARM => "macosx-arm64",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ZkSolcManagerOpts {
    version: String,
}

impl ZkSolcManagerOpts {
    pub fn new(version: String) -> Self {
        Self { version }
    }
}
#[derive(Debug, Clone)]
pub struct ZkSolcManagerBuilder {
    _compilers_path: Option<PathBuf>,
    version: String,
    _compiler: Option<String>,
    download_url: Url,
}

impl ZkSolcManagerBuilder {
    pub fn new(opts: ZkSolcManagerOpts) -> Self {
        Self {
            _compilers_path: None,
            version: opts.version,
            _compiler: None,
            download_url: Url::parse(ZKSOLC_DOWNLOAD_BASE_URL).unwrap(),
        }
    }
    fn get_compiler(self) -> Result<String> {
        get_operating_system()
            .with_context(|| "Failed to determine OS for compiler")
            .map(|it| it.get_compiler().to_string())
    }
    pub fn build(self) -> Result<ZkSolcManager> {
        // TODO: try catching & returning errors quickly (rather than doing 'long' if and return
        // else at the end)
        let mut home_path =
            dirs::home_dir().ok_or(anyhow!("Could not build SolcManager - homedir not found"))?;
        home_path.push(".zksync");
        let version = self.version.to_string();
        let download_url = self.download_url.to_owned();
        let compiler = self.get_compiler()?;
        let compilers_path = home_path.to_owned();

        let solc_version = parse_version(&version)?;

        Ok(ZkSolcManager::new(compilers_path, solc_version, compiler, download_url))
    }
}

#[derive(Debug, Clone)]
pub struct ZkSolcManager {
    compilers_path: PathBuf,
    version: ZkSolcVersion,
    compiler: String,
    download_url: Url,
}

impl fmt::Display for ZkSolcManager {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "ZkSolcManager (
                compilers_path: {}, 
                version: {}, 
                compiler: {}, 
                download_url: {}, 
                compiler_name: {}, 
                full_download_url: {}
                exists: {}
            )",
            self.compilers_path.display(),
            self.version.get_version(),
            self.compiler,
            self.download_url,
            self.get_full_compiler(),
            self.get_full_download_url().unwrap(),
            self.exists(),
        )
    }
}

pub fn ensure_zksolc(zksolc_version: String) -> eyre::Result<PathBuf> {
    let zksolc_manager_opts = ZkSolcManagerOpts::new(zksolc_version.clone());
    let zksolc_manager_builder = ZkSolcManagerBuilder::new(zksolc_manager_opts);
    let zksolc_manager = zksolc_manager_builder.build().map_err(|e| {
        eyre::eyre!("Error initializing ZkSolcManager for version '{}': {}", zksolc_version, e)
    })?;

    if let Err(err) = zksolc_manager.check_setup_compilers_dir() {
        eyre::bail!("Failed to set up or access the ZkSolc compilers directory: {}", err);
    }

    if !zksolc_manager.exists() {
        let download_url = zksolc_manager
            .get_full_download_url()
            .map(|url| url.to_string())
            .unwrap_or_else(|_| "unknown URL".to_string());

        println!(
            "zksolc not found in `.zksync` directory. Downloading zksolc compiler from {}",
            download_url
        );
        zksolc_manager.download().map_err(|err| {
            eyre::eyre!(
                "Failed to download zksolc version '{}' from {}: {}",
                zksolc_version,
                download_url,
                err
            )
        })?;
    }

    Ok(zksolc_manager.get_full_compiler_path())
}

impl ZkSolcManager {
    pub fn new(
        compilers_path: PathBuf,
        version: ZkSolcVersion,
        compiler: String,
        download_url: Url,
    ) -> Self {
        Self { compilers_path, version, compiler, download_url }
    }
    pub fn get_full_compiler(&self) -> String {
        format!("{}{}", self.compiler, self.version.get_version())
    }
    pub fn get_full_download_url(&self) -> Result<Url> {
        let zk_solc_os = get_operating_system()
            .map_err(|err| anyhow!("Failed to determine OS to select the binary: {}", err))?;

        let download_uri = zk_solc_os.get_download_uri();

        // Using the GitHub releases URL pattern
        let full_download_url = format!(
            "https://github.com/matter-labs/zksolc-bin/releases/download/{}/zksolc-{}-{}",
            self.version.get_version(),
            download_uri,
            self.version.get_version()
        );

        Url::parse(&full_download_url)
            .map_err(|err| anyhow!("Could not parse URL for binary download: {}", err))
    }

    pub fn get_full_compiler_path(&self) -> PathBuf {
        self.compilers_path.join(self.clone().get_full_compiler())
    }

    pub fn exists(&self) -> bool {
        let compiler_path = self.compilers_path.join(self.clone().get_full_compiler());

        fs::metadata(compiler_path)
            .map(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o755 != 0)
            .unwrap_or(false)
    }
    pub fn check_setup_compilers_dir(&self) -> Result<()> {
        if !self.compilers_path.exists() {
            fs::create_dir_all(&self.compilers_path)
                .map_err(|e| Error::msg(format!("Could not create compilers path: {}", e)))?;
        }
        Ok(())
    }
    pub fn download(&self) -> Result<()> {
        if self.exists() {
            // TODO: figure out better don't download if compiler is downloaded
            return Ok(())
        }

        let url = self
            .get_full_download_url()
            .map_err(|e| Error::msg(format!("Could not get full download url: {}", e)))?;

        let client = Client::new();
        let mut response = client
            .get(url)
            .send()
            .map_err(|e| Error::msg(format!("Failed to download file: {}", e)))?;

        if response.status().is_success() {
            let mut output_file = File::create(self.get_full_compiler_path())
                .map_err(|e| Error::msg(format!("Failed to create output file: {}", e)))?;

            copy(&mut response, &mut output_file)
                .map_err(|e| Error::msg(format!("Failed to write the downloaded file: {}", e)))?;

            let compiler_path = self.compilers_path.join(self.get_full_compiler());
            fs::set_permissions(compiler_path, PermissionsExt::from_mode(0o755)).map_err(|e| {
                Error::msg(format!("Failed to set zksync compiler permissions: {e}"))
            })?;
        } else {
            return Err(Error::msg(format!(
                "Failed to download file: status code {}",
                response.status()
            )))
        }
        Ok(())
    }
}
