# Web Console - קטלוג פיצ'רים ומחקר

מסמך מחקר: מה ה-Web Console של RustDesk Server Pro עושה, איך הוא עובד, ומה התוכנה (client) בספרייה הזו כבר מצפה לקבל משרת.
מקורות: דפי התיעוד של Server Pro (console, permissions, admin-role, control-role, audit-logs, strategy, address-book, 2fa, relay, oidc, ldap, faq), הקוד ב-`src/` וב-`flutter/`, והסקריפטים ב-`res/*.py`.

---

## 1. התמונה הגדולה - איך זה בנוי

```
  מחשבים עם RustDesk (client)          דפדפן של מנהל
        |        |                            |
        |        | HTTP (פורט 21114)          | HTTP/HTTPS (פורט 21114)
        |        v                            v
        |   +-------------------------------------+
        |   |  API Server + Web Console           |  <- זה מה שבונים
        |   |  (משתמשים, מכשירים, לוגים, מדיניות) |
        |   +------------------+------------------+
        |                      | מסד נתונים משותף (SQLite)
        | TCP/UDP 21115-21119  |
        v                      v
   +-----------+          +-----------+
   |   hbbs    |<-------->|   hbbr    |
   | (מתווך)   |          | (ממסר)    |
   +-----------+          +-----------+
```

- **hbbs** (rendezvous): רושם מכשירים לפי ID ומחבר בין שני צדדים. ב-Pro הוא גם **אוכף** בקרת גישה ו-Control Roles: הוא שולח לצד הנשלט את ההרשאות בתוך הודעת `PunchHole`/`RequestRelay` (שדה `control_permissions`, ראו `libs/hbb_common/protos/rendezvous.proto:49`).
- **hbbr** (relay): מעביר תעבורה כשאין חיבור ישיר.
- **API Server / Console**: אתר + REST API בפורט 21114. ב-Pro הוא חלק מאותה תוכנת שרת וחולק איתה מסד SQLite (`/var/lib/rustdesk-server/`).
- **ה-client בספרייה הזו** הוא רק התוכנה על המחשבים. ה-Console לא קיים בה. הגרסה הפתוחה של השרת (`rustdesk/rustdesk-server`) כוללת רק hbbs+hbbr בלי API ובלי Console.

**מסקנה:** חלק גדול מה-Console הוא שרת API שה-client הקיים כבר יודע לדבר איתו. אבל בקרת גישה, Control Roles ובחירת relay לפי מיקום דורשים שינוי ב-**hbbs** עצמו.

---

## 2. מה ה-client כבר שולח ומצפה לקבל (חוזה קיים)

### 2.1 דיווח מכשיר (כל מחשב, ברקע) - `src/hbbs_http/sync.rs`
| קריאה | מתי | מה נשלח | מה מצופה בתשובה |
|---|---|---|---|
| `POST /api/sysinfo` | בהפעלה ואחרי שינוי | id, uuid, version, hostname, username, os, cpu, memory + preset-* (שם פנקס, תגית, משתמש, מדיניות, קבוצת מכשירים, הערה) | המחרוזת `SYSINFO_UPDATED` או `ID_NOT_FOUND` |
| `POST /api/sysinfo_ver` | לפני upload | ריק | גרסת sysinfo בשרת |
| `POST /api/heartbeat` | כל 3 שניות כשיש חיבור, אחרת כל 15 | id, uuid, ver, conns (מספרי חיבורים פעילים), modified_at (חותמת מדיניות) | JSON אופציונלי: `sysinfo` (בקשת העלאה מחדש), `disconnect: [conn_id..]` (ניתוק מה-Console), `strategy: {config_options}`, `modified_at` |
| `POST /api/switch-grant` | Switch Sides | id, verifier, timestamp, חתימה | `{accepted, server_time}` |

### 2.2 לוגים (נשלחים מהצד הנשלט) - `src/server/connection.rs`
| קריאה | מה |
|---|---|
| `POST /api/audit/conn` | פתיחה/סגירה של חיבור, סוג, צד שולט, שיטת אימות, הערה |
| `POST /api/audit/file` | העברת קבצים: כיוון, נתיב, גודל, רשימת קבצים |
| `POST /api/audit/alarm` | אזעקות אבטחה (IP whitelist, ניסיונות רבים, scope violation, וכו') |
| `PUT /api/audit` | עדכון הערה לחיבור (`flutter/lib/common/widgets/dialog.dart:1670`) |
| `POST /api/record` | העלאת הקלטות סשן (`src/hbbs_http/record_upload.rs`) |

### 2.3 חשבון משתמש ב-client - `flutter/lib/models/user_model.dart`, `src/hbbs_http/account.rs`
`POST /api/login`, `POST /api/logout`, `POST /api/currentUser`, `GET /api/login-options`, `POST /api/oidc/auth`, `GET /api/oidc/auth-query`, אימות 2FA/מייל בתוך זרימת ה-login.

### 2.4 רשימות ב-client - `flutter/lib/models/group_model.dart`, `ab_model.dart`
- `GET /api/users`, `GET /api/peers`, `GET /api/device-group/accessible` (לשונית "מכשירים נגישים")
- פנקס כתובות: `/api/ab/settings`, `/api/ab/personal`, `/api/ab/shared/profiles`, `/api/ab/peers`, `/api/ab/tags/{guid}`, `/api/ab/peer/add|update|{guid}`, `/api/ab/tag/add|rename|update|{guid}`, ו-`/api/ab` הישן (legacy)

### 2.5 פריסה משורת פקודה - `src/core_main.rs:616`, `src/ui_interface.rs:1154`
`POST /api/devices/cli` (`--assign --token ... --user_name --strategy_name --address_book_name --device_group_name --note`) ו-`/api/devices/deploy`.

### 2.6 API ניהול עם Token (הסקריפטים ב-`res/*.py`)
| תחום | נקודות קצה |
|---|---|
| משתמשים | `GET/POST /api/users`, `POST /api/users/{guid}/enable|disable`, `DELETE /api/users/{guid}`, `POST /api/users/invite`, `PUT /api/users/tfa/totp/enforce`, `PUT /api/users/disable_login_verification`, `POST /api/users/force-logout` |
| קבוצות משתמשים | `GET/POST /api/user-groups`, `PATCH/DELETE /api/user-groups/{guid}`, `POST /api/user-groups/{guid}` (חברים) |
| מכשירים | `GET /api/devices`, `POST /api/devices/{guid}/enable|disable`, `DELETE /api/devices/{guid}` + שיוך |
| קבוצות מכשירים | `GET/POST /api/device-groups`, `PATCH/DELETE /api/device-groups/{guid}`, `POST/DELETE /api/device-groups/{guid}/devices` |
| מדיניות | `GET /api/strategies`, `GET /api/strategies/{guid}`, `PUT /api/strategies/{guid}/status`, `POST /api/strategies/assign` |
| פנקס כתובות | `/api/ab/shared/add|update/profile`, `DELETE /api/ab/shared`, `/api/ab/rules`, `/api/ab/rule` |
| לוגים | `GET /api/audits/{conn|file|alarm|console}` עם סינון ועימוד |
| Admin/Control Roles | ראו `res/admin-roles.py`, `res/control-roles.py` |

---

## 3. קטלוג המסכים והפיצ'רים של ה-Console

### 3.1 כניסה וחשבון
- כניסה עם שם משתמש וסיסמה. משתמש `admin` נוצר בהתקנה, וחובה להחליף לו סיסמה.
- אימות במייל (קוד), או **TOTP 2FA** עם QR, 6 קודי גיבוי חד-פעמיים, ומצב "פג תוקף" אחרי 180 יום. כש-2FA פעיל, שינוי הגדרות חשבון דורש אימות נוסף.
- משתמש רגיל (לא admin) נכנס ורואה רק את המכשירים והלוגים שלו, ומשנה את ההגדרות של עצמו.
- כניסה חיצונית: **OIDC** (Azure, Okta, Google, GitHub, GitLab) ו-**LDAP** (host, port, Base DN, scope, Bind DN, filter, username attribute, StartTLS, NoTLSVerify; משתמש נוצר אוטומטית בכניסה הראשונה).

### 3.2 דף פתיחה - "הגדרות אוטומטיות"
- יצירת EXE לווינדוס שההגדרות מוטמעות בשם הקובץ, וקוד QR לאנדרואיד.

### 3.3 משתמשים
- יצירה, הזמנה במייל, עריכה (מייל, סיסמה, הערה), הפעלה/השבתה, מחיקה (רק אחרי השבתה), מתן הרשאת admin.
- שיוך לקבוצה, למדיניות, ל-Control Role ול-Admin Roles.
- ניהול 2FA: אכיפה, איפוס, ביטול אימות מייל. Force Logout.

### 3.4 קבוצות משתמשים
- יצירה ועריכה. "Access with other groups": **Can access to** ו-**Can be accessed from**.

### 3.5 מכשירים
- רשימה וחיפוש (`%` כתו כללי). עמודות: ID, שם, משתמש מערכת, OS, גרסה, מקוון/לא מקוון, משתמש משויך, קבוצת מכשירים, מדיניות (device/user/group), הערה.
- עריכה (שם, משתמש מערכת, הערה), שיוך למשתמש ולקבוצה, הפעלה/השבתה, מחיקה (רק אחרי השבתה), שיוך באצווה.

### 3.6 קבוצות מכשירים
- יצירה, הוספה והסרה של מכשירים (כל מכשיר בקבוצה אחת לכל היותר), מתן גישה למשתמשים או לקבוצות משתמשים.

### 3.7 בקרת גישה (מי יכול להתחבר למי)
- מכשיר משויך למשתמש ו/או לקבוצת מכשירים.
- גישה מותרת אם: המשתמש הוא הבעלים, או שהקבוצה שלו מורשית מול קבוצת הבעלים, **או** שקיבל גישה לקבוצת המכשירים (איחוד, מספיק שאחד מאשר).
- מכשיר מושבת או משתמש מושבת = חסום תמיד.
- הבדיקה היא לפי **המשתמש המחובר בצד השולט**, לא לפי המכשיר.
- **נאכף ב-hbbs** (דורש שינוי בשרת).

### 3.8 מדיניות (Strategies)
- אוסף הגדרות אבטחה של ה-client (רשימה חלקית מהקוד: `access-mode`, `enable-keyboard`, `enable-clipboard`, `enable-file-transfer`, `enable-audio`, `enable-camera`, `enable-terminal`, `enable-tunnel`, `enable-remote-restart`, `enable-record-session`, `enable-block-input`, `enable-privacy-mode`, `enable-remote-printer`, `allow-remote-config-modification`, `approve-mode`, `verification-method`, `temporary-password-length`, `allow-numeric-one-time-password`, `allow-hide-cm`, `enable-lan-discovery`, `direct-server`, `direct-access-port`, `whitelist`, `id-whitelist`, `allow-auto-disconnect`, `auto-disconnect-timeout`, `allow-remove-wallpaper`, `allow-only-conn-window-open`, `allow-scope-violation-close`, `allow-scope-violation-alarm`).
- שיוך למכשיר, למשתמש או לקבוצת מכשירים. עדיפות: מכשיר, אחריו משתמש, אחריו קבוצה, ואם אין אז ברירת מחדל.
- תפריט: הפעלה/השבתה, שינוי שם, שכפול, מחיקה, דיאלוגים לשיוך. מונה של כמה מכשירים בפועל משתמשים במדיניות.
- סנכרון דרך heartbeat (שדות `strategy` ו-`modified_at`), תוך 30 שניות.
- ה-client **חוסם** מפתחות רגישים מגיעים ממדיניות (שרת, מפתח, 2FA וכו'), ראו `BLOCKED_CONFIG_OPTIONS` ב-`sync.rs:295`.

### 3.9 Control Roles (מה מותר בתוך סשן)
- 13 הרשאות: מקלדת/עכבר, מדפסת מרוחקת, לוח, העברת קבצים, אודיו, מצלמה, טרמינל, TCP Tunnel, אתחול מרוחק, הקלטה, חסימת קלט, שינוי הגדרות מרחוק, מצב פרטיות (ובפרוטו גם `usb`).
- כל הרשאה במצב אחד מתוך שלושה: Use Client Settings, Enable, Disable. Enable/Disable דורסים את ההגדרה המקומית.
- תפקידים מיוחדים: **Not Logged** (שולט לא מחובר) ו-**Default** (מחובר בלי תפקיד). תפקיד מושבת = בלי עקיפה.
- כלל מיוחד ל-"שינוי הגדרות מרחוק" כשיש כמה שולטים: Disable אחד מנצח.
- **נשלח ע"י hbbs** בתוך `control_permissions` (דורש שינוי בשרת).

### 3.10 Admin Roles (ניהול מואצל)
- שדות: שם, סוג (Global / Individual / Group Scoped), Scope, הרשאות.
- הרשאות לפי תחום: Users, Devices, User Groups, Device Groups, Audit Logs, Strategies, Control Roles, Custom Clients (פירוט מלא בדף התיעוד).
- כללים: Edit כולל View; Edit לא כולל Assign; View של קבוצה לא חושף חברים; לא-admin לא יכול לערוך admin. משתמש עם כמה תפקידים מקבל את איחוד ההרשאות.

### 3.11 פנקס כתובות
- פנקס אישי לכל משתמש, ופנקסים משותפים עם הרשאות Read-only, Read/Write או Full Control, לפי משתמש, קבוצה או "כולם" (הספציפי מנצח).
- רשומה: ID, כינוי, תגיות, הערה, סיסמה (בפנקס משותף). סיסמה ברמת הפנקס או ברמת הרשומה (הרשומה מנצחת). ה-Console מציג רק אם יש סיסמה, לא את הערך.
- סל מחזור לכל פנקס, תגיות עם צבע, ייבוא מרשימת המכשירים.
- הגדרה: לאפשר ללא-admin לשתף פנקסים לקבוצות אחרות.

### 3.12 לוגים
- **Connection**: סוג, מכשיר נשלט, צד שולט (משתמש, מכשיר, IP), התחלה, סוף, משך, אימות (+2FA), הערה, וכפתור **Disconnect** לחיבור פעיל.
- **File**: מכשיר, צד שולט, זמן, כיוון, נתיב, פירוט (מגירה עם עד 10 הקבצים הגדולים).
- **Alarm**: סוג, מקור, יעד, זמן (7 סוגי אזעקות חיבור ו-3 סוגי אזעקות כניסה).
- **Console**: סוג, משתמש, פעולה, זמן, פירוט (כל פעולה ניהולית).
- סינון לפי זמן התחלה, ייצוא CSV (עד 1,000 שורות), הגדרת "רק admin רואה לוגים", שמירה לפי ימים (ניקוי כל שעה).

### 3.13 הגדרות
- **Relay**: רשימת שרתי relay (IP/DNS), Geo Override (lat/lon), Reload Geo (GeoLite2). hbbs בוחר את ה-relay הקרוב והמקוון. **דורש שינוי ב-hbbs.**
- **License**: רלוונטי רק למוצר המסחרי. אצלנו לא נדרש.
- **SMTP**: שרת דואר להזמנות ולאימות.
- **Tokens**: יצירת API tokens עם הרשאות (Device, Audit Log, User, Group, Strategy, Address Book, Admin Role, Control Role).
- **Other**: שמירת לוגים, "רק admin רואה לוגים", שיתוף פנקסים ללא-admin, הצגת "מכשירים נגישים" ב-client.
- **OIDC / LDAP**: ראו 3.1.

### 3.14 Custom Client Generator
- בניית client ממותג (שם, לוגו, אייקון, שרת מוטמע, הגדרות ברירת מחדל ונעולות, הסתרת UI, חתימה) לווינדוס, מק, לינוקס ואנדרואיד. דורש מערכת build בענן (CI). הכי כבד מכל הרשימה.

### 3.15 מחוץ ל-Console אבל באותו מוצר
- WebSocket (`/ws/id`, `/ws/relay`) ו-web client בכתובת `/web`.
- כלי שורת פקודה `rustdesk-utils` (איפוס סיסמה, 2FA, מפתחות, doctor).

---

## 4. פערים במידע
התיעוד לא מתאר: מבנה JSON מדויק של רוב התשובות (חלק אפשר לגזור מקוד ה-client ומהסקריפטים ב-`res/`), התנהגות ברירת מחדל של בקרת גישה כשאין חוקים, וכל שדות ה-Custom Client. בכל מקום כזה נגזור מהקוד או נחליט בעצמנו ונסמן את זה.
