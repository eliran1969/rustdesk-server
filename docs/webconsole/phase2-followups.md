# Phase 2 — decisions and deferred follow-ups

Carried over from the Phase 2 build ledger (subagent-driven build, 2026-11-15). Every item was reviewed; none blocks Phase 2.

## Important to know

- **Disabling a device and access rules do not block connections yet.** They only change what the console and the client's Group tab show. Enforcement in hbbs is Phase 5. An online device that is deleted re-registers when it reconnects.
- **Rebuild the Docker image** before deploying (Cargo.toml changed: tower `load-shed` feature).
- **The self-service password hint** points to a "Change password" entry; verify it exists in the UI menu outside the forced first-login flow.

## Decisions (rulings) made during the build

- Ruling Q1: accessible-users computation lives in one function in src/console/db/access.rs (created in T3) and T6 imports it — avoids duplication — cost: trivial move.
- Final: parked — fail-closed test for /api/peers can't fail against old code — Ruling: defensive fix is correct; add a forced-None unit test later
- Final: parked — selfPasswordHint references a "Change password" menu entry that may not exist outside the forced flow — Ruling: verify in browser; add a self-service change-password link in Phase 3 if missing

## Deferred minor findings

- Task 1: minor (deferred): web.rs `!Assets::get(rel).is_some()` → is_none (clippy nonminimal_bool)
- Task 1: minor (deferred): cli_set_password doesn't strip `?query` from sqlite URLs
- Task 1: minor (deferred): ensure_initial_admin hashes on every startup synchronously
- Task 2: minor (deferred): duplicate-name detection by message text (prefer DatabaseError::code 2067)
- Task 2: minor (deferred): groups::list page_size not clamped at DB layer (API must clamp)
- Task 3: minor (deferred): non-admins in accessible mode see others' note/email (blank for non-admin later)
- Task 3: minor (deferred): accessible set bound as IN(?...) — SQLite param limit at huge scale; join CTE instead
- Task 3: minor (deferred): BUSY_SNAPSHOT on concurrent admin writes → 500 (use BEGIN IMMEDIATE)
- Task 3: minor (deferred): admin password reset doesn't set must_change_password
- Task 4: minor (deferred): update() lookup before begin() (TOCTOU with concurrent delete); add-users array uncapped; N+1 in list
- Task 5: minor (deferred): accessible device groups filtered/paginated in Rust (loads all); N+1 in list; update lookup before tx
- Task 6: minor (deferred, should fix in final wave): /api/peers restriction keyed on Some/None of access helpers — fail open if a helper ever returns None for non-admin; key on !is_admin, treat None as empty
- Task 6: minor (deferred): delete vs in-flight register_pk race self-heals; add comment
- Task 6: minor (deferred): assign allows disabled users (inconsistent with sysinfo preset path)
- Task 7: minor (deferred): nested modals both close on Esc; null server fields → "" in PATCH diff; fixed 30ms settle in tests; RulesEditor ≤100 options; self row by name
- Task 8: minor (deferred, cheap — final wave): manage-devices list uses LIKE on group name → filter client-side by exact device_group_name
- Task 8: minor (deferred): unused i18n key devices.columns.select; drawer stale snapshot vs auto-refresh; 100vh→100dvh; row enable button not busy-disabled
- Task 9: minor (deferred): weak check `devices.py view D7 after delete`; res path hardcoded (md rerun lacks --res-dir)
