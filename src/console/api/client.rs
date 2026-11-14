use axum::Json;
use serde_json::{json, Value};

pub(crate) async fn login_options() -> Json<Value> {
    Json(json!([]))
}

#[cfg(test)]
mod tests {
    use crate::console::test_util::{body_string, test_app, test_pm};
    use axum::{body::Body, http::Request};
    use std::time::Duration;
    use tower::ServiceExt;

    #[tokio::test]
    async fn login_options_returns_empty_json_array() {
        let app = test_app().await;
        let res = app
            .oneshot(Request::get("/api/login-options").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(res.status(), 200);
        assert_eq!(body_string(res).await, "[]");
    }

    #[tokio::test]
    async fn start_survives_port_in_use() {
        let hold = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = hold.local_addr().unwrap().port();
        std::env::set_var("API_PORT", port.to_string());
        let r = tokio::time::timeout(
            Duration::from_secs(2),
            crate::console::start(test_pm().await, ":memory:".into()),
        )
        .await;
        assert!(r.is_ok());
        drop(hold);
    }
}
