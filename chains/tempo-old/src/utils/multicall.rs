use anyhow::Result as AnyhowResult;
use ethers::prelude::*;
use std::sync::Arc;

// Multicall3 ABI (partial)
ethers::contract::abigen!(
    IMulticall3,
    r#"[
        struct Call3 { address target; bool allowFailure; bytes callData; }
        struct Result { bool success; bytes returnData; }
        function aggregate3(Call3[] calls) payable returns (Result[] returnData)
    ]"#
);

pub struct BatchHelper<M> {
    contract: IMulticall3<M>,
}

impl<M: Middleware + 'static> BatchHelper<M> {
    pub fn new(client: Arc<M>) -> Self {
        // Canonical Multicall3
        let address = "0xcA11bde05977b3631167028862bE2a173976CA11"
            .parse::<Address>()
            .unwrap();
        Self {
            contract: IMulticall3::new(address, client),
        }
    }

    pub async fn execute_batch(
        &self,
        calls: Vec<(Address, Bytes)>,
    ) -> AnyhowResult<Option<TransactionReceipt>> {
        let call3_requests: Vec<Call3> = calls
            .into_iter()
            .map(|(target, data)| {
                Call3 {
                    target,
                    allow_failure: false, // We want strict execution for now
                    call_data: data,
                }
            })
            .collect();

        // Send transaction
        let tx = self.contract.aggregate_3(call3_requests);
        let pending_tx = tx.send().await?;
        let receipt = pending_tx.await?;

        Ok(receipt)
    }
}
