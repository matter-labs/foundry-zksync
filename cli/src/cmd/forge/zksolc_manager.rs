use std::{env, path::PathBuf, os, fs, str::FromStr};
use semver::Version;
use url::Url;
use dirs;

const ZKSOLC_DOWNLOAD_ROOT: String = "https://github.com/matter-labs/zksolc-bin/raw/main/".to_owned();

// TODO: fix this ragged pirate stuff
const ZKSOLC_COMPILERS_PATH: String = "/.zksync".to_owned();

enum ZkSolcOS {
    Linux,
    Osx,
}

impl ZkSolcOS {
    fn get_compiler(&self) -> &str {
        match self {
            ZkSolc::Linux => "zksolc-linux-amd64-musl-",
            ZkSolc::Osx => "zksolc-macosx-arm64-",
        }
    }
}

pub struct ZkSolcManagerOpts {
    version: Version,
}

pub struct ZkSolcManagerBuilder {
    compilers_path: Option<PathBuf>,
    version: Version,
    compiler: Option<String>,
}

pub struct ZkSolcManager {
    compilers_path: PathBuf,
    version: Version,
    compiler: String,
}

impl ZkSolcManagerBuilder {
    pub fn new(mut self, opts: ZkSolcManagerOpts) -> Self {
        Self {
            compilers_path: None,
            version: opts.version,
            compiler: None,
        }
    }

    fn get_compiler(self, version: &String) -> String {
        let os_type = std::env::var_os("CARGO_CFG_TARGET_OS").ok_or("Unable to determine OS type")?;

        let zk_solc_os = match os_type.to_str().ok_or("Unable to convert OS type to string")? {
            "linux" => ZkSolc::Linux,
            "macos" => ZkSolc::Osx,
            _ => return Err(format!("Unsupported OS type: {}", os_type.to_str().unwrap()).into()),
        };

        let compiler = zk_sync_os.get_compiler();
        compiler
    }

    pub fn build(self) -> Result<ZkSolcManager> {
        if let Some(home_path) = dirs::home_dir() {
            let compilers_path = home_path.push(&ZKSOLC_COMPILERS_PATH)?;
            if let Some(compiler) = self.get_compiler(self.version) {
                Ok(ZkSolcManager { compilers_path, version, compiler })   
            }
        };
        Err(Error::other("Could not build ZkSolcManager"))
    }
}

impl ZkSolcManager {
    pub fn exists(self) -> bool {
        let full_path = path.join(self.compiler);
        if let Ok(metadata) = fs::metadata(self.os_path) {
            if metadata.is_file() && metadata.permissions().mode() & 0o755 != 0 {
                return true;
            }
        }
        false
    }

    pub fn download(self) -> Result<()> {
        let base_url = Url::parse(ZKSOLC_DOWNLOAD_ROOT)?;
        let url = base_url.join(compiler)?;
        
        let mut downloader = Downloader::new();

        let request = RequestBuilder::get(url)
            .build()
            .map_err(DownloadError::from)?;

        let response = downloader.download(request).await
            .map_err(|err| DownloadError::DownloadError(err.to_string()))?;

        let mut file = std::fs::File::create(&self.compilers_path)
            .map_err(|err| DownloadError::FileCreationError(err.to_string()))?;
        
        file.write_all(&response.body)
            .map_err(|err| DownloadError::FileWriteError(err.to_string()))?;
        
        fs::set_permissions(&zksolc_path, PermissionsExt::from_mode(0o755))
            .map_err(|e| Error::other(format!("failed to set zksync compiler permissions: {e}")));

        Ok(())
    }
}
