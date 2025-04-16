use axum::{
    extract::{FromRequest, Multipart, Request},
    http::header,
    response::{IntoResponse, Response},
};
use zksync_basic_types::protocol_version::ProtocolSemanticVersion;
use zksync_object_store::StoredObject;
use zksync_proof_data_handler::ProcessorError;
use zksync_prover_interface::{
    api::ProofGenerationData, inputs::WitnessInputData, outputs::L1BatchProofForL1,
};

use crate::error::{ApiError, FileError};

#[derive(Debug)]
pub(crate) struct ProofGenerationDataResponse(pub ProofGenerationData);

impl IntoResponse for ProofGenerationDataResponse {
    fn into_response(self) -> Response {
        let l1_batch_number = self.0.l1_batch_number;
        let data = match <WitnessInputData as StoredObject>::serialize(&self.0.witness_input_data) {
            Ok(data) => data,
            Err(err) => {
                return ApiError::Processor(ProcessorError::Serialization(err)).into_response();
            }
        };

        let headers = [
            (header::CONTENT_TYPE, "application/octet-stream"),
            (
                header::CONTENT_DISPOSITION,
                &format!(
                    "attachment; filename=\"witness_inputs_{}.cbor\"",
                    l1_batch_number.0
                ),
            ),
        ];
        (headers, data).into_response()
    }
}

#[derive(Debug)]
pub(crate) struct ExternalProof {
    raw: Vec<u8>,
    protocol_version: ProtocolSemanticVersion,
}

impl ExternalProof {
    const FIELD_NAME: &'static str = "proof";
    const CONTENT_TYPE: &'static str = "application/octet-stream";

    pub fn protocol_version(&self) -> ProtocolSemanticVersion {
        self.protocol_version
    }

    pub fn raw(&self) -> Vec<u8> {
        self.raw.clone()
    }

    async fn extract_from_multipart<S: Send + Sync>(
        req: Request,
        state: &S,
    ) -> Result<Vec<u8>, FileError> {
        let mut multipart = Multipart::from_request(req, state).await?;

        let mut serialized_proof = vec![];
        while let Some(field) = multipart.next_field().await? {
            if field.name() == Some(Self::FIELD_NAME)
                && field.content_type() == Some(Self::CONTENT_TYPE)
            {
                serialized_proof = field.bytes().await?.to_vec();
                break;
            }
        }

        if serialized_proof.is_empty() {
            // No proof field found
            return Err(FileError::FileNotFound {
                field_name: Self::FIELD_NAME,
                content_type: Self::CONTENT_TYPE,
            });
        }

        Ok(serialized_proof)
    }
}

#[async_trait::async_trait]
impl<S: Send + Sync> FromRequest<S> for ExternalProof {
    type Rejection = ApiError;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        let serialized_proof = Self::extract_from_multipart(req, state).await?;
        let proof = <L1BatchProofForL1 as StoredObject>::deserialize(serialized_proof.clone())
            .map_err(|err| ApiError::Processor(ProcessorError::Serialization(err)))?;

        Ok(Self {
            raw: serialized_proof,
            protocol_version: proof.protocol_version(),
        })
    }
}
