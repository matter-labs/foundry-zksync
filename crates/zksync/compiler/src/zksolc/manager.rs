#![allow(missing_docs)]
use anyhow::{anyhow, Context, Error, Result};
use reqwest::Client;
use serde::Serialize;
use std::{fmt, fs, os::unix::prelude::PermissionsExt, path::PathBuf};
use tokio::{fs::File, io::copy};
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
    V1322,
    V1323,
    V140,
    V1401,
}

pub const DEFAULT_ZKSOLC_VERSION: &str = "v1.4.1";

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
        "v1.3.22" => Ok(ZkSolcVersion::V1322),
        "v1.3.23" => Ok(ZkSolcVersion::V1323),
        "v1.4.0" => Ok(ZkSolcVersion::V140),
        "v1.4.1" => Ok(ZkSolcVersion::V1401),
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
            ZkSolcVersion::V1322 => "v1.3.22",
            ZkSolcVersion::V1323 => "v1.3.23",
            ZkSolcVersion::V140 => "v1.4.0",
            ZkSolcVersion::V1401 => "v1.4.1",
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

pub async fn setup_zksolc_manager(zksolc_version: String) -> eyre::Result<PathBuf> {
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
        zksolc_manager.download().await.map_err(|err| {
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
    /// Constructs a new instance of `ZkSolcManager` with the specified configuration options.
    ///
    /// This function creates a new `ZkSolcManager` instance with the provided parameters. It
    /// initializes the manager with the compilers directory path, the version of the `zksolc`
    /// compiler, the compiler name, and the download URL for the compiler binary.
    ///
    /// # Parameters
    ///
    /// * `compilers_path`: A `PathBuf` representing the directory where the compiler binaries are
    ///   stored.
    /// * `version`: A `ZkSolcVersion` representing the specific version of the `zksolc` compiler
    ///   managed by this instance.
    /// * `compiler`: A `String` representing the compiler name.
    /// * `download_url`: A `Url` representing the base URL from which the `zksolc` compiler binary
    ///   is downloaded.
    ///
    /// # Returns
    ///
    /// Returns a new `ZkSolcManager` instance with the specified configuration options.
    pub fn new(
        compilers_path: PathBuf,
        version: ZkSolcVersion,
        compiler: String,
        download_url: Url,
    ) -> Self {
        Self { compilers_path, version, compiler, download_url }
    }

    /// Returns the full name of the `zksolc` compiler, including the version.
    ///
    /// This function constructs and returns the full name of the `zksolc` compiler by combining the
    /// base compiler name with the specific version associated with the `ZkSolcManager`
    /// instance. The resulting string represents the complete name of the compiler, including
    /// the version.
    ///
    /// # Returns
    ///
    /// Returns a `String` representing the full name of the `zksolc` compiler, including the
    /// version.
    pub fn get_full_compiler(&self) -> String {
        format!("{}{}", self.compiler, self.version.get_version())
    }

    /// Returns the full download URL for the `zksolc` compiler binary based on the current
    /// operating system.
    ///
    /// This function constructs the full download URL for the `zksolc` compiler binary by combining
    /// the base download URL with the appropriate download URI based on the current operating
    /// system. The resulting URL represents the location from which the compiler binary can be
    /// downloaded.
    ///
    /// # Returns
    ///
    /// Returns a `Result<Url>` representing the full download URL for the `zksolc` compiler binary.
    ///
    /// # Errors
    ///
    /// This function can return an `Err` if the full download URL cannot be parsed into a valid
    /// `Url`.
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

    /// Checks if the `zksolc` compiler binary exists in the compilers directory.
    ///
    /// This function checks if the `zksolc` compiler binary file exists in the compilers directory
    /// specified by `compilers_path`. and checks if it is a regular file and has executable
    /// permissions.
    ///
    /// # Returns
    ///
    /// Returns a `bool` indicating whether the compiler binary exists in the compilers directory
    /// and has executable permissions.
    pub fn exists(&self) -> bool {
        let compiler_path = self.compilers_path.join(self.clone().get_full_compiler());

        fs::metadata(compiler_path)
            .map(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o755 != 0)
            .unwrap_or(false)
    }

    /// Checks if the compilers directory exists and creates it if it doesn't.
    ///
    /// This function checks if the compilers directory specified by `compilers_path` exists. If the
    /// directory doesn't exist, it creates the directory and any necessary parent directories
    /// using `fs::create_dir_all`.
    ///
    /// # Returns
    ///
    /// Returns a `Result<()>` indicating success if the compilers directory exists or is
    /// successfully created.
    ///
    /// # Errors
    ///
    /// This function can return an `Err` if an error occurs during the creation of the compilers
    /// directory, such as:
    /// * If the compilers directory path cannot be resolved.
    /// * If there is a failure in creating the directory structure using `fs::create_dir_all`.
    pub fn check_setup_compilers_dir(&self) -> Result<()> {
        if !self.compilers_path.exists() {
            fs::create_dir_all(&self.compilers_path)
                .map_err(|e| Error::msg(format!("Could not create compilers path: {}", e)))?;
        }
        Ok(())
    }

    /// Downloads the `zksolc` compiler binary if it doesn't already exist in the compilers
    /// directory.
    ///
    /// This function downloads the `zksolc` compiler binary from the specified download URL if it
    /// doesn't already exist in the compilers directory. It performs the following steps:
    /// 1. Checks if the compiler binary already exists in the compilers directory using the
    ///    `exists` function.
    /// 2. If the binary exists, the function returns early without performing any download.
    /// 3. If the binary doesn't exist, it sends a HTTP GET request to the download URL to retrieve
    ///    the binary.
    /// 4. If the download is successful, it creates the output file in the compilers directory and
    ///    writes the binary data to it.
    /// 5. Finally, it sets the appropriate permissions for the downloaded compiler binary.
    ///
    /// # Returns
    ///
    /// Returns a `Result<()>` indicating success if the download and setup process completes
    /// without any errors.
    ///
    /// # Errors
    ///
    /// This function can return an `Err` if any errors occur during the download or setup process,
    /// including:
    /// * If the download URL cannot be obtained using `get_full_download_url`.
    /// * If the HTTP GET request to the download URL fails.
    /// * If the output file cannot be created or written to.
    /// * If the permissions for the downloaded compiler binary cannot be set.
    pub async fn download(&self) -> Result<()> {
        if self.exists() {
            // TODO: figure out better don't download if compiler is downloaded
            return Ok(())
        }

        let url = self
            .get_full_download_url()
            .map_err(|e| Error::msg(format!("Could not get full download url: {}", e)))?;

        let client = Client::new();
        let response = client
            .get(url)
            .send()
            .await
            .map_err(|e| Error::msg(format!("Failed to download file: {}", e)))?;

        if response.status().is_success() {
            let mut output_file = File::create(self.get_full_compiler_path())
                .await
                .map_err(|e| Error::msg(format!("Failed to create output file: {}", e)))?;

            let content = response
                .bytes()
                .await
                .map_err(|e| Error::msg(format!("failed to download file: {}", e)))?;

            copy(&mut content.as_ref(), &mut output_file)
                .await
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
