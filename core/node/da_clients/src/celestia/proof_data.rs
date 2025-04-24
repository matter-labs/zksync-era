use eq_sdk::{
    get_keccak_inclusion_response::{
        ResponseValue as InclusionResponseValue, Status as InclusionResponseStatus,
    },
    types::BlobId,
    EqClient,
};
use zksync_da_client::DAError;

use crate::utils::{to_non_retriable_da_error, to_retriable_da_error};

pub(crate) async fn get_proof_data(
    eq_client: &EqClient,
    blob_id: &str,
) -> Result<Option<(Vec<u8>, Vec<u8>)>, DAError> {
    tracing::debug!("Parsing blob id: {}", blob_id);
    let blob_id_struct = blob_id
        .parse::<BlobId>()
        .map_err(to_non_retriable_da_error)?;

    let response = eq_client
        .get_keccak_inclusion(&blob_id_struct)
        .await
        .map_err(to_retriable_da_error)?;

    tracing::debug!("Got response from eq-service");
    let response_data: Option<InclusionResponseValue> = response
        .response_value
        .try_into()
        .map_err(to_non_retriable_da_error)?;
    tracing::debug!("response_data: {:?}", response_data);

    let response_status: InclusionResponseStatus = response
        .status
        .try_into()
        .map_err(to_non_retriable_da_error)?;
    tracing::debug!("response_status: {:?}", response_status);

    let proof_data = match response_status {
        InclusionResponseStatus::ZkpFinished => match response_data {
            Some(InclusionResponseValue::Proof(proof)) => proof,
            _ => {
                return Err(DAError {
                    error: anyhow::anyhow!("Complete status should be accompanied by a Proof, eq-service is broken"), 
                    is_retriable: false
                });
            }
        },
        InclusionResponseStatus::PermanentFailure => {
            return Err(DAError {
                error: anyhow::anyhow!("eq-service returned PermanentFailure"),
                is_retriable: false,
            });
        },
        InclusionResponseStatus::RetryableFailure => {
            return Err(DAError {
                error: anyhow::anyhow!("eq-service returned RetryableFailure"),
                is_retriable: true,
            });
        },
        _ => {
            tracing::debug!("eq-service returned non-complete status, returning None");
            return Ok(None);
        }
    };
    tracing::debug!("Got proof data from eq-service: {:?}", proof_data);

    let proof = proof_data.proof_data;
    let public_values = proof_data.public_values;
    Ok(Some((public_values, proof)))
} 