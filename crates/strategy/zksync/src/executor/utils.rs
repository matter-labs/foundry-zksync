use alloy_rpc_types::serde_helpers::OtherFields;
use foundry_zksync_core::{ZkTransactionMetadata, ZKSYNC_TRANSACTION_OTHER_FIELDS_KEY};

pub fn try_get_zksync_transaction_metadata(
    other_fields: &OtherFields,
) -> Option<ZkTransactionMetadata> {
    other_fields
        .get_deserialized::<ZkTransactionMetadata>(ZKSYNC_TRANSACTION_OTHER_FIELDS_KEY)
        .transpose()
        .ok()
        .flatten()
}
