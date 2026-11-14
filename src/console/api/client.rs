use axum::Json;
use serde_json::{json, Value};

pub(crate) async fn login_options() -> Json<Value> {
    Json(json!([]))
}

#[cfg(test)]
mod tests {
    use crate::console::test_util::{body_string, test_app};
    use axum::{body::Body, http::Request};
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
    async fn bind_fails_cleanly_when_port_in_use() {
        let hold = std::net::TcpListener::bind("0.0.0.0:0").unwrap();
        let port = hold.local_addr().unwrap().port();
        let addr = std::net::SocketAddr::from(([0, 0, 0, 0], port));
        assert!(crate::console::bind(addr).is_err());
        drop(hold);
    }
}
