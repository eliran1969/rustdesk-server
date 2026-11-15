pub(crate) mod account;
pub(crate) mod client;
pub(crate) mod device_groups;
pub(crate) mod devices;
pub(crate) mod peers;
pub(crate) mod user_groups;
pub(crate) mod users;

use super::error::ApiError;
use axum::{
    async_trait,
    body::HttpBody,
    extract::{FromRequest, RequestParts},
};
use hbb_common::bytes::BufMut;
use serde::de::DeserializeOwned;

const MAX_JSON_BODY: usize = 64 * 1024;

/// JSON request body that ignores `Content-Type` (the RustDesk client posts `/api/login`
/// as text/plain), caps the size, and reports problems as `{"error": ...}` 400s.
pub(crate) struct JsonBody<T>(pub T);

#[async_trait]
impl<T, B> FromRequest<B> for JsonBody<T>
where
    T: DeserializeOwned,
    B: HttpBody + Send + Unpin,
    B::Data: Send,
{
    type Rejection = ApiError;

    async fn from_request(req: &mut RequestParts<B>) -> Result<Self, Self::Rejection> {
        let invalid = || ApiError::BadRequest("Invalid request body".to_owned());
        let mut body = req.take_body().ok_or_else(invalid)?;
        let mut buf = Vec::new();
        while let Some(chunk) = body.data().await {
            let chunk = chunk.map_err(|_| invalid())?;
            if buf.len() + hbb_common::bytes::Buf::remaining(&chunk) > MAX_JSON_BODY {
                return Err(ApiError::BadRequest("Request body too large".to_owned()));
            }
            buf.put(chunk);
        }
        serde_json::from_slice(&buf)
            .map(JsonBody)
            .map_err(|_| invalid())
    }
}
