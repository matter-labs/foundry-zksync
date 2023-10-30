/// The `ZkSolcManager` module manages the downloading and setup of the `zksolc` Solidity
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

/// `ZkSolcVersion` is an enumeration of the supported versions of the `zksolc` compiler.
///
/// Each variant in this enum represents a specific version of the `zksolc` compiler:
/// * `V138` corresponds to version 1.3.8 of the `zksolc` compiler.
/// * `V139` corresponds to version 1.3.9 of the `zksolc` compiler.
///
/// The `get_version` method is provided to get a string representation of the version associated
/// with each enum variant.
///
/// This enumeration is used in the `ZkSolcManager` to specify the `zksolc` compiler version to be
/// used for contract compilation.
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
}

pub const DEFAULT_ZKSOLC_VERSION: &str = "v1.3.16";

/// `parse_version` parses a string representation of a `zksolc` compiler version
/// and returns the `ZkSolcVersion` enum variant if it matches a supported version.
///
/// # Arguments
///
/// * `version`: A string slice of the `zksolc` version to parse.
///
/// # Returns
///
/// A `Result` with the `ZkSolcVersion` variant for the parsed version, or an `Err`
/// if the version isn't supported.
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
        _ => Err(Error::msg(
            "ZkSolc compiler version not supported. Proper version format: 'v1.3.x'",
        )),
    }
}

impl ZkSolcVersion {
    /// `get_version` returns a string slice representing the `ZkSolcVersion` variant.
    ///
    /// # Returns
    ///
    /// A string slice representing the `ZkSolcVersion` variant.
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
        }
    }
}

#[derive(Debug, Clone, Serialize)]
enum ZkSolcOS {
    Linux,
    MacAMD,
    MacARM,
}

/// `get_operating_system` identifies the current operating system and returns the corresponding
/// `ZkSolcOS` variant.
///
/// # Returns
///
/// `ZkSolcOS` variant representing the current operating system.
///
/// # Errors
///
/// If the operating system is not supported, it returns an error.
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
    /// `get_compiler` provides the appropriate compiler string based on the current operating
    /// system.
    ///
    /// # Returns
    ///
    /// A string representing the compiler, depending on the operating system.
    ///
    /// # Note
    ///
    /// This function is used to construct the filename for the zkSync compiler binary.
    fn get_compiler(&self) -> &str {
        match self {
            ZkSolcOS::Linux => "zksolc-linux-amd64-musl-",
            ZkSolcOS::MacAMD => "zksolc-macosx-amd64-",
            ZkSolcOS::MacARM => "zksolc-macosx-arm64-",
        }
    }

    /// `get_download_uri` provides the appropriate URI for downloading the compiler binary based on
    /// the current operating system.
    ///
    /// # Returns
    ///
    /// A string representing the URI, depending on the operating system.
    ///
    /// # Note
    ///
    /// This function is used to construct the URI for downloading the zkSync compiler binary.
    fn get_download_uri(&self) -> &str {
        match self {
            ZkSolcOS::Linux => "linux-amd64",
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

/// `ZkSolcManagerBuilder` is a structure that helps in creating a `ZkSolcManager` instance.
///
/// This builder pattern is used to encapsulate all the details needed to create a `ZkSolcManager`.
/// The fields include:
///
/// * `compilers_path`: An optional `PathBuf` that denotes the directory where the compiler binaries
///   are stored.
/// * `version`: A string that represents the version of the zkSync compiler to be used.
/// * `compiler`: An optional string that describes the compiler name.
/// * `download_url`: The base URL from where the zkSync compiler binary is to be downloaded.
///
/// The builder provides a `new` function to initialize the structure with `ZkSolcManagerOpts`, and
/// a `build` function which constructs and returns a `ZkSolcManager` instance.
///
/// The `get_compiler` function is used to determine the appropriate compiler name based on the
/// operating system.
///
/// The builder follows a fluent interface design, allowing the setting of properties in a chained
/// manner. The `build` function should be the final method invoked after all necessary properties
/// have been set.
///
/// # Example
///
/// ```ignore
/// use zkforge::zksolc_manager::{ZkSolcManagerBuilder, ZkSolcManagerOpts};
/// let opts = ZkSolcManagerOpts::new("v1.3.16")
/// let zk_solc_manager = ZkSolcManagerBuilder::new(opts)
///     .build()
///     .expect("Failed to build ZkSolcManager");
/// ```
#[derive(Debug, Clone)]
pub struct ZkSolcManagerBuilder {
    _compilers_path: Option<PathBuf>,
    version: String,
    _compiler: Option<String>,
    download_url: Url,
}

impl ZkSolcManagerBuilder {
    /// Constructs a new instance of `ZkSolcManagerBuilder` using the provided `ZkSolcManagerOpts`.
    ///
    /// This function takes a `ZkSolcManagerOpts` instance as a parameter, which contains the zkSync
    /// compiler version as a string. The builder's `version` field is initialized with this
    /// version. The `compilers_path` and `compiler` fields are initialized as `None` since they
    /// are optional and can be set later.
    ///
    /// The `download_url` field is initialized with the constant `ZKSOLC_DOWNLOAD_BASE_URL`, which
    /// is the base URL from where the compiler binary will be downloaded.
    ///
    /// This function should be used as the starting point for creating a `ZkSolcManager` instance
    /// using the builder pattern.
    ///
    /// # Parameters
    ///
    /// * `opts`: A `ZkSolcManagerOpts` instance containing the zkSync compiler version.
    ///
    /// # Returns
    ///
    /// Returns a new `ZkSolcManagerBuilder` instance.
    pub fn new(opts: ZkSolcManagerOpts) -> Self {
        Self {
            _compilers_path: None,
            version: opts.version,
            _compiler: None,
            download_url: Url::parse(ZKSOLC_DOWNLOAD_BASE_URL).unwrap(),
        }
    }

    /// Returns the appropriate compiler string based on the current operating system.
    ///
    /// This function determines the current operating system using `get_operating_system`, and
    /// returns the corresponding compiler string based on the operating system. The compiler
    /// string is used to construct the filename for the `zksolc` compiler binary.
    ///
    /// # Returns
    ///
    /// Returns a `Result<String>` representing the compiler string based on the current operating
    /// system.
    ///
    /// # Errors
    ///
    /// This function can return an `Err` if the operating system cannot be determined using
    /// `get_operating_system`.
    fn get_compiler(self) -> Result<String> {
        get_operating_system()
            .with_context(|| "Failed to determine OS for compiler")
            .map(|it| it.get_compiler().to_string())
    }

    /// `build` constructs and returns a `ZkSolcManager` instance based on the provided
    /// configuration options.
    ///
    /// This function finalizes the construction of a `ZkSolcManager` instance using the builder
    /// pattern. It validates the provided options, resolves the necessary details, and creates
    /// the manager instance.
    ///
    /// The function performs the following steps:
    /// 1. Obtains the home directory path and appends the `.zksync` directory to it, which
    ///    represents the compilers directory.
    /// 2. Parses the provided version string and verifies if it matches one of the supported
    ///    `ZkSolcVersion` variants.
    /// 3. Determines the appropriate compiler string based on the current operating system using
    ///    the `get_compiler` function.
    /// 4. Constructs a `ZkSolcManager` instance with the resolved compilers directory, version,
    ///    compiler, and download URL.
    ///
    /// # Returns
    ///
    /// A `Result` containing the constructed `ZkSolcManager` instance if the build process is
    /// successful, or an `Err` if any errors occur.
    ///
    /// # Errors
    ///
    /// The function can return an `Err` in the following cases:
    /// * If the home directory path cannot be determined.
    /// * If the provided version string cannot be parsed into a valid `ZkSolcVersion` variant.
    /// * If the current operating system is not supported or cannot be determined.
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

/// `ZkSolcManager` is a structure that manages a specific version of the `zksolc` Solidity
/// compiler.
///
/// This structure encapsulates the functionality to interact with different versions of the
/// `zksolc` compiler, including downloading, setting up, and checking for the existence of the
/// compiler. It provides an abstraction for managing the `zksolc` compiler, making it easier for
/// developers to use different versions without dealing with the low-level details.
///
/// # Fields
///
/// * `compilers_path`: A `PathBuf` representing the directory where the compiler binaries are
///   stored.
/// * `version`: A `ZkSolcVersion` representing the specific version of the `zksolc` compiler
///   managed by this instance.
/// * `compiler`: A `String` representing the compiler name.
/// * `download_url`: A `Url` representing the base URL from which the `zksolc` compiler binary is
///   downloaded.
///
/// # Example
///
/// ```ignore
/// let compilers_path = PathBuf::from("/path/to/compilers");
/// let version = ZkSolcVersion::V139;
/// let compiler = "zksolc-linux-amd64-musl-v1.3.9".to_string();
/// let download_url = Url::parse("https://github.com/matter-labs/zksolc-bin/raw/main").unwrap();
///
/// let zksolc_manager = ZkSolcManager::new(compilers_path, version, compiler, download_url);
/// ```
///
/// The example above demonstrates the creation of a `ZkSolcManager` instance. It specifies the
/// `compilers_path` where the compiler binaries are stored, the `version` of the `zksolc` compiler,
/// the `compiler` name, and the `download_url` for the compiler binary.
///
/// # Implementation Details
///
/// The `ZkSolcManager` structure provides several methods to interact with the `zksolc` compiler:
///
/// * `get_full_compiler()`: Returns a `String` representing the full compiler name, including the
///   version.
/// * `get_full_download_url()`: Returns a `Result<Url>` representing the full download URL for the
///   compiler binary.
/// * `get_full_compiler_path()`: Returns a `PathBuf` representing the full path to the compiler
///   binary.
/// * `exists()`: Checks if the compiler binary exists in the compilers directory.
/// * `check_setup_compilers_dir()`: Checks and sets up the compilers directory if it doesn't exist.
/// * `download()`: Downloads the compiler binary if it doesn't already exist in the compilers
///   directory.
///
/// The `ZkSolcManager` structure provides a high-level interface to manage the `zksolc` compiler,
/// simplifying the process of handling different versions and ensuring the availability of the
/// compiler for contract compilation.
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
