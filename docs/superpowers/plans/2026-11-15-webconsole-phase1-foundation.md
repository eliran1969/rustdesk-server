# Web Console - שלב 1 (בסיס) - תוכנית בנייה

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** שרת RustDesk (fork) שמריץ גם Console ו-API בפורט 21114: כניסת admin לממשק בעברית ובאנגלית, רשימת מכשירים חיה עם פרטי מחשב, ו-client אמיתי שמדווח ומתחבר כמשתמש. פריסה כ-Docker על ה-Synology במקום ה-hbbs הקיים, בלי לאבד מפתחות או נתונים.

**Architecture:** מודול חדש `src/console/` בתוך hbbs, שרת axum שרץ באותו תהליך ומשתף את קובץ ה-SQLite ואת ה-`PeerMap` שבזיכרון. ממשק React+TS+Vite בתיקייה `console-ui/`, שנבנה ומוטמע בבינארי עם `rust-embed`. שינויים בקבצים קיימים מצומצמים לנקודת הפעלה אחת ולפקודה ב-`rustdesk-utils`.

**Tech Stack:** Rust 2021, axum 0.5 (קיים), sqlx 0.6 SQLite (קיים, **שאילתות runtime בלבד** ולא `query!`), bcrypt (קיים), rust-embed, rand. React 18 + TypeScript + Vite + react-router + react-i18next + vitest. Docker multi-stage (amd64 + arm64).

**Spec:** `docs/superpowers/specs/2026-11-14-webconsole-design.md` (+ קטלוג: `docs/webconsole/feature-catalog.md`)

## Global Constraints

- ריפו: fork של `rustdesk/rustdesk-server` בשם `eliran1969/rustdesk-server`, משוכפל ל-`E:\RustDesk-server`.
- אין `unwrap()`/`expect()` בקוד production (מותר בבדיקות). אין שינויי פורמט בקוד קיים (כללי `AGENTS.md` של השרת).
- כל הקוד החדש ב-`src/console/` וב-`console-ui/`. בקבצים קיימים רק: `src/lib.rs` (`pub mod console;`), `src/rendezvous_server.rs` (קריאה אחת להפעלה), `src/utils.rs` (פקודה), `Cargo.toml`.
- פורט: `API_PORT`, ברירת מחדל `21114`. הערך `0` מכבה את ה-Console.
- DB: אותו קובץ שמחושב ב-`PeerMap::new` (`DB_URL`, ברירת מחדל `db_v2.sqlite3`). migrations רק מוסיפות (`create table if not exists`). לפני הרצת migration שמעלה גרסה, מגבים לקובץ `<db>.bak-<unix_ts>`.
- טבלת `peer` הקיימת: מותר לכתוב רק לעמודות `user`, `status`, `note`. אסור לשנות את `id`, `uuid`, `pk`, `info`, כי hbbs מנהל אותן.
- חוזה ה-client (מקור: `E:\RustDesk`): תשובות טקסט `SYSINFO_UPDATED` / `ID_NOT_FOUND`. `uuid` בבקשות הוא base64 של בתי ה-uuid ששמורים ב-`peer.uuid`. כל שגיאה ב-JSON היא `{"error": "<text>"}`.
- נתיבים שמיועדים רק לממשק הם תחת `/api/console/*`, כדי לא להתנגש עם נתיבי Pro שה-client והסקריפטים מכירים.
- ממשק: כל מחרוזת דרך i18n (`he`, `en`), בלי טקסט קשיח. `<html dir>` הוא `rtl` לעברית ו-`ltr` לאנגלית. CSS עם logical properties בלבד (`margin-inline-start` ולא `margin-left`).
- סיסמאות ב-bcrypt (cost 12). טוקן session הוא 32 בתים אקראיים בקידוד hex, ונשמר ב-DB כ-SHA-256 ולא כטקסט גלוי.

## Review Focus

1. **DB אמיתי מה-Synology** (עם טבלת `peer` מלאה ובלי טבלאות חדשות): השרת עולה, נוצר גיבוי, כל השורות נשמרות, ו-hbbs ממשיך לרשום מכשירים. הבדיקה נמצאת במשימה 2.
2. **sysinfo או heartbeat מזויף** (ID קיים עם uuid אחר): נדחה ולא דורס נתונים של המכשיר האמיתי. משימה 5.
3. **ניחוש סיסמאות:** אחרי 10 כישלונות בדקה מאותו IP מוחזר 429. שם משתמש לא קיים, סיסמה שגויה ומשתמש מושבת מחזירים את אותה הודעה. משימה 4.
4. **משתמש לא-admin** שקורא ל-`/api/devices` רואה רק את המכשירים ששויכו אליו, ומקבל 403 על נתיבי admin. משימה 6.
5. **פורט 21114 תפוס** או תקלה ב-Console: נרשמת שגיאה ללוג, ו-hbbs ממשיך לעבוד כרגיל (חיבורים בין מחשבים לא נפגעים). משימה 1.

---

### Task 0: הקמת הריפו וסנכרון פרוטוקול

**Files:**
- Modify: `.gitmodules` (ללא שינוי URL), submodule `libs/hbb_common` → commit `fae47e024dd83cef6aaecde0aa7d11731df11649`
- Create: `docs/superpowers/specs/2026-11-14-webconsole-design.md`, `docs/webconsole/feature-catalog.md`, `docs/superpowers/plans/2026-11-15-webconsole-phase1-foundation.md` (העתקה מ-`E:\RustDesk\docs\...`)

- [ ] **Step 1:** המשתמש יוצר ב-GitHub fork של `rustdesk/rustdesk-server` לחשבון `eliran1969` (פעולה ידנית שלו).
- [ ] **Step 2:** `git clone https://github.com/eliran1969/rustdesk-server.git E:/RustDesk-server && cd E:/RustDesk-server && git submodule update --init --recursive && git checkout -b webconsole`
- [ ] **Step 3:** וידוא כלים: `cargo --version` (≥1.75), `node --version` (≥20), `docker --version`. אם חסר, מתקינים לפני שממשיכים.
- [ ] **Step 4:** `cargo build && cargo test`. צפוי: עובר, וזה בסיס הייחוס.
- [ ] **Step 5:** `cd libs/hbb_common && git fetch && git checkout fae47e024dd83cef6aaecde0aa7d11731df11649 && cd ../.. && cargo build && cargo test`. אם הבנייה נשברת בגלל API שהשתנה ב-hbb_common, מתקנים בשינוי המינימלי בקבצי השרת ומתעדים כל תיקון בהודעת ה-commit.
- [ ] **Step 6:** מעתיקים את שלושת מסמכי התכנון לריפו. Commit: `chore: sync hbb_common with client fae47e0, add web console design docs`

---

### Task 1: שלד Console ושרת HTTP שלא מפיל את hbbs

**Files:**
- Create: `src/console/mod.rs`, `src/console/state.rs`, `src/console/error.rs`, `src/console/api/mod.rs`, `src/console/api/client.rs`, `src/console/test_util.rs` (`#[cfg(test)]`: ‏`test_pm()` עם DB זמני, ‏`test_app()` שמחזיר `router` מוכן אחרי `migrate`, ‏`body_string(res)`, ‏`insert_peer(pool, id, uuid)`, ‏`insert_user(pool, name, password, is_admin)`, ‏`login_token(app, name, password)`. כל המשימות הבאות משתמשות בהם)
- Modify: `src/lib.rs` (הוספת `pub mod console;`), `src/rendezvous_server.rs` (ב-`start_with_bind`, אחרי `PeerMap::new()`), `Cargo.toml` (`rand = "0.8"`, `sha2 = "0.10"`, `hex = "0.4"`, `rust-embed = "8"`, `mime_guess = "2"`)
- Test: `src/console/api/client.rs` (`#[cfg(test)]`)

**Interfaces:**
- Produces:
  - `pub(crate) struct ConsoleState { pub db: sqlx::SqlitePool, pub pm: crate::peer::PeerMap, pub limiter: Arc<crate::console::auth::LoginLimiter> }` (Clone). ה-limiter מתווסף במשימה 4, ועד אז השדה לא קיים.
  - `pub(crate) async fn start(pm: PeerMap, db_url: String)`: מחזירה מיד ומריצה `tokio::spawn`. כל כישלון (bind, DB) נרשם ב-`log::error!` ולא חוזר ל-hbbs.
  - `pub(crate) fn router(state: ConsoleState) -> axum::Router`, לבדיקות.
  - `pub(crate) enum ApiError { BadRequest(String), Unauthorized, Forbidden, NotFound, TooManyRequests, Internal(String) }`, שמממש `IntoResponse`. הסטטוס בהתאמה (400/401/403/404/429/500), והגוף `{"error": msg}`. ב-`Internal` הגוף הוא `"Internal error"` והפרטים נרשמים ללוג.
- Consumes: `PeerMap` (צריך `pub(crate) fn db_url()` או להעביר את המחרוזת שחושבה ב-`PeerMap::new`. **החלטה:** מוציאים את חישוב ה-URL ל-`pub(crate) fn resolve_db_url() -> String` ב-`peer.rs`, ומשתמשים בה בשני המקומות).

- [ ] **Step 1: בדיקות כושלות**

```rust
#[tokio::test]
async fn login_options_returns_empty_json_array() {
    let app = test_app().await; // helper: in-memory sqlite + empty PeerMap
    let res = app.oneshot(Request::get("/api/login-options").body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(res.status(), 200);
    assert_eq!(body_string(res).await, "[]");
}

#[tokio::test]
async fn start_survives_port_in_use() {
    let _hold = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = _hold.local_addr().unwrap().port();
    std::env::set_var("API_PORT", port.to_string());
    let r = tokio::time::timeout(Duration::from_secs(2),
        crate::console::start(test_pm().await, ":memory:".into())).await;
    assert!(r.is_ok()); // returned promptly, no panic; bind error only logged
}
```

- [ ] **Step 2:** `cargo test console::` → FAIL (המודול לא קיים).
- [ ] **Step 3:** מממשים את `start`, `router` ו-`ApiError`, ואת `GET /api/login-options` שמחזיר `[]`. ה-pool נפתח עם `SqlitePoolOptions` ו-`create_if_missing(true)`, ‏`busy_timeout(5s)` ו-`journal_mode(WAL)`. ב-`start_with_bind` מוסיפים שורה אחת: `crate::console::start(pm.clone(), crate::peer::resolve_db_url()).await;`
- [ ] **Step 4:** `cargo test console::` → PASS. מריצים `cargo run --bin hbbs` ובודקים ש-`curl http://127.0.0.1:21114/api/login-options` מחזיר `[]`, ושהלוג מראה ש-hbbs מאזין ב-21116.
- [ ] **Step 5:** Commit: `feat(console): HTTP server on 21114 inside hbbs`

---

### Task 2: Migrations וגיבוי

**Files:**
- Create: `src/console/db/mod.rs`, `src/console/db/migrations.rs`, `tests/fixtures/upstream_db_v2.sqlite3` (עותק של `db_v2.sqlite3` מה-root של הריפו)
- Test: `src/console/db/migrations.rs`

**Interfaces:**
- Produces: `pub(crate) async fn migrate(pool: &SqlitePool, db_path: &str) -> ResultType<()>`, נקראת ב-`start` לפני ה-bind.
- סכימה (גרסה 1), טבלת `console_meta(key text primary key, value text)` עם `schema_version`:

```sql
create table if not exists console_user (
  guid blob primary key not null,
  name varchar(100) not null unique collate nocase,
  display_name varchar(100) not null default '',
  email varchar(200) not null default '',
  note varchar(300) not null default '',
  password_hash text not null,
  is_admin tinyint not null default 0,
  status tinyint not null default 1,          -- 1 normal, 0 disabled (UserPayload.status)
  must_change_password tinyint not null default 0,
  created_at datetime not null default(current_timestamp)
);
create table if not exists console_session (
  token_hash blob primary key not null,       -- sha256(token)
  user_guid blob not null,
  kind varchar(10) not null,                  -- 'web' | 'client'
  peer_id varchar(100), peer_uuid varchar(100),
  created_at datetime not null default(current_timestamp),
  last_seen datetime not null default(current_timestamp)
);
create table if not exists console_peer_sysinfo (
  peer_guid blob primary key not null,        -- peer.guid
  sysinfo text not null,                      -- JSON כפי שנשלח
  last_online datetime,                       -- heartbeat אחרון
  updated_at datetime not null default(current_timestamp)
);
```

- [ ] **Step 1: בדיקות כושלות**
  - `migrate_preserves_upstream_peers`: מעתיקים את ה-fixture לקובץ זמני, סופרים `select count(*) from peer`, מריצים `migrate`. מצפים: אותו מספר שורות, הטבלאות החדשות קיימות, `schema_version = '1'`, וקיים קובץ `<path>.bak-*`.
  - `migrate_is_idempotent`: הרצה שנייה לא יוצרת גיבוי נוסף ולא נכשלת.
  - `migrate_fresh_db`: על קובץ חדש וריק (ללא `peer`) עובר, כי hbbs יוצר את `peer` בעצמו.
- [ ] **Step 2:** `cargo test console::db` → FAIL.
- [ ] **Step 3:** מממשים: קוראים את `schema_version` (חסר = 0). אם קטן מ-1 והקובץ קיים ואינו ריק, `std::fs::copy` לגיבוי. מריצים את ה-DDL בתוך טרנזקציה.
- [ ] **Step 4:** `cargo test console::db` → PASS.
- [ ] **Step 5:** Commit: `feat(console): additive schema v1 with pre-migration backup`

---

### Task 3: משתמשים, סיסמאות ו-admin ראשוני

**Files:**
- Create: `src/console/db/users.rs`
- Modify: `src/utils.rs` (פקודה `set_password`)
- Test: `src/console/db/users.rs`

**Interfaces:**
- Produces:
  - `pub(crate) struct User { pub guid: Vec<u8>, pub name: String, pub display_name: String, pub email: String, pub note: String, pub is_admin: bool, pub status: i64, pub must_change_password: bool }`
  - `pub(crate) async fn find_by_name(pool, name: &str) -> ResultType<Option<(User, String /*hash*/)>>`
  - `pub(crate) async fn find_by_guid(pool, guid: &[u8]) -> ResultType<Option<User>>`
  - `pub(crate) async fn set_password(pool, name: &str, password: &str, must_change: bool) -> ResultType<bool /*found*/>`
  - `pub(crate) async fn ensure_initial_admin(pool) -> ResultType<Option<String /*generated password*/>>`
  - `pub(crate) fn hash_password(p: &str) -> ResultType<String>` / `pub(crate) fn verify_password(p: &str, hash: &str) -> bool`
- כלל סיסמה: לפחות 8 תווים (נאכף ב-`set_password` שנקראת מה-API ומה-CLI).

- [ ] **Step 1: בדיקות כושלות**
  - `initial_admin_created_once`: על DB ריק `ensure_initial_admin` מחזירה `Some(pw)` עם `pw.len() == 16`. נוצר `admin` עם `is_admin` ו-`must_change_password`. קריאה שנייה מחזירה `None`.
  - `verify_password_roundtrip`: מחרוזת נכונה מחזירה `true`, ומחרוזת שגויה או hash פגום מחזירים `false`.
  - `set_password_rejects_short`: `"1234567"` מחזיר שגיאה.
  - `find_by_name_case_insensitive`: `"ADMIN"` מוצא את `admin`.
- [ ] **Step 2:** `cargo test console::db::users` → FAIL.
- [ ] **Step 3:** מממשים. ב-`start`: אם `ensure_initial_admin` מחזירה סיסמה, `log::warn!("Console initial admin password: {pw} (change it on first login)")`. ב-`utils.rs`: `rustdesk-utils set_password <username> <password>` פותח את ה-DB לפי `DB_URL` (או `./db_v2.sqlite3`), קורא ל-`set_password(..., must_change=false)`, ומדפיס `Done` או `User not found`. מוסיפים שורה ל-`print_help`.
- [ ] **Step 4:** `cargo test console::db::users` → PASS.
- [ ] **Step 5:** Commit: `feat(console): users, bcrypt passwords, initial admin, set_password CLI`

---

### Task 4: כניסה, sessions והגנה מניחוש

**Files:**
- Create: `src/console/auth.rs`, `src/console/db/sessions.rs`, `src/console/api/account.rs`
- Test: `src/console/api/account.rs`

**Interfaces:**
- Produces:
  - `pub(crate) struct AuthUser(pub User, pub SessionKind)`: axum extractor. קורא את `Authorization: Bearer <token>`, מחשב sha256, מחפש ב-`console_session` ובודק `status == 1`. אחרת מחזיר `ApiError::Unauthorized`. מעדכן את `last_seen`.
  - `pub(crate) struct AdminUser(pub User)`: עוטף את `AuthUser` ומחזיר `Forbidden` אם המשתמש לא admin.
  - `pub(crate) struct LoginLimiter`: ‏`fn check(&self, ip: IpAddr) -> bool` ו-`fn fail(&self, ip)` ו-`fn success(&self, ip)`. מעל 10 כישלונות בחלון של 60 שניות מחזיר `false`.
  - `pub(crate) async fn create_session(pool, user_guid, kind, peer_id: Option<&str>, peer_uuid: Option<&str>) -> ResultType<String /*plain token*/>`
- נתיבים:
  - `POST /api/login`, גוף כמו `LoginRequest` ב-`flutter/lib/common/hbbs/hbbs.dart:133`. שדה `type` חסר או `"account"` נתמך, וכל ערך אחר מחזיר 400 `{"error":"Unsupported login type"}`. בהצלחה: `{"type":"access_token","access_token":"<token>","user":{"name","display_name","email","note","status":1,"is_admin":bool}}`. בכישלון: 401 `{"error":"Wrong username or password"}`, כולל משתמש מושבת. מעל המגבלה: 429. אם יש `id` ו-`uuid` (כניסה מה-client): `kind='client'`, ומשייכים את המכשיר `update peer set user=? where id=? and uuid=?` (רק אם ה-uuid תואם). אחרת `kind='web'`.
  - `POST /api/currentUser`, Bearer. מחזיר את אותו אובייקט `user`, או 401.
  - `POST /api/logout`, Bearer. מוחק את ה-session ומחזיר `{}`. שיוך המכשיר **נשאר** (החלטה: Pro מתנהג כך, וההסרה היא פעולה ניהולית).
  - `GET /api/console/me`, Bearer web. מחזיר `user` ועוד `must_change_password`.
  - `POST /api/console/change-password`, Bearer, גוף `{"old_password","new_password"}`. סיסמה ישנה שגויה מחזירה 400. בהצלחה מאפס את `must_change_password` ומוחק את שאר ה-sessions של המשתמש חוץ מהנוכחי.

- [ ] **Step 1: בדיקות כושלות**
  - `login_success_returns_token_and_user`: מוודא `type == "access_token"`, ‏`user.is_admin == true`, ו-`access_token.len() == 64`.
  - `login_wrong_password_and_unknown_user_same_error`: שני המקרים מחזירים 401 עם אותו גוף.
  - `login_disabled_user_rejected`: ‏401.
  - `login_rate_limited_after_10_failures`: הניסיון ה-11 מאותו IP מחזיר 429, גם עם סיסמה נכונה.
  - `login_from_client_assigns_peer_only_with_matching_uuid`: מכניסים שורת `peer` עם id `"123"` ו-uuid `b"u1"`. כניסה עם `uuid=base64("u1")` מעדכנת `peer.user`, וכניסה עם uuid אחר לא מעדכנת.
  - `current_user_requires_valid_token`: בלי טוקן או עם טוקן לא תקין מחזיר 401, וטוקן תקין מחזיר 200.
  - `logout_invalidates_token`: אחרי logout, `currentUser` מחזיר 401.
  - `token_stored_hashed`: ‏`select count(*) from console_session where token_hash = <plain bytes>` מחזיר 0.
  - `change_password_clears_flag`.
- [ ] **Step 2:** `cargo test console::api::account` → FAIL.
- [ ] **Step 3:** מממשים. כתובת ה-IP נלקחת מ-`ConnectInfo<SocketAddr>`, או מ-`X-Forwarded-For` רק אם `TRUST_PROXY=Y` (ברירת מחדל `N`).
- [ ] **Step 4:** `cargo test console::api::account` → PASS.
- [ ] **Step 5:** Commit: `feat(console): login/logout/currentUser with hashed sessions and rate limit`

---

### Task 5: דיווח מכשירים (sysinfo + heartbeat)

**Files:**
- Create: `src/console/db/devices.rs`, `src/console/api/client.rs` (הרחבה)
- Test: `src/console/api/client.rs`

**Interfaces:**
- Produces:
  - `pub(crate) async fn verify_peer(pool, id: &str, uuid_b64: &str) -> ResultType<Option<Vec<u8> /*guid*/>>`: מחזירה את ה-guid רק אם יש שורת `peer` עם id זהה ו-`uuid == base64_decode(uuid_b64)`.
  - `pub(crate) const SYSINFO_VER: &str = "1";`
- נתיבים (בלי אימות, כמו שה-client שולח):
  - `POST /api/sysinfo`: הגוף JSON עם `id`, ‏`uuid` ועוד שדות. אם `verify_peer` נכשל, מחזירים את הטקסט `ID_NOT_FOUND`. אחרת: upsert ל-`console_peer_sysinfo.sysinfo` (ה-JSON המלא בלי `uuid`), ומחזירים את הטקסט `SYSINFO_UPDATED`. אם יש `preset-username` והמכשיר עוד לא משויך, משייכים למשתמש בשם הזה אם הוא קיים.
  - `POST /api/sysinfo_ver`: מחזיר את הטקסט `SYSINFO_VER`.
  - `POST /api/heartbeat`: גוף `{id, uuid, ver, conns?, modified_at}`. אם `verify_peer` נכשל מחזירים `{}` ולא כותבים כלום. אחרת מעדכנים `last_online = current_timestamp`. אם אין שורת sysinfo מחזירים `{"sysinfo": true}` (זה גורם ל-client להעלות). אחרת `{}`. בשלב 1 לא מחזירים `strategy` או `disconnect`.
- סטטוס מקוון (לשימוש במשימה 6): `pub(crate) async fn is_online(pm: &PeerMap, id: &str) -> bool`, שמחזירה true אם `pm.get_in_memory(id)` קיים ו-`last_reg_time.elapsed() < 30s` (אותו ערך כמו `REG_TIMEOUT`).

- [ ] **Step 1: בדיקות כושלות**
  - `sysinfo_unknown_id_returns_id_not_found`
  - `sysinfo_wrong_uuid_rejected_and_not_written`: שורת `peer` עם uuid `b"u1"`, ושליחה עם uuid של `"u2"`. מצפים ל-`ID_NOT_FOUND` ושאין שורה ב-`console_peer_sysinfo`.
  - `sysinfo_valid_stores_json_and_returns_updated`: הטקסט הוא `SYSINFO_UPDATED`, וה-JSON השמור מכיל את `hostname` ו-`os`.
  - `heartbeat_requests_sysinfo_when_missing`: התשובה היא `{"sysinfo":true}`.
  - `heartbeat_updates_last_online`
  - `sysinfo_preset_username_assigns_unassigned_peer`
- [ ] **Step 2:** `cargo test console::api::client` → FAIL.
- [ ] **Step 3:** מממשים. גבול גוף הבקשה: 64KB (`ContentLengthLimit` או `DefaultBodyLimit`).
- [ ] **Step 4:** `cargo test console::api::client` → PASS.
- [ ] **Step 5:** Commit: `feat(console): sysinfo and heartbeat endpoints with uuid verification`

---

### Task 6: API רשימת מכשירים

**Files:**
- Create: `src/console/api/devices.rs`
- Test: `src/console/api/devices.rs`

**Interfaces:**
- Produces: `GET /api/devices`, עם `AuthUser`.
  - פרמטרים: `current` (≥1, ברירת מחדל 1), ‏`pageSize` (1..100, ברירת מחדל 30), ‏`id`, ‏`device_name`, ‏`user_name` (תבניות `LIKE` כפי שנשלחו. `%` הוא תו כללי, כמו `res/devices.py`).
  - תשובה: `{"total": n, "data": [Device]}`, כאשר `Device = {"guid": hex, "id", "status": 1|0 (peer.status null → 1), "is_online": bool, "last_online": "YYYY-MM-DDTHH:MM:SS" | null, "user_name": string|"", "note": string, "info": {sysinfo JSON: os, hostname→device_name, username, cpu, memory, version}}`. השדה `info.device_name` נלקח מ-`hostname` ב-sysinfo, כי כך `PeerPayload.toPeer` קורא אותו.
  - admin רואה הכל. לא-admin רואה רק `peer.user = <guid שלו>`.
  - מיון: מקוונים קודם, ואחר כך לפי `id`.

- [ ] **Step 1: בדיקות כושלות**
  - `devices_admin_sees_all_paginated`: 35 שורות `peer`, ‏`pageSize=30`. עמוד 1 מחזיר 30, עמוד 2 מחזיר 5, ו-`total == 35`.
  - `devices_filter_by_name_wildcard`: ‏`device_name=%lap%` מחזיר רק את המכשירים שה-hostname שלהם מכיל "lap".
  - `devices_non_admin_sees_only_own`
  - `devices_without_token_401`
  - `devices_info_maps_hostname_to_device_name`
- [ ] **Step 2:** `cargo test console::api::devices` → FAIL.
- [ ] **Step 3:** מממשים: ‏`peer left join console_peer_sysinfo left join console_user`, וסינון `device_name` דרך `json_extract(sysinfo,'$.hostname') like ?`. ה-`is_online` מחושב לכל שורה בעמוד דרך `is_online`, והמיון "מקוונים קודם" מתבצע בתוך העמוד. (החלטה: מיון גלובלי לפי מקוון דורש את כל הרשימה בזיכרון. עד 10,000 מכשירים זה זניח, אבל בשלב 1 העמוד מספיק.)
- [ ] **Step 4:** `cargo test console::api::devices` → PASS.
- [ ] **Step 5:** Commit: `feat(console): paginated device list compatible with res/devices.py`

---

### Task 7: ממשק Web - שלד, i18n ו-RTL, כניסה, החלפת סיסמה, מכשירים

**Files:**
- Create: `console-ui/` (Vite React-TS), ובו: `src/i18n/{index.ts,he.json,en.json}`, `src/api.ts`, `src/auth.tsx`, `src/layout/AppLayout.tsx`, `src/pages/{LoginPage,ChangePasswordPage,DevicesPage}.tsx`, `src/styles.css`, `src/__tests__/*.test.tsx`
- Create: `src/console/web.rs`
- Modify: `build.rs` (מוודא ש-`console-ui/dist/` קיים. אם לא, יוצר `index.html` זמני עם ההודעה "UI not built - run npm run build in console-ui")
- Modify: `.gitignore` (`console-ui/node_modules`, `console-ui/dist`)

**Interfaces:**
- Consumes: הנתיבים ממשימות 4 ו-6.
- Produces:
  - `src/console/web.rs`: ‏`#[derive(RustEmbed)] #[folder = "console-ui/dist/"]`. כל `GET` שאינו מתחיל ב-`/api/` מגיש קובץ. אם הקובץ לא נמצא, מגיש `index.html` (SPA fallback). ה-`Content-Type` לפי `mime_guess`. כותרות: `X-Frame-Options: DENY`, ‏`X-Content-Type-Options: nosniff`.
  - `api.ts`: ‏`apiFetch<T>(path, init)` מוסיף Bearer מ-`localStorage['console_token']`. על 401 מוחק את הטוקן ומנווט ל-`/login`. על גוף עם `error` זורק `ApiError(message)`.
  - מסכים:
    - `/login`: שם משתמש, סיסמה, כפתור, הודעת שגיאה (כולל 429 עם "יותר מדי ניסיונות, נסה שוב בעוד דקה").
    - `/change-password`: חובה כש-`must_change_password`, ואין ניווט אחר עד שמחליפים.
    - `/devices`: טבלה עם עמודות ID, שם מכשיר, משתמש מערכת, מערכת הפעלה, גרסה, משתמש משויך, מקוון (נקודה ירוקה או אפורה), נראה לאחרונה, הערה. שדה חיפוש עם בורר "לפי ID / לפי שם", שעוטף ב-`%…%` אם אין `%` (כמו `devices.py`). עימוד. רענון אוטומטי כל 15 שניות.
  - `AppLayout`: תפריט צד. בשלב 1 רק "מכשירים" פעיל. הפריטים של השלבים הבאים לא מוצגים (אנחנו לא מציגים פיצ'רים שלא קיימים). בראש העמוד: שם המשתמש, מתג שפה עברית/English, ויציאה.
  - i18n: ברירת המחדל היא שפת הדפדפן (`he*` → `he`, אחרת `en`), והבחירה נשמרת ב-`localStorage['console_lang']`. בשינוי שפה מעדכנים `document.documentElement.dir` ו-`lang`.

- [ ] **Step 1: בדיקות כושלות (vitest + @testing-library/react)**
  - `i18n.test.ts › he and en have identical key sets` (השוואה רקורסיבית של המפתחות).
  - `i18n.test.ts › switching to he sets dir=rtl and lang=he`, ‏`› switching to en sets dir=ltr`.
  - `LoginPage.test.tsx › shows server error message on 401` (fetch mock שמחזיר `{"error":"Wrong username or password"}` → הטקסט המתורגם מופיע).
  - `LoginPage.test.tsx › redirects to /change-password when must_change_password`.
  - `DevicesPage.test.tsx › wraps search term with % and sends device_name` (בודק את ה-URL שנשלח ל-fetch).
  - `no-hardcoded-text.test.ts`: סורק את `src/**/*.tsx` ונכשל אם יש טקסט JSX גולמי עם אותיות (regex `>[^<{]*[A-Za-zא-ת][^<{]*<`).
  - בצד Rust: `web_serves_index_for_unknown_route` ו-`api_404_not_swallowed_by_spa` (‏`/api/nope` מחזיר 404 JSON ולא HTML).
- [ ] **Step 2:** `cd console-ui && npm test` → FAIL. ‏`cargo test console::web` → FAIL.
- [ ] **Step 3:** מממשים. כיוון RTL מטופל ב-CSS בלבד (logical properties), ואייקוני חץ מתהפכים עם `[dir=rtl] .icon-dir { transform: scaleX(-1) }`. בלי ספריית UI כבדה: CSS משלנו, נקי, בהשראת מבנה ה-Console המקורי (תפריט צד כהה, תוכן בהיר).
- [ ] **Step 4:** `npm test && npm run build && cargo test console::web` → PASS. בדיקה ידנית: ‏`cargo run --bin hbbs`, גלישה ל-`http://localhost:21114`, כניסה עם הסיסמה מהלוג, החלפה, ורשימת מכשירים. החלפה לאנגלית הופכת את הכיוון.
- [ ] **Step 5:** Commit: `feat(console-ui): bilingual RTL/LTR UI with login, password change and devices`

---

### Task 8: Docker ל-Synology ונוהל שדרוג בטוח

**Files:**
- Create: `docker/Dockerfile.console`, `docker/docker-compose.console.yml`, `docs/webconsole/synology-upgrade.md`
- Modify: `docker/rootfs/etc/s6-overlay/s6-rc.d/hbbs/run` — רק אם צריך להעביר `API_PORT`. העדיפות: ENV בלי שינוי בקובץ.

**Interfaces:**
- Dockerfile רב-שלבי:
  1. `node:20-alpine`: ‏`npm ci && npm run build` ב-`console-ui`.
  2. `rust:1-bookworm`: ‏`cargo build --release --bin hbbs --bin hbbr --bin rustdesk-utils`, עם ה-`dist` מהשלב הקודם.
  3. runtime זהה לקיים (busybox + s6) עם הבינאריים ב-`/usr/bin`. ‏`EXPOSE 21114`. ‏`WORKDIR /data`.
  - בנייה לשתי ארכיטקטורות: `docker buildx build --platform linux/amd64,linux/arm64`.
- compose: אותו volume `/data` כמו ההתקנה הקיימת, `network_mode: host`, ו-`API_PORT=21114`.
- המסמך `synology-upgrade.md` (בעברית) מכסה:
  1. איך מוצאים את תיקיית הנתונים הנוכחית ב-Container Manager.
  2. גיבוי מלא של התיקייה (`id_ed25519*`, `db_v2.sqlite3*`).
  3. **הרצת ניסיון:** עותק של התיקייה, container בפורטים חלופיים (`-p 31115-31119`, `API_PORT=31114`), ובדיקה שהממשק עולה עם המכשירים הקיימים ושהגיבוי נוצר.
  4. החלפה אמיתית: עצירת ה-container הישן והפעלת החדש על התיקייה המקורית.
  5. שחזור: עצירה, החזרת הגיבוי, והפעלה מחדש של ה-image הישן.
  6. reverse proxy ב-DSM (‏`Login Portal → Advanced → Reverse Proxy`) מ-443 ל-21114.
  7. הגדרת ה-client: שדה API Server.

- [ ] **Step 1:** `docker buildx build --platform linux/amd64 -f docker/Dockerfile.console -t rustdesk-server-console:dev --load .` → מצליח.
- [ ] **Step 2:** מריצים מקומית עם volume שמכיל עותק של `tests/fixtures/upstream_db_v2.sqlite3` ומפתחות שנוצרו ב-`rustdesk-utils genkeypair`. ‏`curl localhost:21114/api/login-options` מחזיר `[]`, נוצר `db_v2.sqlite3.bak-*` ב-volume, ו-`id_ed25519.pub` לא השתנה (משווים hash לפני ואחרי).
- [ ] **Step 3:** בונים arm64 (`--platform linux/arm64`) ומוודאים שהבנייה מצליחה.
- [ ] **Step 4:** Commit: `feat(docker): console image for amd64/arm64 + Synology upgrade guide`

---

### Task 9: בדיקה מול client אמיתי וסיום שלב

**Files:**
- Create: `docs/webconsole/phase1-acceptance.md` (צ'קליסט ותוצאות)

- [ ] **Step 1:** מריצים את השרת המקומי (`cargo run --bin hbbs` + `hbbr`) או את ה-container. ב-client (גרסה מ-`E:\RustDesk` או RustDesk רגיל עדכני): ID Server הוא `<ip>`, ‏API Server הוא `http://<ip>:21114`, ו-Key הוא תוכן `id_ed25519.pub`.
- [ ] **Step 2:** תוך 30 שניות המכשיר מופיע ב-`/devices` עם hostname, ‏OS, משתמש מערכת, גרסה ונקודה ירוקה. סוגרים את ה-client, ותוך כ-30 שניות הנקודה אפורה.
- [ ] **Step 3:** ב-client: התחברות (Login) עם משתמש admin. ה-client מציג את שם המשתמש, וב-Console העמודה "משתמש משויך" מציגה `admin`.
- [ ] **Step 4:** יוצאים מה-client ומוודאים שהשיוך נשאר. מתחברים שוב עם סיסמה שגויה 11 פעמים ומוודאים שמתקבלת הודעת חסימה.
- [ ] **Step 5:** חיבור מרחוק רגיל בין שני מחשבים עובד כמו לפני השדרוג (בדיקת רגרסיה של hbbs/hbbr).
- [ ] **Step 6:** `python res/devices.py --url http://<ip>:21114 --token <token מ-localStorage> view` מ-`E:\RustDesk` מדפיס את המכשירים. מתעדים תוצאות, עושים commit, ו-push לענף `webconsole`.
- [ ] **Step 7:** Commit: `docs(console): phase 1 acceptance results`

---

## מה בא אחר כך
שלבים 2 עד 6 יקבלו כל אחד תוכנית משלו באותו פורמט, אחרי ששלב 1 נבדק על ה-Synology. כל שלב בונה על ה-`ConsoleState`, ‏`AuthUser`/`AdminUser`, ‏`ApiError` ו-migrations מגורסות (גרסה 2, 3...) שהוגדרו כאן.
