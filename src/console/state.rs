use crate::peer::PeerMap;

#[derive(Clone)]
pub(crate) struct ConsoleState {
    #[allow(dead_code)]
    pub db: sqlx::SqlitePool,
    #[allow(dead_code)]
    pub pm: PeerMap,
}
