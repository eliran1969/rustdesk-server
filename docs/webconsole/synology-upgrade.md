# שדרוג בטוח של שרת RustDesk ב-Synology לגרסה עם קונסולת ניהול

מדריך זה מסביר, צעד אחר צעד, איך מחליפים את ה-image הקיים של שרת RustDesk ב-image החדש (`rustdesk-server-console`) שכולל ממשק ניהול בדפדפן, בלי לאבד מכשירים ובלי סיכון. העיקרון: **קודם גיבוי, אחר כך ניסיון על עותק, ורק בסוף החלפה אמיתית.** אם משהו משתבש, יש דרך חזרה.

המדריך מניח שהשרת רץ ב-Container Manager של DSM כשני containers: `hbbs` ו-`hbbr`.

## 0. מה צריך להכין

- ה-image החדש `rustdesk-server-console:latest` זמין ב-NAS (בנייה: `docker buildx build -f docker/Dockerfile.console -t rustdesk-server-console:latest --load .`, או טעינה מקובץ tar).
- גישת SSH או Terminal ל-NAS (אפשר גם לעשות חלק מהדברים מתוך Container Manager ו-File Station).
- עשר דקות של זמן שקט. בזמן ההחלפה האמיתית השרת לא זמין לכמה שניות.

## 1. איך מוצאים את תיקיית הנתונים הנוכחית

1. פותחים **Container Manager → Container**.
2. לוחצים על ה-container של `hbbs`, ואז **Details** ולשונית **Volume**.
3. בטבלה יש עמודה "Mount path" (הנתיב בתוך ה-container) ועמודה עם התיקייה ב-NAS. התיקייה ב-NAS היא **תיקיית הנתונים** שלכם, למשל `/volume1/docker/rustdesk`.

**איך יודעים איזה סוג התקנה יש לכם?** לפי ה-Mount path:

| Mount path בתוך ה-container | סוג ההתקנה | מה לעשות ב-image החדש |
|---|---|---|
| `/root` | classic (image בשם `rustdesk/rustdesk-server`, שני containers) | ממפים את אותה תיקייה ל-`/root`, בדיוק כמו היום |
| `/data` | s6 (image בשם `rustdesk/rustdesk-server-s6`, container אחד) | ממפים את **אותה תיקיית נתונים** ל-`/root` (לא ל-`/data`) |

ה-image החדש תמיד משתמש ב-`/root`. בשני המקרים התיקייה עצמה נשארת אותה תיקייה, ורק היעד שאליו ממפים אותה משתנה.

בתוך התיקייה צריכים להיות הקבצים:

```
id_ed25519
id_ed25519.pub
db_v2.sqlite3
```

אם יש לכם התקנה s6 עם container אחד שמריץ גם hbbs וגם hbbr, בגרסה החדשה יהיו שני containers (אחד עם הפקודה `hbbs` ואחד עם `hbbr`), כמו בקובץ ה-compose שמצורף.

## 2. גיבוי מלא של התיקייה

לפני כל דבר אחר. דרך SSH (מחליפים את הנתיב לנתיב שלכם):

```bash
cd /volume1/docker
sudo docker stop hbbs hbbr
sudo cp -a rustdesk rustdesk-backup-$(date +%Y%m%d)
ls -la rustdesk-backup-*
sudo docker start hbbs hbbr
```

או ב-File Station: מסמנים את התיקייה, לחיצה ימנית, **Copy**, ומדביקים לצידה בשם חדש.

חשוב במיוחד לשמור את `id_ed25519` ו-`id_ed25519.pub` (המפתחות: בלעדיהם כל המכשירים יצטרכו הגדרה מחדש) ואת `db_v2.sqlite3` (רשימת המכשירים). קבצים נוספים כמו `db_v2.sqlite3-wal` ו-`db_v2.sqlite3-shm` הם חלק מהגיבוי. עוצרים את ה-containers לפני ההעתקה כדי לקבל גיבוי עקבי.

השרת החדש גם יוצר בעצמו גיבוי של בסיס הנתונים לפני שדרוג שלו, בשם `db_v2.sqlite3.bak-<מספר>`. זה נוסף על הגיבוי הידני ולא במקומו.

## 3. הרצת ניסיון על עותק (לא נוגעים בשרת האמיתי)

מטרה: לוודא שהגרסה החדשה עולה, רואה את המכשירים הקיימים והגיבוי האוטומטי נוצר, בלי לגעת בנתונים האמיתיים ובלי להפריע לשרת שרץ.

1. יוצרים עותק של התיקייה:

```bash
cd /volume1/docker
sudo cp -a rustdesk rustdesk-trial
```

2. מריצים container ניסיוני **על העותק**, בפורטים חלופיים (השרת האמיתי ממשיך לרוץ על הפורטים הרגילים). הפורט הפנימי של הממשק נקבע ב-`API_PORT=31114`, והפורטים של השרת ממופים מ-31115 עד 31119:

```bash
sudo docker run -d --name hbbs-trial \
  -e API_PORT=31114 \
  -v /volume1/docker/rustdesk-trial:/root \
  -p 31114:31114 \
  -p 31115:21115 -p 31116:21116 -p 31116:21116/udp \
  -p 31117:21117 -p 31118:21118 -p 31119:21119 \
  rustdesk-server-console:latest hbbs
```

3. בודקים:

```bash
sudo docker logs hbbs-trial 2>&1 | head -50
```

   - מחפשים את השורה `Console initial admin password: ...` (ראו סעיף 8). אם כבר קיים משתמש admin בבסיס הנתונים, שורה כזו לא תופיע, וזה תקין.
   - בודקים שנוצר גיבוי: `ls /volume1/docker/rustdesk-trial` צריך להראות קובץ `db_v2.sqlite3.bak-...`.
   - פותחים בדפדפן `http://<כתובת-ה-NAS>:31114`, מתחברים עם `admin` והסיסמה, ובודקים שרשימת המכשירים הקיימים מופיעה.
   - בודקים שהמפתח הציבורי לא השתנה: `sha256sum rustdesk/id_ed25519.pub rustdesk-trial/id_ed25519.pub` צריך להחזיר אותו ערך בשניהם.

4. מנקים את הניסוי:

```bash
sudo docker rm -f hbbs-trial
sudo rm -rf /volume1/docker/rustdesk-trial
```

אם משהו בניסוי לא עבד, **עוצרים כאן** ולא עוברים להחלפה האמיתית.

## 4. ההחלפה האמיתית

1. עוצרים את ה-containers הישנים (`hbbs`, `hbbr`) ולא מוחקים אותם. משנים להם שם כדי שלא יתנגשו בשם ה-containers החדשים, וכך אפשר לחזור אחורה בקלות:

```bash
sudo docker stop hbbs hbbr
sudo docker rename hbbs hbbs-old
sudo docker rename hbbr hbbr-old
```

2. מפעילים את החדשים על **התיקייה המקורית**. הדרך הפשוטה היא עם הקובץ `docker/docker-compose.console.yml`: מעתיקים אותו ל-NAS ומשנים בו את שורת ה-volume (בשני השירותים) לתיקייה האמיתית, למשל `/volume1/docker/rustdesk:/root`. אחר כך:

```bash
sudo docker compose -f docker-compose.console.yml up -d
sudo docker logs hbbs 2>&1 | head -50
```

   אפשר גם דרך הממשק: Container Manager → Container → Create על ה-image `rustdesk-server-console:latest`, הפקודה `hbbs` (ובנפרד `hbbr` ל-container השני), רשת **host**, Volume של התיקייה האמיתית ל-`/root`, ומשתנה סביבה `API_PORT=21114`.

3. בודקים: הממשק עולה על `http://<כתובת-ה-NAS>:21114`, ומכשירים מתחברים כרגיל.

כשהכל עובד כמה ימים, אפשר למחוק את `hbbs-old` ו-`hbbr-old`. לא כדאי למחוק מהר את תיקיית הגיבוי.

## 5. שחזור (אם משהו השתבש)

```bash
sudo docker stop hbbs hbbr
sudo docker rm hbbs hbbr
# מחזירים את הנתונים מהגיבוי:
cd /volume1/docker
sudo mv rustdesk rustdesk-failed
sudo cp -a rustdesk-backup-YYYYMMDD rustdesk
# מחזירים את ה-containers הישנים:
sudo docker rename hbbs-old hbbs
sudo docker rename hbbr-old hbbr
sudo docker start hbbs hbbr
```

אם כבר מחקתם את ה-containers הישנים, יוצרים אותם מחדש מה-image הישן (`rustdesk/rustdesk-server` או `rustdesk/rustdesk-server-s6`) על התיקייה המשוחזרת, עם אותן הגדרות כמו קודם.

חשוב: אחרי הפעלת הגרסה החדשה בסיס הנתונים עבר שדרוג. השחזור הבטוח ביותר הוא מהגיבוי שעשיתם בסעיף 2 (או מהקובץ `db_v2.sqlite3.bak-...` שנוצר אוטומטית).

## 6. Reverse proxy ב-DSM (גישה ב-HTTPS על פורט 443)

כדי להגיע לקונסולה בכתובת נוחה עם תעודה, בלי מספר פורט:

1. DSM → **Control Panel → Login Portal → Advanced → Reverse Proxy → Create**.
2. Source: פרוטוקול HTTPS, שם מארח (למשל `rustdesk.example.com`), פורט 443.
3. Destination: פרוטוקול HTTP, מארח `localhost`, פורט `21114`.
4. מוודאים שיש תעודה (Control Panel → Security → Certificate) המשויכת לשם המארח.

### הגדרת TRUST_PROXY

אם הקונסולה נגישה **אך ורק** דרך ה-reverse proxy של DSM (קפיצה אחת, proxy אחד), מוסיפים ל-container של hbbs את משתנה הסביבה:

```
TRUST_PROXY=Y
```

כך השרת מזהה את כתובת ה-IP האמיתית של המשתמש מהכותרת `X-Forwarded-For` (הוא סומך על הערך הימני ביותר) לצורך הגבלת ניסיונות כניסה. **אל תגדירו `TRUST_PROXY=Y` אם אפשר להגיע לפורט 21114 ישירות מבחוץ**, כי אז כל אחד יכול לזייף כתובת. כשמגדירים אותו, לא חושפים את פורט 21114 לאינטרנט (בלי Port Forward בנתב, ובלי לפתוח אותו בחומת האש של DSM לכתובות חיצוניות). רק ה-reverse proxy (443) נחשף.

אם אתם לא משתמשים ב-reverse proxy, משאירים את `TRUST_PROXY` לא מוגדר.

## 7. הגדרת הלקוח (RustDesk client)

בלקוח RustDesk: **Settings → Network → ID/Relay server** (ייתכן שצריך ללחוץ "Unlock network settings"). בשדה **API Server** מזינים את כתובת הקונסולה:

- עם reverse proxy: `https://rustdesk.example.com`
- בלי reverse proxy: `http://<כתובת-ה-NAS>:21114`

שדות ה-ID server, ה-Relay server וה-Key נשארים כמו היום. המפתח הציבורי הוא התוכן של `id_ed25519.pub`.

## 8. סיסמת אדמין ראשונית ואיפוס סיסמה

בהפעלה הראשונה נוצר משתמש `admin` עם סיסמה אקראית, שמודפסת **פעם אחת** ללוג של ה-container של hbbs, בשורה:

```
Console initial admin password: ...
```

לצפייה: Container Manager → hbbs → Log, או:

```bash
sudo docker logs hbbs 2>&1 | grep "initial admin password"
```

מתחברים עם `admin` והסיסמה הזו, ומחליפים אותה מיד בממשק.

**שכחתם את הסיסמה?** מאפסים אותה משורת הפקודה (שם ה-container הוא `hbbs`):

```bash
sudo docker exec hbbs rustdesk-utils set_password admin <הסיסמה-החדשה>
```

הפקודה עובדת על בסיס הנתונים בתיקיית העבודה של ה-container (`/root/db_v2.sqlite3`), ולכן אין צורך בפרמטרים נוספים.
