//! Receipt types for RPC

use seismic_alloy_consensus::SeismicReceiptEnvelope;

/// Seismic transaction receipt type
pub type SeismicTransactionReceipt<T = SeismicReceiptEnvelope<alloy_rpc_types_eth::Log>> =
    alloy_rpc_types_eth::TransactionReceipt<T>;

#[cfg(test)]
mod tests {
    use crate::receipt::SeismicTransactionReceipt;

    // <https://github.com/alloy-rs/op-alloy/issues/18>
    #[test]
    fn parse_rpc_receipt() {
        let s = r#"{
        "blockHash": "0x9e6a0fb7e22159d943d760608cc36a0fb596d1ab3c997146f5b7c55c8c718c67",
        "blockNumber": "0x6cfef89",
        "contractAddress": null,
        "cumulativeGasUsed": "0xfa0d",
        "effectiveGasPrice": "0x0",
        "from": "0xdeaddeaddeaddeaddeaddeaddeaddeaddead0001",
        "gasUsed": "0xfa0d",
        "logs": [],
        "logsBloom": "0x00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000",
        "status": "0x1",
        "to": "0x4200000000000000000000000000000000000015",
        "transactionHash": "0xb7c74afdeb7c89fb9de2c312f49b38cb7a850ba36e064734c5223a477e83fdc9",
        "transactionIndex": "0x0",
        "type": "0x4a"
    }"#;

        let receipt: SeismicTransactionReceipt = serde_json::from_str(s).unwrap();
        let value = serde_json::to_value(&receipt).unwrap();
        let expected_value = serde_json::from_str::<serde_json::Value>(s).unwrap();
        assert_eq!(value, expected_value);
    }
}
