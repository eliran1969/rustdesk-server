# Phase 1 — decisions and deferred follow-ups

Carried over from the Phase 1 build ledger (subagent-driven build, 2026-11-15). Every item was reviewed; none blocks Phase 1. Items marked **before live upgrade** should be done before switching the real Synology server.

## Before live upgrade

- **Synology guide §4 step 5:** the hbbr `Key:` log check is labelled classic-only, but s6 users with `ENCRYPTED_ONLY=1` need it most (it is the check that catches a missing `-k _` on hbbr). Label only the `${OLD_HBBR}-old` line as classic-only, and note that two `Key:` lines in the old single s6 log means key mode was on.
- **Rebuild the Docker image** before deploying (Cargo.toml gained the `tower` dependency after the last image build).

## Decisions (rulings) made during the build

- Ruling P1: T1's `test_app()` builds the router on a pool without `migrate`; T2 adds the `migrate` call into both `start` and `test_app()` — migrate does not exist until T2 — cost if wrong: trivial edit.
- Ruling P2: test helpers are added by the first task that needs them (`insert_peer` T2+, `insert_user` T3, `login_token` T4) in `src/console/test_util.rs` — avoids T1 depending on later code — cost: none.
- Ruling P3: `pub(crate) enum SessionKind { Web, Client }` lives in `src/console/db/sessions.rs`, stored as 'web'/'client' — matches schema `kind` column — cost: rename.
- Ruling P4: client IP read from request extension `ConnectInfo<SocketAddr>` (server uses `into_make_service_with_connect_info`); tests insert the extension; if absent fall back to 127.0.0.1 — keeps tests real — cost: small.
- Ruling P5: T9 split — controller runs everything automatable locally (hbbs+hbbr+`res/devices.py`, simulated client via HTTP using a real registered peer); steps that need the user's real machines become a checklist in `phase1-acceptance.md` for the user — cost: user does final manual check.
- Task 1: Ruling: plan-mandated test `start_survives_port_in_use` can't fail → replace with a test on an extracted `bind(addr) -> Result` that asserts Err on a held 0.0.0.0 port; no env mutation — spec requires failure isolation be verified — cost: small
- Task 2: Ruling: fixture privacy finding (plan-mandated) — fixture is upstream's own tracked db_v2.sqlite3 (2 rows from 2022, LAN IPs, public in rustdesk/rustdesk-server since commit 538aef7), not the user's data; keep — cost if wrong: none (already public)
- Task 4: Ruling (revises P4): with TRUST_PROXY=Y use the RIGHTMOST X-Forwarded-For entry (added by the trusted proxy), not the first — leftmost is client-controlled and bypasses the limiter behind nginx/Caddy/DSM proxy — cost: none
- Task 9: Ruling: installed RustDesk client at C:\Program Files\RustDesk is the owner's real client — never reconfigure it; client simulated via HTTP per sync.rs — cost: owner runs the real-client checklist
- Final: Ruling: panic strategy = keep upstream `panic = 'abort'` (no upstream divergence) + module-level clippy deny (unwrap_used, expect_used, panic, indexing_slicing; allowed in tests) + clippy run as verification + constraint documented — cost if wrong: a future panic path slips past clippy and kills hbbs
- Final: Ruling: T2 WAL concern closed — sqlx 0.6 default already WAL, hbbs was already WAL
- Final: parked — REQUEST_TIMEOUT comment wrong (concurrency wait unbounded, queues not rejects) — Ruling: real, harmless at Phase 1 scale; fix comment / add load_shed in Phase 2
- Final: parked — change-password writes not in one transaction (now more likely with 30s timeout) — Ruling: real, low impact; fix in Phase 2 (already T4 minor)
- Final: parked — 408 for handler timeout should be 503/504 — Ruling: real, cosmetic; Phase 2
- Final: parked — guide §4 step 5 marks hbbr Key check classic-only; s6 users with ENCRYPTED_ONLY skip the one check that catches missing -k _ — Ruling: real and load-bearing for the live upgrade → surface to owner as a must-do doc fix before switching
- Final: parked — hbbr PORT env quirk (listens PORT+1) worth a guide line — Ruling: Phase 2 doc

## Deferred minor findings (for Phase 2 or later)

- Task 1: minor (deferred): test_pm temp files not cleaned (use tempfile)
- Task 1: minor (deferred): no catch_panic layer on router (handler panic drops conn instead of 500)
- Task 1: minor (deferred): tokio dev-dep rt-multi-thread unneeded
- Task 1: minor (deferred): #[allow(dead_code)] scaffolding on ConsoleState/ApiError to remove later
- Task 2: minor (deferred): wal_checkpoint busy result ignored (copy could miss WAL under concurrent hbbs writes) — consider log/fail-if-busy
- Task 2: minor (deferred): unparsable schema_version silently → 0 (add warn)
- Task 2: minor (deferred): no test for backup-failure abort / rollback atomicity
- Task 2: minor (deferred): open_pool switches live hbbs DB to WAL — verify manually against running hbbs (Task 8/9)
- Task 3: minor (deferred): ensure_initial_admin count+insert not atomic (use insert ... where not exists)
- Task 3: minor (deferred): CLI set_password creates DB when missing → should refuse "Database not found"
- Task 3: minor (deferred): utils.rs:379 long line; errors via println
- Task 4: minor (deferred): verify-semaphore acquire has no timeout (spread attack can stall real logins) → timeout ~10s → 429/503
- Task 4: minor (deferred): IPv6 rate-limit keyed per address, not /64
- Task 4: minor (deferred): change-password hash update + session delete not in one transaction
- Task 4: minor (deferred): sessions never expire; disabled user's sessions not deleted (Phase 2 disable-user must delete sessions; consider idle expiry)
- Task 4: minor (deferred): console_session.peer_id stored even when uuid didn't match — never treat as proof of device ownership
- Task 4: minor (deferred): no 72-byte password max (bcrypt truncation)
- Task 4: minor (deferred): single trusted proxy hop only (document in synology guide)
- Task 5: minor (deferred): heartbeat does SELECT+UPDATE every 3s per active device — throttle last_online writes (~10s) if scale grows
- Task 6: minor (deferred): missing tests — non-admin + wildcard filter scoping, malformed sysinfo → {}, status null→1
- Task 7: minor (deferred): missing /assets/* file falls back to index.html 200 (should 404)
- Task 7: minor (deferred): path-traversal test asserts weakly
- Task 7: minor (deferred): no-hardcoded-text test misses attribute literals (placeholder/aria-label/title/alt) and wrapped text
- Task 7: minor (deferred): setToken lacks try/catch
- Task 7: minor (deferred): two hardcoded colors in styles.css (row hover, online ring)
- Task 7: minor (deferred): he.json tooManyAttempts wording (period, gender-neutral)
- Task 7: minor (deferred): no tests for auto-refresh/relativeTime/pagination
- Task 8: minor (deferred): LAN clients can reach 21114 directly and spoof XFF when TRUST_PROXY=Y (rate-limit evasion only); older DSM uses docker-compose
- Task 8: minor (deferred): guide doesn't mention host-network vs old port-mapping difference; digest pins need a refresh procedure; placeholder `<...>` lines error if pasted unedited
- Task 9: minor (deferred): Synology UI paths in acceptance md unverified on device; summary count hardcoded in md
