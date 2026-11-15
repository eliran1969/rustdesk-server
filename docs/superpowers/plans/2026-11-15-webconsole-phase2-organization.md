# Web Console - שלב 2 (ארגון) - תוכנית בנייה

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** ניהול משתמשים, קבוצות משתמשים, קבוצות מכשירים ומכשירים: API תואם לסקריפטים `res/users.py`, ‏`user-groups.py`, ‏`devices.py`, ‏`device-groups.py` ולטאב "קבוצה" ב-client, ומסכי ניהול בממשק (עברית/אנגלית).

**Architecture:** ממשיך את `src/console/` משלב 1 (axum 0.5, SQLite, `ConsoleState`, `AuthUser`/`AdminUser`, `ApiError`, `JsonBody`). מוסיף migration גרסה 2 (טבלאות קבוצות, חוקי גישה והרחבת מכשיר), מודולי DB ו-API לכל תחום, ומסכים חדשים ב-`console-ui/`. **חוקי הגישה נשמרים בשלב הזה, ונאכפים ב-hbbs רק בשלב 5.** כאן הם משמשים רק לחישוב "נגיש" בטאב הקבוצה של ה-client.

**Tech Stack:** כמו שלב 1.

**Spec:** `docs/superpowers/specs/2026-11-14-webconsole-design.md` (+ `docs/webconsole/feature-catalog.md` סעיפים 2.4-2.6, 3.3-3.6). המשך: `docs/webconsole/phase1-followups.md`.

## Global Constraints

כל האילוצים של שלב 1 בתוקף (ראו `docs/superpowers/plans/2026-11-15-webconsole-phase1-foundation.md` → Global Constraints). בנוסף:
- `src/console/` חייב להישאר נקי מ-`unwrap`/`expect`/`panic!`/indexing, כפי שנאכף ע"י clippy deny ב-`src/console/mod.rs`. ב-release רץ `panic = 'abort'`.
- Migration v2 מוסיפה בלבד: `create table if not exists` ו-`alter table console_user add column group_guid blob`. מגבים לפני שמעלים גרסה (המנגנון הקיים).
- עמודות `peer` שמותר לכתוב: `user`, ‏`status`, ‏`note`. שינוי שם תצוגה, משתמש מערכת וקבוצת מכשירים נשמר ב-`console_peer_ext`. **חריג אחד:** מחיקת מכשיר מוחקת את שורת `peer` ומסירה אותו מה-`PeerMap` שבזיכרון (משימה 6).
- פורמט רשימות: `{"total": n, "data": [...]}` עם `current` (ברירת מחדל 1) ו-`pageSize` (ברירת מחדל 30, מקסימום 100). פרמטר לא תקין מחזיר את ברירת המחדל ולא 400. סינון שמות הוא `LIKE` עם פרמטר כפי שנשלח (`%` הוא תו כללי).
- תשובות פעולה (enable, disable, delete, create, update): ‏200 עם גוף `{}`. שגיאה: ‏`{"error": "..."}` עם 400/403/404.
- `guid` בכל ה-API: hex באותיות קטנות של 16 בתים. ‏`guid` לא תקין מחזיר 404.
- סוגי חוקי גישה: `{"type": n, "name": "..."}`. ‏`0` = קבוצת משתמשים, ‏`1` = קבוצת מכשירים, ‏`2` = משתמש. ‏`allowed_incomings` ("Can be accessed from") מקבל סוגים 0 ו-2. ‏`allowed_outgoings` ("Can access to") מקבל סוגים 0 ו-1 (רק בקבוצות משתמשים). שם שלא קיים מחזיר 400 `{"error":"Unknown group or user: <name>"}`.
- כל נתיבי הניהול דורשים `AdminUser` (בשלב 5 יתווספו Admin Roles). חריגים: `GET /api/users` עם `accessible`, ‏`GET /api/peers` ו-`GET /api/device-group/accessible` פתוחים לכל `AuthUser`.
- ממשק: כל מחרוזת דרך i18n (he/en), RTL/LTR ו-logical properties. מסכי ניהול מוצגים רק ל-admin.

## Review Focus

1. **מחיקת מכשיר בזמן שהוא מחובר ל-hbbs:** אחרי המחיקה, המכשיר נרשם מחדש כשהוא חוזר ומופיע שוב ברשימה. hbbs לא מנסה לעדכן guid ישן. משימה 6.
2. **השבתת משתמש:** כל ה-sessions שלו נמחקים מיד (ה-client מתנתק בבקשה הבאה), והמכשירים שלו נשארים משויכים. משימה 3.
3. **מחיקת קבוצה שיש בה חברים:** משתמשים או מכשירים לא נשארים "יתומים". מחיקת קבוצת משתמשים לא ריקה מחזירה 400. מחיקת קבוצת מכשירים מנתקת את המכשירים ממנה. קבוצת `Default` לא ניתנת למחיקה. משימות 4 ו-5.
4. **מנהל שמנסה להשבית, למחוק או להסיר admin מעצמו:** נחסם עם 400, כדי שלא יישאר מערכת בלי admin פעיל. משימה 3.
5. **`/api/devices/cli` עם id+uuid לא תואמים:** לא משנה כלום. משימה 6.

---

### Task 1: השלמות משלב 1 (follow-ups)

**Files:** `src/console/api/account.rs`, `src/console/db/sessions.rs`, `src/console/db/users.rs`, `src/console/mod.rs`, `src/console/web.rs`, `src/console/api/devices.rs`, `console-ui/src/api.ts`, `docs/webconsole/synology-upgrade.md`, `src/utils.rs`

- [ ] **Step 1: בדיקות כושלות**
  - `change_password_is_atomic`: מוודאים שעדכון ה-hash ומחיקת ה-sessions האחרים רצים באותה טרנזקציה. אם מחיקת ה-sessions נכשלת, ה-hash לא משתנה.
  - `handler_timeout_returns_503`: timeout של בקשה מחזיר 503 עם `{"error":"Request timed out"}` (לא 408).
  - `web_missing_asset_returns_404`: ‏`GET /assets/nope.js` מחזיר 404 ולא את `index.html`.
  - `devices_last_online_falls_back_to_created_at`: מכשיר בלי sysinfo מקבל `last_online` מתוך `peer.created_at` (באותו פורמט `T`), כדי ש-`devices.py --offline_days` לא יקרוס.
  - `cli_set_password_refuses_missing_db`: הפקודה `rustdesk-utils set_password` על נתיב DB שלא קיים מדפיסה `Database not found` ויוצאת עם קוד 1, בלי ליצור קובץ.
  - `initial_admin_insert_is_atomic`: אותה בדיקה של `ensure_initial_admin`, במימוש `insert ... where not exists`.
- [ ] **Step 2:** מריצים ומוודאים שהבדיקות נכשלות.
- [ ] **Step 3:** מממשים. מתקנים את הערת `REQUEST_TIMEOUT` (שאת ההמתנה לתור היא לא מכסה) ומוסיפים `load_shed` כדי שבקשות מעל המגבלה יקבלו 503 מיד. ב-`api.ts` עוטפים את `setToken` ב-try/catch. במדריך Synology §4 שלב 5: רק שורת `${OLD_HBBR}-old` מסומנת classic-only, ומוסיפים משפט על ספירת שורות `Key:` בלוג s6 ושורה על ה-env `PORT` של hbbr (מאזין ב-PORT+1).
- [ ] **Step 4:** ‏`cargo test`, ‏`cargo clippy --lib` ו-`npm test` עוברים.
- [ ] **Step 5:** Commit: `fix(console): phase 1 follow-ups (atomic password change, 503 timeout, asset 404, last_online fallback)`

---

### Task 2: Migration v2 ומודולי DB לקבוצות

**Files:** Modify `src/console/db/migrations.rs`. Create `src/console/db/groups.rs`, `src/console/db/rules.rs`, `src/console/db/peer_ext.rs`.

**Interfaces:**
- סכימה (גרסה 2):

```sql
create table if not exists console_user_group (
  guid blob primary key not null, name varchar(100) not null unique collate nocase,
  note varchar(300) not null default '', created_at datetime not null default(current_timestamp));
create table if not exists console_device_group (
  guid blob primary key not null, name varchar(100) not null unique collate nocase,
  note varchar(300) not null default '', created_at datetime not null default(current_timestamp));
create table if not exists console_access_rule (
  owner_kind varchar(12) not null,   -- 'user_group' | 'device_group'
  owner_guid blob not null,
  direction varchar(3) not null,     -- 'in' (allowed_incomings) | 'out' (allowed_outgoings)
  target_type tinyint not null,      -- 0 user group, 1 device group, 2 user
  target_guid blob not null,
  primary key (owner_kind, owner_guid, direction, target_type, target_guid));
create table if not exists console_peer_ext (
  peer_guid blob primary key not null,
  device_group_guid blob, device_name varchar(200), device_username varchar(200));
alter table console_user add column group_guid blob;  -- רק אם העמודה חסרה (pragma table_info)
```

  ה-migration יוצרת קבוצת משתמשים `Default` אם אין קבוצה בשם הזה, ומשייכת אליה כל משתמש שה-`group_guid` שלו null. בהמשך, כל משתמש חדש בלי קבוצה מקבל את `Default`.
- Produces:
  - `groups.rs`: ‏`pub(crate) struct Group { guid: Vec<u8>, name: String, note: String }`. פונקציות לכל אחד מ-`kind ∈ {User, Device}` (‏`pub(crate) enum GroupKind { User, Device }`): ‏`list(pool, kind, name_like: Option<&str>, current, page_size) -> (i64, Vec<Group>)`, ‏`find_by_name`, ‏`find_by_guid`, ‏`create(pool, kind, name, note) -> guid` (שם כפול מחזיר שגיאה מזוהה `DuplicateName`), ‏`update(pool, kind, guid, name: Option, note: Option)`, ‏`delete`, ‏`default_user_group(pool) -> Group`.
  - `rules.rs`: ‏`pub(crate) struct Rule { target_type: i64, name: String }` (השם נפתר מה-guid בעת קריאה). ‏`get(pool, owner_kind, owner_guid, direction) -> Vec<Rule>`, ‏`replace(tx, owner_kind, owner_guid, direction, rules: &[Rule]) -> Result<(), RuleError::Unknown(name) | InvalidType(n)>` שמחליף את כל החוקים לכיוון הזה באותה טרנזקציה.
  - `peer_ext.rs`: ‏`get(pool, peer_guid) -> Option<PeerExt>`, ‏`set_device_group(pool, peer_guid, Option<guid>)`, ‏`set_device_name`, ‏`set_device_username`, ‏`delete(pool, peer_guid)`.

- [ ] **Step 1: בדיקות כושלות**
  - `migrate_v2_from_v1_keeps_users_and_assigns_default_group`: ‏DB בגרסה 1 עם 2 משתמשים. אחרי ה-migration: ‏`schema_version='2'`, שני המשתמשים ב-`Default`, ונוצר גיבוי.
  - `migrate_v2_idempotent`.
  - `group_names_unique_case_insensitive`: יצירת `Sales` ואז `sales` מחזירה `DuplicateName`.
  - `rules_replace_resolves_names_and_rejects_unknown`.
  - `rules_reject_invalid_type_for_direction`: ‏`out` עם `type 2` מחזיר `InvalidType`.
- [ ] **Step 2-4:** מריצים (נכשלות), מממשים, ומריצים שוב (עוברות). ‏`cargo clippy --lib` נקי.
- [ ] **Step 5:** Commit: `feat(console): schema v2 for user/device groups, access rules and device overrides`

---

### Task 3: API משתמשים

**Files:** Create `src/console/api/users.rs`. Modify `src/console/db/users.rs`, `src/console/db/sessions.rs`, `src/console/mod.rs`.

**Interfaces (נתיבים):**
- `GET /api/users`, פרמטרים `name`, ‏`group_name`, ‏`status`, ‏`accessible`, ‏`current`, ‏`pageSize`.
  - admin בלי `accessible` רואה הכל.
  - עם `accessible` (לכל `AuthUser`): משתמשים בקבוצה שלו, ועוד משתמשים בקבוצות שהקבוצה שלו יכולה לגשת אליהן (`out` מסוג 0), ועוד קבוצות שנותנות לו גישה (`in` מסוג 0 על הקבוצה שלו, או מסוג 2 עליו עצמו). admin עם `accessible` רואה הכל.
  - לא-admin בלי `accessible` מקבל 403 `{"error":"Admin required!"}`. זו המחרוזת שה-client מזהה ב-`group_model.dart:185`.
  - `status=1` מסנן רק פעילים.
  - שורה: `{"guid","name","display_name","email","note","status","is_admin","group_name","created_at"}`.
- `POST /api/users` (admin), גוף `{name, password, group_name?, email?, note?, is_admin?}`. ‏`group_name` לא קיים מחזיר 400, ובלי `group_name` המשתמש נכנס ל-`Default`. סיסמה קצרה מ-8 תווים מחזירה 400. שם כפול מחזיר 400 `{"error":"User already exists"}`.
- `PATCH /api/users/{guid}` (admin), שדות אופציונליים `display_name`, ‏`email`, ‏`note`, ‏`password`, ‏`group_name`, ‏`is_admin`. שינוי סיסמה מוחק את כל ה-sessions של המשתמש.
- `POST /api/users/{guid}/enable` ו-`POST /api/users/{guid}/disable` (admin). השבתה מוחקת את כל ה-sessions.
- `DELETE /api/users/{guid}` (admin). רק משתמש מושבת, אחרת 400 `{"error":"Disable the user first"}`. המחיקה מנקה `peer.user` למכשירים שלו (מעדכנים `peer.user=null where user=?`, כי `user` היא עמודה מותרת), ומוחקת sessions וחוקי גישה שמפנים אליו.
- `POST /api/users/force-logout` (admin), גוף `{"user_guids":[hex...]}`. מוחק sessions.
- `POST /api/users/invite`, ‏`PUT /api/users/tfa/totp/enforce`, ‏`PUT /api/users/disable_login_verification`, ‏`PUT /api/users/tfa/reset`: מחזירים 400 `{"error":"Not supported yet"}` עד שלב 6.
- הגנה עצמית: admin לא יכול להשבית או למחוק את עצמו ולא להוריד לעצמו `is_admin`. גם אסור שהפעולה תשאיר 0 admins פעילים. בשני המקרים 400 `{"error":"Cannot remove the last active administrator"}` או `{"error":"Cannot change your own admin status"}`.

- [ ] **Step 1: בדיקות כושלות**
  - `users_list_admin_paginated_and_filtered`: סינון לפי `name=%ad%` ולפי `group_name`.
  - `users_list_non_admin_requires_accessible`: מחזיר 403 עם הגוף המדויק.
  - `users_accessible_same_group_and_outgoing_rules`: ‏A ו-B ב-G1, ‏C ב-G2, ו-G1 out→G2. ‏A רואה את A, ‏B ו-C. ‏C (בלי חוקים) רואה רק את עצמו.
  - `create_user_validates_and_defaults_group`
  - `disable_user_deletes_sessions`: טוקן של המשתמש מחזיר 401 אחרי ההשבתה.
  - `delete_requires_disabled_and_unassigns_devices`
  - `admin_cannot_disable_or_demote_self`
  - `force_logout_deletes_sessions`
  - `unsupported_user_endpoints_return_not_supported`
- [ ] **Step 2-4:** מריצים (נכשלות), מממשים, ומריצים שוב (עוברות). ‏`cargo clippy --lib` נקי.
- [ ] **Step 5:** Commit: `feat(console): users API (CRUD, enable/disable, force logout, accessible list)`

---

### Task 4: API קבוצות משתמשים

**Files:** Create `src/console/api/user_groups.rs`. Modify `src/console/mod.rs`.

**Interfaces:**
- `GET /api/user-groups` (admin), פרמטרים `name`, ‏`current`, ‏`pageSize`. שורה: `{"guid","name","note","created_at","allowed_incomings":[Rule],"allowed_outgoings":[Rule],"user_count":n}`.
- `POST /api/user-groups`, גוף `{name, note?, allowed_incomings?, allowed_outgoings?}`.
- `PATCH /api/user-groups/{guid}`, שדות אופציונליים, בטרנזקציה אחת. שדה חוקים שנשלח מחליף את הקיים, ושדה שלא נשלח נשאר.
- `DELETE /api/user-groups/{guid}`: ‏400 `{"error":"Group is not empty"}` אם יש משתמשים. ‏400 `{"error":"Cannot delete the default group"}` אם זו `Default`. מוחק גם חוקים שהקבוצה היא הבעלים או היעד שלהם.
- `POST /api/user-groups/{guid}`, גוף מערך guids של משתמשים. מעביר אותם לקבוצה. guid לא קיים מחזיר 400.

- [ ] **Step 1: בדיקות כושלות:** `user_groups_crud_with_rules`, ‏`patch_replaces_only_sent_rules`, ‏`delete_non_empty_or_default_rejected`, ‏`add_users_moves_them`, ‏`rules_unknown_name_rejected`.
- [ ] **Step 2-4:** מריצים (נכשלות), מממשים, ומריצים שוב (עוברות). ‏clippy נקי.
- [ ] **Step 5:** Commit: `feat(console): user groups API with access rules`

---

### Task 5: API קבוצות מכשירים

**Files:** Create `src/console/api/device_groups.rs`. Modify `src/console/mod.rs`.

**Interfaces:**
- `GET /api/device-groups` (admin): כמו קבוצות משתמשים, בלי `allowed_outgoings`, ועם `device_count`.
- `POST`, ‏`PATCH /api/device-groups/{guid}`: ‏`allowed_incomings` מקבל סוגים 0 ו-2.
- `DELETE /api/device-groups/{guid}`: מנקה `console_peer_ext.device_group_guid` למכשירים שלה, ומוחק חוקים.
- `POST /api/device-groups/{guid}`, גוף מערך **IDs של מכשירים** (מחרוזות `peer.id`). משייך אותם לקבוצה (מכשיר יכול להיות רק בקבוצה אחת, אז השיוך מחליף קבוצה קודמת). ID לא קיים מחזיר 400 `{"error":"Unknown device: <id>"}`, ולא מתבצע אף שינוי (טרנזקציה).
- `DELETE /api/device-groups/{guid}/devices`, גוף מערך IDs. מסיר רק מכשירים ששייכים לקבוצה הזו.
- `GET /api/device-group/accessible` (כל `AuthUser`): admin רואה הכל. משתמש רואה קבוצות מכשירים שבהן `in` כולל את הקבוצה שלו (סוג 0) או אותו (סוג 2), ועוד קבוצות שהקבוצה שלו מגיעה אליהן ב-`out` מסוג 1. שורה: `{"name"}` ועוד `guid`.

- [ ] **Step 1: בדיקות כושלות:** `device_groups_crud`, ‏`add_devices_replaces_previous_group_atomically`, ‏`add_unknown_device_changes_nothing`, ‏`remove_devices_only_from_this_group`, ‏`delete_group_unassigns_devices`, ‏`accessible_device_groups_by_rules`.
- [ ] **Step 2-4:** מריצים (נכשלות), מממשים, ומריצים שוב (עוברות). ‏clippy נקי.
- [ ] **Step 5:** Commit: `feat(console): device groups API and accessible device groups`

---

### Task 6: ניהול מכשירים, `/api/peers`, ‏`cli` ו-`deploy`

**Files:** Modify `src/console/api/devices.rs`, `src/console/mod.rs`, `src/peer.rs` (מוסיפים רק `pub(crate) async fn remove(&self, id: &str)` שמוחק מה-map בזיכרון). Create `src/console/api/peers.rs`.

**Interfaces:**
- הרחבת `GET /api/devices`: פרמטרים נוספים `group_name` (קבוצת המשתמש המשויך) ו-`device_group_name`. שדות נוספים בכל Device: ‏`device_group_name` (או `""`), ‏`group_name`. ‏`info.device_name` לוקח את `console_peer_ext.device_name` אם קיים, אחרת את hostname. ‏`info.username` לוקח את `device_username` אם קיים.
- `POST /api/devices/{guid}/enable` ו-`.../disable` (admin): כותבים ל-`peer.status` (1 או 0).
- `DELETE /api/devices/{guid}` (admin): רק מכשיר מושבת (`status=0`), אחרת 400 `{"error":"Disable the device first"}`. מוחקים בטרנזקציה את `console_peer_sysinfo`, ‏`console_peer_ext` ואת שורת `peer`, ואז `pm.remove(id)`.
- `PATCH /api/devices/{guid}` (admin), שדות אופציונליים `note`, ‏`device_name`, ‏`device_username`, ‏`user_name` (‏`""` מבטל שיוך), ‏`device_group_name` (‏`""` מסיר מקבוצה).
- `POST /api/devices/{guid}/assign` (admin), גוף `{"type","value"}`, תואם ל-`devices.py`. הסוגים `user_name`, ‏`device_group_name`, ‏`note`, ‏`device_username` ו-`device_name` עוברים לאותה לוגיקה של PATCH. ‏`strategy_name` ו-`ab` מחזירים 400 `{"error":"Not supported yet"}`.
- `GET /api/peers` (כל `AuthUser`), פרמטרים `current`, ‏`pageSize`, ‏`status`, ‏`accessible`. admin רואה הכל. משתמש רואה מכשירים ששויכו למשתמשים הנגישים לו (אותה לוגיקה כמו `/api/users?accessible`), ועוד מכשירים בקבוצות המכשירים הנגישות לו. שורה תואמת ל-`PeerPayload` (‏`flutter/lib/common/hbbs/hbbs.dart:77`): ‏`{"id","info":{"username","os","device_name"},"status","user","user_name","device_group_name","note"}`. ‏`user` הוא hex guid של המשתמש המשויך.
- `POST /api/devices/cli`: אימות `Authorization: Bearer <token>` של session של admin (API Tokens יגיעו בשלב 6). גוף `{id, uuid, user_name?, device_group_name?, note?, device_username?, device_name?, strategy_name?, address_book_name?, address_book_*?}`. אם `verify_peer(id, uuid)` נכשל, מחזירים טקסט `Device not found` (200, כי ה-client מדפיס את הגוף). שדות נתמכים מוחלים בטרנזקציה. אם נשלחו `strategy_name` או `address_book_name`, מחזירים טקסט `Applied; not supported yet: strategy_name, address_book_name` (רק את מה שנשלח). אחרת מחזירים גוף ריק, וה-client מדפיס "Done!".
- `POST /api/devices/deploy`: מחזיר `{"result":"NOT_ENABLED"}` (השרת לא דורש deploy).

- [ ] **Step 1: בדיקות כושלות**
  - `devices_list_includes_groups_and_overrides`: שם שהוחלף מופיע ב-`info.device_name`, וסינון לפי `device_group_name` עובד.
  - `enable_disable_device_sets_status`
  - `delete_device_requires_disabled_and_removes_everywhere`: אחרי המחיקה אין שורה ב-`peer`, ‏`sysinfo` או `ext`, ו-`pm.get_in_memory(id)` מחזיר `None`.
  - `assign_user_and_group_and_reject_unsupported`
  - `peers_accessible_for_non_admin`: רק מכשירים של משתמשים נגישים ושל קבוצות נגישות.
  - `cli_assign_with_matching_uuid_applies`, ‏`cli_wrong_uuid_changes_nothing`, ‏`cli_requires_admin_token`
  - `deploy_returns_not_enabled`
- [ ] **Step 2-4:** מריצים (נכשלות), מממשים, ומריצים שוב (עוברות). ‏clippy נקי.
- [ ] **Step 5:** Commit: `feat(console): device management, /api/peers, --assign cli and deploy`

---

### Task 7: ממשק - משתמשים וקבוצות משתמשים

**Files:** `console-ui/src/pages/UsersPage.tsx`, `console-ui/src/pages/UserGroupsPage.tsx`, `console-ui/src/components/{Modal,ConfirmDialog,RulesEditor}.tsx`, ‏`console-ui/src/layout/AppLayout.tsx`, ‏`console-ui/src/i18n/{he,en}.json`, ‏`console-ui/src/api.ts`, בדיקות ב-`console-ui/src/__tests__/`.

**Interfaces:**
- תפריט צד: "מכשירים" לכולם. "משתמשים", "קבוצות משתמשים" ו-"קבוצות מכשירים" רק כש-`is_admin` (מתוך `/api/console/me`).
- **משתמשים:** טבלה עם שם, שם תצוגה, אימייל, קבוצה, admin, סטטוס (פעיל/מושבת), נוצר. חיפוש לפי שם, סינון לפי קבוצה, עימוד. כפתור "משתמש חדש" פותח מודאל עם שם, סיסמה (לפחות 8), קבוצה, אימייל, הערה ו-admin. פעולות בשורה: עריכה (מודאל עם אותם שדות, סיסמה אופציונלית), הפעלה/השבתה, ניתוק מכל המכשירים (force logout), ומחיקה (רק מושבת, עם דיאלוג אישור). שגיאות שרת מוצגות כהודעה מתורגמת.
- **קבוצות משתמשים:** טבלה עם שם, הערה, מספר משתמשים, "יכולה לגשת אל" ו-"נגישה מ" (תגיות). יצירה ועריכה במודאל עם `RulesEditor`: בחירה מרובה מרשימת הקבוצות והמשתמשים, עם סוג מוצג. מחיקה עם אישור. ‏`Default` מוצגת עם תג "ברירת מחדל" ובלי כפתור מחיקה.
- `RulesEditor` הוא רכיב משותף שגם משימה 8 משתמשת בו: ‏`props: { value: Rule[], onChange, allowedTypes: number[] }`, כאשר `Rule = {type:number,name:string}`.
- `ConfirmDialog`: ‏`props: { open, title, message, confirmLabel, danger?, onConfirm, onCancel }`. ‏`Modal`: ‏`props: { open, title, onClose, children, footer }`, עם focus trap וסגירה ב-Esc.

- [ ] **Step 1: בדיקות כושלות (vitest):**
  - `UsersPage › admin sees users menu, non-admin does not`
  - `UsersPage › create user posts expected body and refreshes`
  - `UsersPage › delete button disabled for active user`
  - `UserGroupsPage › default group has no delete button`
  - `RulesEditor › only allowed types selectable`
  - בדיקות i18n הקיימות (זהות מפתחות, אין טקסט קשיח) עוברות.
- [ ] **Step 2-4:** מריצים (נכשלות), מממשים, ומריצים שוב (עוברות). גם `npm run build`.
- [ ] **Step 5:** Commit: `feat(console-ui): users and user groups management pages`

---

### Task 8: ממשק - עריכת מכשירים וקבוצות מכשירים

**Files:** `console-ui/src/pages/DevicesPage.tsx` (הרחבה), ‏`console-ui/src/pages/DeviceGroupsPage.tsx`, ‏`console-ui/src/components/DeviceEditDrawer.tsx`, ‏i18n, ובדיקות.

**Interfaces:**
- **מכשירים (admin):**
  - עמודת בחירה (checkbox) ובר פעולות אצווה: שיוך למשתמש, שיוך לקבוצת מכשירים, הפעלה והשבתה. כל פעולה שולחת בקשה לכל מכשיר נבחר ומציגה סיכום "הצליחו X / נכשלו Y".
  - פעולות בשורה: עריכה, שפותחת `DeviceEditDrawer` עם שם תצוגה, משתמש מערכת, הערה, משתמש משויך (בחירה מרשימה) וקבוצת מכשירים. מחיקה (רק מושבת, עם אישור).
  - עמודות נוספות: קבוצת מכשירים וקבוצת משתמש.
  - פילטר לפי קבוצת מכשירים.
  - לא-admin רואה את המסך הקיים בלי פעולות.
- **קבוצות מכשירים:** טבלה עם שם, הערה, מספר מכשירים ו-"נגישה מ". יצירה ועריכה עם `RulesEditor` (סוגים 0 ו-2), ומחיקה עם אישור (עם הסבר שהמכשירים יוסרו מהקבוצה). כפתור "ניהול מכשירים" פותח רשימה עם הוספה לפי ID והסרה.

- [ ] **Step 1: בדיקות כושלות:**
  - `DevicesPage › batch assign sends one request per selected device and shows summary`
  - `DevicesPage › delete disabled only`
  - `DeviceEditDrawer › empty user unassigns`
  - `DeviceGroupsPage › add device by id posts array`
  - i18n
- [ ] **Step 2-4:** מריצים (נכשלות), מממשים, ומריצים שוב (עוברות). ‏`npm run build`, ‏`cargo test console::web`.
- [ ] **Step 5:** Commit: `feat(console-ui): device editing, batch actions and device groups page`

---

### Task 9: בדיקת קבלה לשלב 2

**Files:** Modify `docs/webconsole/phase1-acceptance-check.py` (או יוצרים `phase2-acceptance-check.py` שמייבא ממנו). Create `docs/webconsole/phase2-acceptance.md` (עברית).

- [ ] **Step 1:** מריצים מקומית (תיקייה זמנית, בלי לגעת ב-client האמיתי או ב-Synology), עם token של admin, את הסקריפטים מ-`E:\RustDesk\res`, כל אחד על התאמה יחידה כדי שלא תוצג בקשת אישור:
  - `users.py`: ‏`new` (עם `--group_name Default`), ‏`view`, ‏`disable`, ‏`enable`, ‏`disable` ואז `delete`.
  - `user-groups.py`: ‏`create` עם `--access-to`, ‏`update`, ‏`add-users`, ‏`view-users`, ‏`delete`.
  - `device-groups.py`: ‏`create`, ‏`add-devices`, ‏`view-devices`, ‏`remove-devices`, ‏`delete`.
  - `devices.py`: ‏`assign --assign_to user_name=...`, ‏`assign --assign_to device_group_name=...`, ‏`disable`, ‏`enable`, ו-`view --offline_days 0` שלא קורס.
  - סימולציית `--assign` של ה-client: ‏`POST /api/devices/cli` עם id ו-uuid.
  - טאב הקבוצה של ה-client, כמשתמש לא-admin: ‏`/api/users?accessible=&status=1`, ‏`/api/peers?accessible=&status=1` ו-`/api/device-group/accessible` מחזירים בדיוק את מה שחוקי הגישה מתירים.
  - מחיקת מכשיר מושבת בזמן ש-hbbs רץ, ואז sysinfo מחדש מאותו id מחזיר `ID_NOT_FOUND` (כי השורה נמחקה), בלי panic בלוג.
- [ ] **Step 2:** כותבים את `phase2-acceptance.md`: טבלת תוצאות, וצ'קליסט ידני למשתמש (טאב "קבוצה" ב-client מציג משתמשים ומכשירים נגישים, ועוד מסכי הניהול בעברית ובאנגלית).
- [ ] **Step 3:** Commit: `docs(console): phase 2 acceptance results`
