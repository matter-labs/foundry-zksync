use crate::{error::PrivateKeyError, PendingSigner, WalletSigner};
use ethers_signers::{HDPath as LedgerHDPath, LocalWallet, TrezorHDPath, WalletError};
use eyre::{Context, Result};
use foundry_config::Config;
use std::{
    fs,
    path::{Path, PathBuf},
    str::FromStr,
};

/// Validates and sanitizes user inputs, returning configured [WalletSigner].
pub fn create_private_key_signer(private_key: &str) -> Result<WalletSigner> {
    let privk = private_key.trim().strip_prefix("0x").unwrap_or(private_key);
    match LocalWallet::from_str(privk) {
        Ok(pk) => Ok(WalletSigner::Local(pk)),
        Err(err) => {
            // helper closure to check if pk was meant to be an env var, this usually happens if
            // `$` is missing
            let ensure_not_env = |pk: &str| {
                // check if pk was meant to be an env var
                if !pk.starts_with("0x") && std::env::var(pk).is_ok() {
                    // SAFETY: at this point we know the user actually wanted to use an env var
                    // and most likely forgot the `$` anchor, so the
                    // `private_key` here is an unresolved env var
                    return Err(PrivateKeyError::ExistsAsEnvVar(pk.to_string()))
                }
                Ok(())
            };
            match err {
                WalletError::HexError(err) => {
                    ensure_not_env(private_key)?;
                    return Err(PrivateKeyError::InvalidHex(err).into());
                }
                WalletError::EcdsaError(_) => ensure_not_env(private_key)?,
                _ => {}
            };
            eyre::bail!("Failed to create wallet from private key: {err}")
        }
    }
}

/// Creates [WalletSigner] instance from given mnemonic parameters.
///
/// Mnemonic can be either a file path or a mnemonic phrase.
pub fn create_mnemonic_signer(
    mnemonic: &str,
    passphrase: Option<&str>,
    hd_path: Option<&str>,
    index: u32,
) -> Result<WalletSigner> {
    let mnemonic = if Path::new(mnemonic).is_file() {
        fs::read_to_string(mnemonic)?.replace('\n', "")
    } else {
        mnemonic.to_owned()
    };

    Ok(WalletSigner::from_mnemonic(&mnemonic, passphrase, hd_path, index)?)
}

/// Creates [WalletSigner] instance from given Ledger parameters.
pub async fn create_ledger_signer(
    hd_path: Option<&str>,
    mnemonic_index: u32,
) -> Result<WalletSigner> {
    let derivation = if let Some(hd_path) = hd_path {
        LedgerHDPath::Other(hd_path.to_owned())
    } else {
        LedgerHDPath::LedgerLive(mnemonic_index as usize)
    };

    WalletSigner::from_ledger_path(derivation).await.wrap_err_with(|| {
        "\
Could not connect to Ledger device.
Make sure it's connected and unlocked, with no other desktop wallet apps open."
    })
}

/// Creates [WalletSigner] instance from given Trezor parameters.
pub async fn create_trezor_signer(
    hd_path: Option<&str>,
    mnemonic_index: u32,
) -> Result<WalletSigner> {
    let derivation = if let Some(hd_path) = hd_path {
        TrezorHDPath::Other(hd_path.to_owned())
    } else {
        TrezorHDPath::TrezorLive(mnemonic_index as usize)
    };

    WalletSigner::from_trezor_path(derivation).await.wrap_err_with(|| {
        "\
Could not connect to Trezor device.
Make sure it's connected and unlocked, with no other conflicting desktop wallet apps open."
    })
}

pub fn maybe_get_keystore_path(
    maybe_path: Option<&str>,
    maybe_name: Option<&str>,
) -> Result<Option<PathBuf>> {
    let default_keystore_dir = Config::foundry_keystores_dir()
        .ok_or_else(|| eyre::eyre!("Could not find the default keystore directory."))?;
    Ok(maybe_path
        .map(PathBuf::from)
        .or_else(|| maybe_name.map(|name| default_keystore_dir.join(name))))
}

/// Creates keystore signer from given parameters.
///
/// If correct password or password file is provided, the keystore is decrypted and a [WalletSigner]
/// is returned.
///
/// Otherwise, a [PendingSigner] is returned, which can be used to unlock the keystore later,
/// prompting user for password.
pub fn create_keystore_signer(
    path: &PathBuf,
    maybe_password: Option<&str>,
    maybe_password_file: Option<&str>,
) -> Result<(Option<WalletSigner>, Option<PendingSigner>)> {
    if !path.exists() {
        eyre::bail!("Keystore file `{path:?}` does not exist")
    }

    if path.is_dir() {
        eyre::bail!(
            "Keystore path `{path:?}` is a directory. Please specify the keystore file directly."
        )
    }

    let password = match (maybe_password, maybe_password_file) {
        (Some(password), _) => Ok(Some(password.to_string())),
        (_, Some(password_file)) => {
            let password_file = Path::new(password_file);
            if !password_file.is_file() {
                Err(eyre::eyre!("Keystore password file `{password_file:?}` does not exist"))
            } else {
                Ok(Some(
                    fs::read_to_string(password_file)
                        .wrap_err_with(|| {
                            format!("Failed to read keystore password file at {password_file:?}")
                        })?
                        .trim_end()
                        .to_string(),
                ))
            }
        }
        (None, None) => Ok(None),
    }?;

    if let Some(password) = password {
        let wallet = LocalWallet::decrypt_keystore(path, password)
            .wrap_err_with(|| format!("Failed to decrypt keystore {path:?}"))?;
        Ok((Some(WalletSigner::Local(wallet)), None))
    } else {
        Ok((None, Some(PendingSigner::Keystore(path.clone()))))
    }
}
