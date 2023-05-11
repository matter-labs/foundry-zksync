use anyhow::{anyhow, Error, Result};
use dirs;
use reqwest::blocking::Client;
use serde::Serialize;
use std::fs::File;
use std::io::copy;
use std::{fmt, fs, os::unix::prelude::PermissionsExt, path::PathBuf};
use url::Url;

const ZKSOLC_DOWNLOAD_BASE_URL: &str = "https://github.com/matter-labs/zksolc-bin/raw/main";

#[derive(Debug, Clone, Serialize)]
pub enum ZkSolcVersion {
    V135,
    V136,
    V137,
    V138,
    V139,
}

fn parse_version(version: &str) -> Result<ZkSolcVersion> {
    match version {
        "v1.3.5" => Ok(ZkSolcVersion::V135),
        "v1.3.6" => Ok(ZkSolcVersion::V136),
        "v1.3.7" => Ok(ZkSolcVersion::V137),
        "v1.3.8" => Ok(ZkSolcVersion::V138),
        "v1.3.9" => Ok(ZkSolcVersion::V139),
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
        }
    }
}

#[derive(Debug, Clone, Serialize)]
enum ZkSolcOS {
    Linux,
    Mac,
}

fn get_operating_system() -> Result<ZkSolcOS> {
    match std::env::consts::OS {
        "linux" => Ok(ZkSolcOS::Linux),
        "macos" | "darwin" => Ok(ZkSolcOS::Mac),
        _ => Err(Error::msg("Unsupported operating system")),
    }
}

impl ZkSolcOS {
    fn get_compiler(&self) -> &str {
        match self {
            ZkSolcOS::Linux => "zksolc-linux-amd64-musl-",
            ZkSolcOS::Mac => "zksolc-macosx-amd64-",
        }
    }

    fn get_download_uri(&self) -> &str {
        match self {
            ZkSolcOS::Linux => "linux-amd64",
            ZkSolcOS::Mac => "macosx-amd64",
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

#[derive(Debug, Clone, Serialize)]
pub struct ZkSolcManagerBuilder {
    compilers_path: Option<PathBuf>,
    version: String,
    compiler: Option<String>,
    download_url: Url,
}

impl ZkSolcManagerBuilder {
    pub fn new(opts: ZkSolcManagerOpts) -> Self {
        Self {
            compilers_path: None,
            version: opts.version,
            compiler: None,
            download_url: Url::parse(ZKSOLC_DOWNLOAD_BASE_URL).unwrap(),
        }
    }

    /// FIXME: do you really need to return a string (which is more like a 'mutable' object) - or would &str be enough?
    fn get_compiler(self) -> Result<String> {
        if let Ok(zk_solc_os) = get_operating_system() {
            let compiler = zk_solc_os.get_compiler().to_string();
            Ok(compiler)
        } else {
            Err(Error::msg("Could not determine compiler"))
        }
    }

    pub fn build(self) -> Result<ZkSolcManager> {
        // TODO: try catching & returning errors quickly (rather than doing 'long' if and return else at the end)
        let mut home_path =
            dirs::home_dir().ok_or(anyhow!("Could not build SolcManager - homedir not found"))?;
        home_path.push(".zksync");
        let version = self.version.to_string();
        let download_url = self.download_url.to_owned();
        let compiler = self.get_compiler()?;
        // FIXME: PathBuf (like String) - is more like a builder.. so when you 'built' your compiler's path
        // you might want it to be just a 'path'  (by calling 'as_path()')
        let compilers_path = home_path.to_owned();

        let solc_version = parse_version(&version)?;
        return Ok(ZkSolcManager::new(compilers_path, solc_version, compiler, download_url));
    }
}

#[derive(Debug, Clone, Serialize)]
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
    pub fn new(
        compilers_path: PathBuf,
        version: ZkSolcVersion,
        compiler: String,
        download_url: Url,
    ) -> Self {
        Self { compilers_path, version, compiler, download_url }
    }

    pub fn get_full_compiler(&self) -> String {
        return format!("{}{}", self.compiler, self.version.get_version());
    }

    pub fn get_full_download_url(&self) -> Result<Url> {
        // TODO: this is an example, of how you can 'propagate' the error from below, and add some local context information.
        let zk_solc_os = get_operating_system()
            .map_err(|err| anyhow!("Failed to determine OS to select the binary: {}", err))?;

        let download_uri = zk_solc_os.get_download_uri();

        let full_download_url =
            format!("{}/{}/{}", self.download_url, download_uri, self.get_full_compiler());

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
            return Ok(());
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

            // FIXME: there is a performance issue in the line below
            let compiler_path = self.compilers_path.join(self.clone().get_full_compiler());
            // FIXME: This is a bug - can you spot it? (usually if you need to use 'let _' - then something risky might be going on)
            let _ = fs::set_permissions(compiler_path, PermissionsExt::from_mode(0o755))
                .map_err(|e| Error::msg(format!("Failed to set zksync compiler permissions: {e}")));
        } else {
            return Err(Error::msg(format!(
                "Failed to download file: status code {}",
                response.status()
            )));
        }
        Ok(())
    }
}
