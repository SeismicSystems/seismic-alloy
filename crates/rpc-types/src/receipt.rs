//! Receipt types for RPC

use alloy_consensus::{Receipt, ReceiptWithBloom};
use alloy_serde::OtherFields;
use seismic_alloy_consensus::SeismicReceiptEnvelope;
use serde::{Deserialize, Serialize};

pub type SeismicTransactionReceipt =
    alloy_rpc_types_eth::TransactionReceipt<SeismicReceiptEnvelope<alloy_rpc_types_eth::Log>>;

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString;
    use serde_json::{json, Value};

    // <https://github.com/alloy-rs/op-alloy/issues/18>
    #[test]
    fn parse_rpc_receipt() {
        let s = r#"{
        "blockHash": "0x9e6a0fb7e22159d943d760608cc36a0fb596d1ab3c997146f5b7c55c8c718c67",
        "blockNumber": "0x6cfef89",
        "contractAddress": null,
        "cumulativeGasUsed": "0xfa0d",
        "depositNonce": "0x8a2d11",
        "effectiveGasPrice": "0x0",
        "from": "0xdeaddeaddeaddeaddeaddeaddeaddeaddead0001",
        "gasUsed": "0xfa0d",
        "logs": [],
        "logsBloom": "0x00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000",
        "status": "0x1",
        "to": "0x4200000000000000000000000000000000000015",
        "transactionHash": "0xb7c74afdeb7c89fb9de2c312f49b38cb7a850ba36e064734c5223a477e83fdc9",
        "transactionIndex": "0x0",
        "type": "0x7e",
        "l1GasPrice": "0x3ef12787",
        "l1GasUsed": "0x1177",
        "l1Fee": "0x5bf1ab43d",
        "l1BaseFeeScalar": "0x1",
        "l1BlobBaseFee": "0x600ab8f05e64",
        "l1BlobBaseFeeScalar": "0x1"
    }"#;

        let receipt: SeismicTransactionReceipt = serde_json::from_str(s).unwrap();
        let value = serde_json::to_value(&receipt).unwrap();
        let expected_value = serde_json::from_str::<serde_json::Value>(s).unwrap();
        assert_eq!(value, expected_value);
    }

    #[test]
    fn serialize_empty_optimism_transaction_receipt_fields_struct() {
        let op_fields = OpTransactionReceiptFields::default();

        let json = serde_json::to_value(op_fields).unwrap();
        assert_eq!(json, json!({}));
    }

    #[test]
    fn serialize_l1_fee_scalar() {
        let op_fields = OpTransactionReceiptFields {
            l1_block_info: L1BlockInfo {
                l1_fee_scalar: Some(0.678),
                ..Default::default()
            },
            ..Default::default()
        };

        let json = serde_json::to_value(op_fields).unwrap();

        assert_eq!(
            json["l1FeeScalar"],
            serde_json::Value::String("0.678".to_string())
        );
    }

    #[test]
    fn deserialize_l1_fee_scalar() {
        let json = json!({
            "l1FeeScalar": "0.678"
        });

        let op_fields: OpTransactionReceiptFields = serde_json::from_value(json).unwrap();
        assert_eq!(op_fields.l1_block_info.l1_fee_scalar, Some(0.678f64));

        let json = json!({
            "l1FeeScalar": Value::Null
        });

        let op_fields: OpTransactionReceiptFields = serde_json::from_value(json).unwrap();
        assert_eq!(op_fields.l1_block_info.l1_fee_scalar, None);

        let json = json!({});

        let op_fields: OpTransactionReceiptFields = serde_json::from_value(json).unwrap();
        assert_eq!(op_fields.l1_block_info.l1_fee_scalar, None);
    }
}
