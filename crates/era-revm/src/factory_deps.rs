use std::str::FromStr;

use zksync_basic_types::H256;

/// Factory deps packer.
///
/// EVM assumes that all the necessary bytecodes (factory deps) are present within the original bytecode.
/// In case of Era - they are actually returned separate from the compiler.
///
/// So in order to fit to the REVM / Forge - we "serialize" all the factory deps into
/// one huge "fake" bytecode string - and then pass them around.

/// Struct with the contract bytecode, and all the other factory deps.
#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct PackedEraBytecode {
    hash: String,
    bytecode: String,
    factory_deps: Vec<String>,
}

impl PackedEraBytecode {
    pub fn new(hash: String, bytecode: String, factory_deps: Vec<String>) -> Self {
        Self {
            hash,
            bytecode,
            factory_deps,
        }
    }
    pub fn to_vec(&self) -> Vec<u8> {
        serde_json::to_vec(self).unwrap()
    }
    pub fn from_vec(input: &[u8]) -> Self {
        serde_json::from_slice(input).unwrap()
    }
    pub fn bytecode(&self) -> Vec<u8> {
        hex::decode(self.bytecode.clone()).unwrap()
    }
    pub fn bytecode_hash(&self) -> H256 {
        let h = hash_bytecode(&self.bytecode());
        assert_eq!(h, H256::from_str(&self.hash).unwrap());
        h
    }
    pub fn factory_deps(&self) -> Vec<Vec<u8>> {
        self.factory_deps
            .iter()
            .chain([&self.bytecode])
            .map(|entry| hex::decode(entry).unwrap())
            .collect()
    }
}

fn ensure_chunkable(bytes: &[u8]) {
    assert!(
        bytes.len() % 32 == 0,
        "Bytes must be divisible by 32 to split into chunks"
    );
}

pub fn bytes_to_chunks(bytes: &[u8]) -> Vec<[u8; 32]> {
    ensure_chunkable(bytes);
    bytes
        .chunks(32)
        .map(|el| {
            let mut chunk = [0u8; 32];
            chunk.copy_from_slice(el);
            chunk
        })
        .collect()
}

pub fn hash_bytecode(code: &[u8]) -> H256 {
    let chunked_code = bytes_to_chunks(code);
    let hash = zk_evm::zkevm_opcode_defs::utils::bytecode_to_code_hash(&chunked_code)
        .expect("Invalid bytecode");

    H256(hash)
}
