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

### רושמים את שמות ה-containers וההגדרות הנוכחיות

לפני שנוגעים בכל דבר, חובה לרשום שני דברים.

**א. שמות ה-containers.** דרך SSH מריצים:

```bash
sudo docker ps
```

ורושמים את העמודה NAMES. בהתקנה classic יש שני containers (אחד מריץ `hbbs` ואחד `hbbr`, השמות שלהם לא בהכרח `hbbs` ו-`hbbr`). בהתקנה s6 יש container אחד בלבד שמריץ את שניהם. בהמשך המדריך מגדירים שני משתנים, ובכל הפקודות משתמשים בהם:

```bash
# classic: שני שמות שונים. מחליפים בשמות האמיתיים שלכם.
OLD_HBBS=<שם-ה-container-של-hbbs>
OLD_HBBR=<שם-ה-container-של-hbbr>

# s6: container אחד, אותו שם בשני המשתנים
# OLD_HBBS=<שם-הקונטיינר>
# OLD_HBBR=<שם-הקונטיינר>
```

המשתנים נשמרים רק בחלון ה-SSH הנוכחי. אם סוגרים אותו, מגדירים אותם שוב.

**ב. הפקודה והמשתני הסביבה של ה-container הישן.** ה-container החדש לא יורש אותם אוטומטית, ואם נשמיט דגל חשוב ההתנהגות של השרת (למשל כתובת ה-relay) עלולה להשתנות. רושמים אותם (בקובץ או במחברת):

```bash
sudo docker inspect --format '{{.Config.Cmd}} {{.Config.Entrypoint}}' "$OLD_HBBS"
sudo docker inspect --format '{{range .Config.Env}}{{println .}}{{end}}' "$OLD_HBBS"
sudo docker inspect --format '{{.Config.Cmd}} {{.Config.Entrypoint}}' "$OLD_HBBR"
sudo docker inspect --format '{{range .Config.Env}}{{println .}}{{end}}' "$OLD_HBBR"
```

אותו מידע מופיע ב-Container Manager → ה-container → **Details** (פקודה, Environment). דברים שחשוב לשים לב אליהם: דגלים כמו `-r your.host:21117` (כתובת ה-relay) או `-k _`, ומשתנים כמו `RELAY`, `RELAY_SERVERS`, `ENCRYPTED_ONLY`, `ALWAYS_USE_RELAY`. בסעיף 4 מעבירים אותם ל-container החדש.

**חשוב: איך מתרגמים את ההגדרות הישנות.** התוכנות `hbbs` ו-`hbbr` עצמן **לא קוראות** את המשתנים `RELAY` ו-`ENCRYPTED_ONLY`. ב-image של s6 רק סקריפט ההפעלה הפנימי שלו (`/etc/s6-overlay/s6-rc.d/hbbs/run`) הופך אותם לדגלים (`hbbs -r $RELAY` ו-`-k _`). ה-image החדש לא מריץ את הסקריפט הזה, ולכן אם רק מעתיקים את המשתנים, הם פשוט לא ישפיעו. מתרגמים כך:

| מה היה ב-container הישן | מה כותבים ב-container החדש |
|---|---|
| s6: `RELAY=x` | `command: hbbs -r x` (ב-hbbs) |
| s6: `ENCRYPTED_ONLY=1` | מוסיפים `-k _` **גם** לפקודה של hbbs **וגם** לפקודה של hbbr: `command: hbbs -r x -k _` ו-`command: hbbr -k _` |
| classic: פקודה עם דגלים (למשל `hbbs -r x -k _` או `hbbr -k _`) | מעתיקים את הדגלים **כמו שהם** לשורת `command` של אותו שירות |
| `RELAY_SERVERS`, `ALWAYS_USE_RELAY` ושאר המשתנים | מעתיקים כמו שהם ל-`environment` (התוכנות קוראות אותם ישירות) |

הערה למתקדמים: התוכנות קוראות גם משתני סביבה בשמות הדגלים עצמם, כך ש-`KEY=_` (ב-hbbs וב-hbbr) שקול ל-`-k _`, ו-`RELAY_SERVERS=x` (ב-hbbs) שקול ל-`-r x`. דגל בשורת הפקודה גובר על המשתנה. הדרך המומלצת היא בכל זאת הדגלים, כי רואים אותם מיד בשורת `command`.

### ג. מיפוי הפורטים של ה-container הישן

קובץ ה-compose החדש משתמש ב-`network_mode: host`: התוכנות מאזינות ישירות על הפורטים של ה-NAS, בלי מיפוי. הפורטים הרגילים: hbbs על 21115, 21116 (TCP ו-UDP) ו-21118, hbbr על 21117 ו-21119, והקונסולה על 21114. רושמים איך ה-container הישן היה מחובר לרשת:

```bash
sudo docker inspect --format '{{.HostConfig.NetworkMode}} {{json .HostConfig.PortBindings}}' "$OLD_HBBS"
sudo docker inspect --format '{{.HostConfig.NetworkMode}} {{json .HostConfig.PortBindings}}' "$OLD_HBBR"
```

(ב-Container Manager: ה-container → **Details** → לשונית **Network** ו-**Port Settings**.)

- אם מופיע `host`, או שכל פורט ממופה לאותו מספר בצד ה-NAS (למשל `21116` ל-`21116`), אין מה לשנות.
- אם בצד ה-NAS היו מספרים אחרים (למשל `31116` ל-`21116`), הלקוחות והנתב מכירים את המספרים האלה, וצריך לבחור אחת משתי האפשרויות:

**אפשרות א: נשארים ב-host ומשנים את הפורטים בדגלים.** מתאים כשהמספרים החדשים שומרים על אותם הפרשים כמו הרגילים (21115 עד 21119 עברו כולם באותה תוספת). הדגל `-p` של hbbs קובע את הפורט הראשי (במקום 21116), וממנו נגזרים הפורט הראשי פחות 1 (במקום 21115) והראשי ועוד 2 (במקום 21118). הדגל `-p` של hbbr קובע את פורט ה-relay (במקום 21117), והפורט שלו ועוד 2 משמש ל-websocket (במקום 21119). דוגמה לפורטים 31115 עד 31119:

```yaml
  hbbs:
    command: hbbs -p 31116 -r your.host:31117
  hbbr:
    command: hbbr -p 31117
```

(מוסיפים לזה את שאר הדגלים שלכם, למשל `-k _`.) הקונסולה נשארת על `API_PORT` (21114 כברירת מחדל), ואפשר לשנות גם אותו.

**אפשרות ב: עוברים למיפוי פורטים מפורש.** מוחקים משני השירותים את השורה `network_mode: host` ומוסיפים `ports:`, כשהמספר השמאלי הוא הפורט ב-NAS **כמו שהיה ב-container הישן**, והימני הוא הפורט הרגיל בתוך ה-container:

```yaml
  hbbs:
    ports:
      - "21114:21114"
      - "31115:21115"
      - "31116:21116"
      - "31116:21116/udp"
      - "31118:21118"
  hbbr:
    ports:
      - "31117:21117"
      - "31119:21119"
```

(הקונסולה חדשה, אז אין לה מיפוי ישן: `21114:21114` או כל פורט פנוי אחר בצד שמאל.) באפשרות ב כתובת ה-relay בדגל `-r` של hbbs היא הכתובת והפורט **החיצוניים** (למשל `your.host:31117`), כמו שהיו ב-container הישן.

## 2. גיבוי מלא של התיקייה

לפני כל דבר אחר. דרך SSH (מחליפים את נתיב התיקייה לנתיב שלכם). חשוב לעצור את ה-containers לפני ההעתקה, כי אחרת בסיס הנתונים נכתב בזמן ההעתקה והגיבוי עלול להיות פגום. הפקודות משורשרות עם `&&` כך שההעתקה לא תרוץ אם העצירה נכשלה:

```bash
cd /volume1/docker
sudo docker stop "$OLD_HBBS" "$OLD_HBBR" && \
  sudo cp -a rustdesk rustdesk-backup-$(date +%Y%m%d)
```

מוודאים ששני ה-containers באמת נעצרו (הרשימה צריכה להיות ריקה משמות ה-containers שלכם) ושהגיבוי קיים:

```bash
sudo docker ps
ls -la rustdesk-backup-*
```

ואז מפעילים אותם מחדש:

```bash
sudo docker start "$OLD_HBBS" "$OLD_HBBR"
```

או ב-File Station: מסמנים את התיקייה, לחיצה ימנית, **Copy**, ומדביקים לצידה בשם חדש (גם כאן קודם עוצרים את ה-containers ב-Container Manager).

חשוב במיוחד לשמור את `id_ed25519` ו-`id_ed25519.pub` (המפתחות: בלעדיהם כל המכשירים יצטרכו הגדרה מחדש) ואת `db_v2.sqlite3` (רשימת המכשירים). קבצים נוספים כמו `db_v2.sqlite3-wal` ו-`db_v2.sqlite3-shm` הם חלק מהגיבוי.

השרת החדש גם יוצר בעצמו גיבוי של בסיס הנתונים לפני שדרוג שלו, בשם `db_v2.sqlite3.bak-<מספר>`. זה נוסף על הגיבוי הידני ולא במקומו.

## 3. הרצת ניסיון על עותק (לא נוגעים בשרת האמיתי)

מטרה: לוודא שהגרסה החדשה עולה, רואה את המכשירים הקיימים והגיבוי האוטומטי נוצר, בלי לגעת בנתונים האמיתיים ובלי להפריע לשרת שרץ.

1. יוצרים עותק של **הגיבוי מסעיף 2** (לא של התיקייה החיה, כדי שהעותק יהיה עקבי):

```bash
cd /volume1/docker
sudo cp -a rustdesk-backup-YYYYMMDD rustdesk-trial
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

1. עוצרים את ה-containers הישנים ולא מוחקים אותם. מיד אחרי העצירה לוקחים עותק טרי של התיקייה (כדי שמכשירים שנרשמו מאז הגיבוי בסעיף 2 לא יאבדו אם תצטרכו לחזור אחורה), ומשנים לישנים שם כדי שלא יתנגשו בשמות החדשים:

```bash
sudo docker stop "$OLD_HBBS" "$OLD_HBBR" && \
  sudo docker ps && \
  cd /volume1/docker && \
  sudo cp -a rustdesk rustdesk-pre-switch-$(date +%Y%m%d)
```

   בודקים ש-`docker ps` לא מציג עוד את ה-containers הישנים. אם הם עדיין רצים, **לא ממשיכים**: הפורטים יתנגשו עם החדשים. אחר כך:

```bash
sudo docker rename "$OLD_HBBS" "${OLD_HBBS}-old"
# רק בהתקנה classic (אם יש שני containers שונים):
sudo docker rename "$OLD_HBBR" "${OLD_HBBR}-old"
```

2. מעבירים את ההגדרות הישנות. מעתיקים את הקובץ `docker/docker-compose.console.yml` ל-NAS ופותחים אותו לעריכה:
   - שורת ה-volume (בשני השירותים): מחליפים ל-`/volume1/docker/rustdesk:/root` (או לתיקייה האמיתית שלכם).
   - **משתני הסביבה והדגלים שרשמתם בסעיף 1ב**: מוסיפים אותם ב-`environment` ובשורת `command` לפי טבלת התרגום בסעיף 1ב (בקובץ יש דוגמאות בהערות, למשל `command: hbbs -r your.host:21117`). משתמשים בערכים **שלכם**, לא בדוגמה. אל תעתיקו את `RELAY` או `ENCRYPTED_ONLY` של s6 ל-`environment`: הם לא משפיעים, וצריך לתרגם אותם לדגלים.
   - **הפורטים**: לפי מה שמצאתם בסעיף 1ג (אם היו פורטים לא רגילים, אפשרות א או ב).

3. מפעילים:

```bash
sudo docker compose -f docker-compose.console.yml up -d
# בגרסאות DSM ישנות, אם הפקודה הקודמת נכשלת ב-"docker: 'compose' is not a docker command":
# sudo docker-compose -f docker-compose.console.yml up -d
sudo docker logs hbbs 2>&1 | head -50
```

   חלופה בלי שורת פקודה לקובץ ה-compose: **Container Manager → Project → Create**, בוחרים תיקייה (למשל `/volume1/docker/rustdesk-console`), מעלים או מדביקים את תוכן `docker-compose.console.yml` הערוך, ומסיימים את האשף. ה-Project מפעיל את שני ה-containers.

   אפשר גם דרך הממשק: Container Manager → Container → Create על ה-image `rustdesk-server-console:latest`, הפקודה (`hbbs` עם הדגלים הישנים שלכם, ובנפרד `hbbr` ל-container השני), רשת **host**, Volume של התיקייה האמיתית ל-`/root`, ומשתנה סביבה `API_PORT=21114` יחד עם שאר המשתנים הישנים.

4. בודקים: הממשק עולה על `http://<כתובת-ה-NAS>:21114`, ומכשירים מתחברים כרגיל.

5. **משווים את לוג ההפעלה של החדשים לישנים.** ה-containers הישנים רק נעצרו, ולכן הלוג שלהם עדיין זמין. בהפעלה, hbbs כותב ללוג את רשימת שרתי ה-relay בשורה מהצורה `relay-servers=["your.host:21117"]` ואת המפתח הציבורי בשורה `Key: <מפתח>`. hbbr כותב `Key: <מפתח>` **רק** כשמצב מפתח מופעל (`-k _` או `KEY`); בלי זה אין לו שורת `Key:` בכלל.

```bash
sudo docker logs "${OLD_HBBS}-old" 2>&1 | grep -E 'relay-servers=|Key: |ALWAYS_USE_RELAY='
sudo docker logs hbbs 2>&1 | grep -E 'relay-servers=|Key: |ALWAYS_USE_RELAY='
# classic בלבד (ב-s6 שורת ה-Key של hbbr כבר מופיעה בלוג של ה-container היחיד):
sudo docker logs "${OLD_HBBR}-old" 2>&1 | grep 'Key: '
sudo docker logs hbbr 2>&1 | grep 'Key: '
```

   מה צריך לראות:
   - שורת `relay-servers=[...]` של hbbs החדש זהה לישנה. אם בחדש מופיע `relay-servers=[]` ובישן הייתה כתובת, הדגל `-r` לא הועבר.
   - ערך ה-`Key:` זהה בין הישן לחדש (אותו `id_ed25519.pub`).
   - אם ב-container הישן הייתה ל-hbbr שורת `Key:` (כלומר `ENCRYPTED_ONLY=1` או `-k _`), גם ל-hbbr החדש חייבת להיות שורה כזו. אם היא חסרה, חסר `-k _` בפקודה של hbbr, וה-relay מקבל חיבורים בלי בדיקת מפתח.
   - `ALWAYS_USE_RELAY=` זהה בשניהם.

   אם `docker logs` לא מציג כלום, אותו לוג נמצא גם ב-Container Manager → ה-container → **Log** (מחפשים שם את אותן שורות). אם הלוג הישן כבר התגלגל ולא מכיל את שורות ההפעלה, משווים מול ההגדרות שרשמתם בסעיף 1ב.

כשהכל עובד כמה ימים, אפשר למחוק את ה-containers הישנים (בשמות `...-old`). לא כדאי למחוק מהר את תיקיות הגיבוי.

## 5. שחזור (אם משהו השתבש)

אם אפשר, חוזרים עם העותק הטרי מסעיף 4 (`rustdesk-pre-switch-...`), כי הוא כולל מכשירים שנרשמו אחרי הגיבוי הראשון. אם הוא לא קיים, משתמשים בגיבוי מסעיף 2. הפקודות מניחות את המשתנים `OLD_HBBS` ו-`OLD_HBBR` (מגדירים מחדש אם צריך), ושהחדשים נקראים `hbbs` ו-`hbbr` כמו בקובץ ה-compose:

```bash
sudo docker stop hbbs hbbr && sudo docker rm hbbs hbbr
sudo docker ps    # מוודאים שהחדשים נעלמו
# מחזירים את הנתונים:
cd /volume1/docker
sudo mv rustdesk rustdesk-failed
sudo cp -a rustdesk-pre-switch-YYYYMMDD rustdesk
# מחזירים את ה-containers הישנים לשמות המקוריים ומפעילים:
sudo docker rename "${OLD_HBBS}-old" "$OLD_HBBS"
# רק בהתקנה classic:
sudo docker rename "${OLD_HBBR}-old" "$OLD_HBBR"
sudo docker start "$OLD_HBBS" "$OLD_HBBR"
```

אם כבר מחקתם את ה-containers הישנים, יוצרים אותם מחדש מה-image הישן (`rustdesk/rustdesk-server` או `rustdesk/rustdesk-server-s6`) על התיקייה המשוחזרת, עם הפקודה והמשתנים שרשמתם בסעיף 1ב.

חשוב: אחרי הפעלת הגרסה החדשה בסיס הנתונים עבר שדרוג. הגרסה הישנה יכולה בדרך כלל לקרוא אותו, אבל השחזור הבטוח ביותר הוא מעותק שנלקח לפני ההפעלה (או מהקובץ `db_v2.sqlite3.bak-...` שנוצר אוטומטית).

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
