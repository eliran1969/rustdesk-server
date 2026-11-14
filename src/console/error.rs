use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use hbb_common::log;
use serde_json::json;

#[derive(Debug)]
#[allow(dead_code)]
pub(crate) enum ApiError {
    BadRequest(String),
    Unauthorized,
    Forbidden,
    NotFound,
    TooManyRequests,
    Internal(String),
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, msg) = match self {
            ApiError::BadRequest(m) => (StatusCode::BAD_REQUEST, m),
            ApiError::Unauthorized => (StatusCode::UNAUTHORIZED, "Unauthorized".to_owned()),
            ApiError::Forbidden => (StatusCode::FORBIDDEN, "Forbidden".to_owned()),
            ApiError::NotFound => (StatusCode::NOT_FOUND, "Not found".to_owned()),
            ApiError::TooManyRequests => (
                StatusCode::TOO_MANY_REQUESTS,
                "Too many requests".to_owned(),
            ),
            ApiError::Internal(detail) => {
                log::error!("console internal error: {}", detail);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Internal error".to_owned(),
                )
            }
        };
        (status, Json(json!({ "error": msg }))).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::console::test_util::body_string;

    #[tokio::test]
    async fn error_variants_map_to_status_and_json_body() {
        let cases = [
            (ApiError::BadRequest("bad".into()), 400, r#"{"error":"bad"}"#),
            (ApiError::Unauthorized, 401, r#"{"error":"Unauthorized"}"#),
            (ApiError::Forbidden, 403, r#"{"error":"Forbidden"}"#),
            (ApiError::NotFound, 404, r#"{"error":"Not found"}"#),
            (ApiError::TooManyRequests, 429, r#"{"error":"Too many requests"}"#),
            (ApiError::Internal("secret".into()), 500, r#"{"error":"Internal error"}"#),
        ];
        for (err, status, body) in cases {
            let res = err.into_response();
            assert_eq!(res.status().as_u16(), status);
            assert_eq!(body_string(res).await, body);
        }
    }
}
