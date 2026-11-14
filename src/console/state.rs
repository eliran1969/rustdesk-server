use super::auth::LoginLimiter;
use crate::peer::PeerMap;
use std::sync::Arc;

#[derive(Clone)]
pub(crate) struct ConsoleState {
    #[allow(dead_code)]
    pub db: sqlx::SqlitePool,
    #[allow(dead_code)]
    pub pm: PeerMap,
    pub limiter: Arc<LoginLimiter>,
}
