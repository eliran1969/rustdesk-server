#!/usr/bin/env python3
"""Phase 2 acceptance check for the hbbs web console (stdlib only; reuses the phase 1 helpers).

Run:  python -I docs/webconsole/phase2-acceptance-check.py [--skip-build] [--res-dir PATH]
Starts hbbr+hbbs in a fresh temp dir (DB_URL inside it), drives the res/*.py admin scripts
(users.py, user-groups.py, device-groups.py, devices.py) against it, simulates the client over
HTTP, prints PASS/FAIL per check, stops everything and deletes the temp dir.
Exit code 0 only if every check passed. Never touches an installed client or a real server.
"""
import argparse, base64, importlib.util, json, os, re, shutil, socket, sqlite3, subprocess, sys, tempfile, time

HERE = os.path.dirname(os.path.abspath(__file__))
_spec = importlib.util.spec_from_file_location("phase1_acceptance", os.path.join(HERE, "phase1-acceptance-check.py"))
p1 = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(p1)
check, http, REPO, BASE = p1.check, p1.http, p1.REPO, p1.BASE

NEW_ADMIN_PW = "Phase2-Admin-Pw-1"
USER_PW = "Phase2-User-Pw-12345"
RES = r"E:\RustDesk\res"


def script(name, *args, stdin=""):
    """Runs res/<name>.py with the admin token; stdin is empty so any confirmation prompt fails loudly."""
    r = subprocess.run([sys.executable, "-I", os.path.join(RES, name + ".py"), "--url", BASE, "--token", TOKEN[0]] + list(args),
                       capture_output=True, text=True, timeout=60, input=stdin)
    return r.returncode, (r.stdout + r.stderr).strip()


TOKEN = [None]


def sc(label, name, args, expect_in=(), expect_rc=0):
    rc, out = script(name, *args)
    ok = rc == expect_rc and all(s in out for s in expect_in)
    check(label, "exit %d%s" % (expect_rc, (", output has " + " / ".join(expect_in)) if expect_in else ""),
          "exit %d: %s" % (rc, out.replace("\n", " | ")[:260]), ok)
    return out


def names(body):
    return [d.get("name") for d in body.get("data", [])] if isinstance(body, dict) else body


def run():
    global RES
    ap = argparse.ArgumentParser()
    ap.add_argument("--skip-build", action="store_true")
    ap.add_argument("--res-dir", default=RES)
    args = ap.parse_args()
    RES = args.res_dir
    exe = ".exe" if os.name == "nt" else ""
    rel = os.path.join(REPO, "target", "release")

    if not args.skip_build:
        if not os.path.isfile(os.path.join(REPO, "console-ui", "dist", "index.html")):
            subprocess.run("npm ci && npm run build", shell=True, cwd=os.path.join(REPO, "console-ui"), check=True)
        r = subprocess.run(["cargo", "build", "--release", "--bin", "hbbs", "--bin", "hbbr"], cwd=REPO)
        check("cargo build --release (hbbs, hbbr)", "exit 0", r.returncode, r.returncode == 0)
        if r.returncode:
            return

    busy = []
    for port in range(21114, 21120):
        for kind in (socket.SOCK_STREAM, socket.SOCK_DGRAM) if port == 21116 else (socket.SOCK_STREAM,):
            for host in ("127.0.0.1", "0.0.0.0"):
                sk = socket.socket(socket.AF_INET, kind)
                try:
                    sk.bind((host, port))
                except OSError:
                    busy.append("%s %s:%d" % ("UDP" if kind == socket.SOCK_DGRAM else "TCP", host, port))
                finally:
                    sk.close()
    check("preflight: ports 21114-21119 (+UDP 21116) are free", "all free", ("IN USE: " + ", ".join(busy)) if busy else "all free", not busy)
    if busy:
        return

    tmp = tempfile.mkdtemp(prefix="hbbs-accept2-")
    procs, files, logs = [], [], {}
    try:
        dbfile = os.path.join(tmp, "db_v2.sqlite3")
        env = dict(os.environ, DB_URL=dbfile, API_PORT="21114", RUST_LOG="info")
        for name in ("hbbr", "hbbs"):
            logs[name] = os.path.join(tmp, name + ".log")
            f = open(logs[name], "wb")
            files.append(f)
            procs.append(subprocess.Popen([os.path.join(rel, name + exe)], cwd=tmp, env=env, stdout=f, stderr=subprocess.STDOUT))
            time.sleep(1.5)
        hbbs = procs[1]
        pw = None
        for _ in range(40):
            time.sleep(0.5)
            m = re.search(r"Console initial admin password: (\S+)", open(logs["hbbs"], errors="replace").read())
            if m:
                pw = m.group(1)
                break
        check("hbbs log prints initial admin password", "line found", "found" if pw else "missing", pw)
        if not pw:
            return
        time.sleep(1)

        # --- admin: first login, forced password change, new session (web sessions: no id/uuid) ---
        st, b = http("POST", "/api/login", {"username": "admin", "password": pw, "type": "account", "deviceInfo": {}})
        t0 = b.get("access_token") if isinstance(b, dict) else None
        st, b = http("GET", "/api/users", token=t0)
        check("fresh admin: management route answers 403 Password change required", "403 + that text", "%s %s" % (st, b),
              st == 403 and isinstance(b, dict) and b.get("error") == "Password change required")
        st, b = http("POST", "/api/console/change-password", {"old_password": pw, "new_password": NEW_ADMIN_PW}, token=t0)
        check("admin changes the initial password", "200", "%s %s" % (st, b), st == 200)
        st, b = http("POST", "/api/login", {"username": "admin", "password": NEW_ADMIN_PW, "type": "account", "deviceInfo": {}})
        TOKEN[0] = b.get("access_token") if isinstance(b, dict) else None
        check("admin logs in with the new password", "token", "token" if TOKEN[0] else (st, b), bool(TOKEN[0]))
        if not TOKEN[0]:
            return
        T = TOKEN[0]

        # --- user groups first (rules need their targets to exist) ---
        sc("user-groups.py add Support", "user-groups", ["add", "--name", "Support", "--note", "support team"], ["{}"])
        sc("user-groups.py add Other", "user-groups", ["add", "--name", "Other"], ["{}"])
        sc("user-groups.py add Sales --access-to Support", "user-groups",
           ["add", "--name", "Sales", "--access-to", '[{"type":0,"name":"Support"}]'], ["{}"])
        out = sc("user-groups.py view Sales shows the rule", "user-groups", ["view", "--name", "Sales"], ['"Support"', '"allowed_outgoings"'])
        sc("user-groups.py add with unknown rule target is rejected", "user-groups",
           ["add", "--name", "Bad", "--access-to", '[{"type":0,"name":"NoSuchGroup"}]'], ["Unknown group or user: NoSuchGroup"], expect_rc=1)

        # --- users.py ---
        sc("users.py new newbie --group_name Default", "users",
           ["new", "--name", "newbie", "--password", USER_PW, "--group_name", "Default"], ["Success: User created"])
        sc("users.py view newbie", "users", ["view", "--name", "newbie", "--group_name", "Default"], ["'name': 'newbie'", "'group_name': 'Default'"])
        sc("users.py disable newbie", "users", ["disable", "--name", "newbie"], ["Disable newbie", "Success"])
        st, b = http("GET", "/api/users?name=newbie", token=T)
        check("newbie is disabled", "status 0", b["data"][0]["status"] if st == 200 and b.get("data") else (st, b),
              st == 200 and b.get("data") and b["data"][0]["status"] == 0)
        sc("users.py enable newbie", "users", ["enable", "--name", "newbie"], ["Enable newbie", "Success"])
        st, b = http("GET", "/api/users?name=newbie", token=T)
        check("newbie is enabled again", "status 1", b["data"][0]["status"] if st == 200 and b.get("data") else (st, b),
              st == 200 and b.get("data") and b["data"][0]["status"] == 1)
        sc("users.py disable newbie (before delete)", "users", ["disable", "--name", "newbie"], ["Success"])
        sc("users.py delete newbie", "users", ["delete", "--name", "newbie"], ["Delete newbie", "Success"])
        sc("users.py view newbie after delete", "users", ["view", "--name", "newbie"], ["Found 0 users"])
        for u, g in (("alice", "Sales"), ("bob", "Sales"), ("erin", "Sales"), ("carol", "Support"), ("dave", "Other")):
            sc("users.py new %s in %s" % (u, g), "users", ["new", "--name", u, "--password", USER_PW, "--group_name", g], ["Success: User created"])
        sc("users.py disable erin", "users", ["disable", "--name", "erin"], ["Success"])

        # --- user-groups.py lifecycle on a separate group ---
        sc("user-groups.py add qa-team --access-to Support", "user-groups",
           ["add", "--name", "qa-team", "--access-to", '[{"type":0,"name":"Support"}]'], ["{}"])
        sc("users.py new qauser (Default)", "users", ["new", "--name", "qauser", "--password", USER_PW, "--group_name", "Default"], ["Success"])
        sc("user-groups.py add-users qa-team qauser", "user-groups", ["add-users", "--name", "qa-team", "--users", "qauser"], ["Added 1 user(s)"])
        sc("user-groups.py view-users qa-team", "user-groups", ["view-users", "--name", "qa-team"], ['"name": "qauser"'])
        sc("user-groups.py update qa-team -> qa-team2 + note + access-to Other", "user-groups",
           ["update", "--name", "qa-team", "--new-name", "qa-team2", "--note", "renamed", "--access-to", '[{"type":0,"name":"Other"}]'], ["Success"])
        sc("user-groups.py view qa-team2 reflects the update", "user-groups", ["view", "--name", "qa-team2"],
           ['"qa-team2"', '"renamed"', '"Other"'])
        sc("user-groups.py delete qa-team2 while it has a member is refused", "user-groups", ["delete", "--name", "qa-team2"],
           ["Group is not empty"], expect_rc=1)
        sc("user-groups.py add-users Default qauser (moves the member out)", "user-groups", ["add-users", "--name", "Default", "--users", "qauser"], ["Added 1 user(s)"])
        sc("user-groups.py delete qa-team2", "user-groups", ["delete", "--name", "qa-team2"], ["Success"])
        st, b = http("GET", "/api/user-groups?name=qa-team2", token=T)
        check("qa-team2 is gone and qauser still exists", "0 groups, qauser present",
              "%s groups, users=%s" % (b.get("total"), names(http("GET", "/api/users?name=qauser", token=T)[1])),
              b.get("total") == 0 and names(http("GET", "/api/users?name=qauser", token=T)[1]) == ["qauser"])

        # --- devices: simulated registrations (peer rows are written by hbbs in real life) ---
        db = sqlite3.connect(dbfile, timeout=10)
        uuids = {}

        def add_peer(pid, hostname):
            raw = os.urandom(16)
            db.execute("insert into peer(guid,id,uuid,pk,info) values(?,?,?,?,?)", (os.urandom(16), pid, raw, b"", '{"ip":"127.0.0.1"}'))
            db.commit()
            uuids[pid] = base64.b64encode(raw).decode()
            st, body = http("POST", "/api/sysinfo", {"id": pid, "uuid": uuids[pid], "version": "1.4.0", "hostname": hostname,
                                                     "username": "tester", "os": "Windows / Windows 11 Pro", "cpu": "CPU", "memory": "16.0GB"})
            return body

        D = {k: "30000000%d" % k for k in range(1, 10)}
        bodies = [add_peer(D[k], "host-%d" % k) for k in range(1, 10)]
        check("9 simulated devices registered (sysinfo)", "all SYSINFO_UPDATED", set(bodies), set(bodies) == {"SYSINFO_UPDATED"})

        sc("devices.py assign D1 user_name=alice", "devices", ["assign", "--id", D[1], "--assign_to", "user_name=alice"], ["{}"])
        sc("devices.py assign D2 user_name=carol", "devices", ["assign", "--id", D[2], "--assign_to", "user_name=carol"], ["{}"])
        sc("devices.py assign D3 user_name=dave", "devices", ["assign", "--id", D[3], "--assign_to", "user_name=dave"], ["{}"])
        sc("devices.py assign D6 user_name=carol", "devices", ["assign", "--id", D[6], "--assign_to", "user_name=carol"], ["{}"])
        sc("devices.py disable D6", "devices", ["disable", "--id", D[6]], ["Disable " + D[6]])
        sc("devices.py enable D6", "devices", ["enable", "--id", D[6]], ["Enable " + D[6]])
        sc("devices.py disable D6 again", "devices", ["disable", "--id", D[6]], ["Disable " + D[6]])
        st, b = http("GET", "/api/devices?id=" + D[6], token=T)
        check("D6 is disabled (status 0)", "0", b["data"][0]["status"] if st == 200 and b.get("data") else (st, b),
              st == 200 and b.get("data") and b["data"][0]["status"] == 0)
        sc("devices.py assign D5 device_group_name=Secret (group missing -> error)", "devices",
           ["assign", "--id", D[5], "--assign_to", "device_group_name=Secret"], ["Unknown"], expect_rc=1)

        # --- device groups ---
        sc("device-groups.py add Lab --accessed-from alice", "device-groups",
           ["add", "--name", "Lab", "--accessed-from", '[{"type":2,"name":"alice"}]'], ["{}"])
        sc("device-groups.py add Secret", "device-groups", ["add", "--name", "Secret"], ["{}"])
        sc("device-groups.py add-devices Lab D4", "device-groups", ["add-devices", "--name", "Lab", "--ids", D[4]], ["{}"])
        sc("devices.py assign D5 device_group_name=Secret", "devices", ["assign", "--id", D[5], "--assign_to", "device_group_name=Secret"], ["{}"])
        sc("device-groups.py view-devices Lab", "device-groups", ["view-devices", "--name", "Lab"], [D[4]])
        sc("device-groups.py add dg-temp", "device-groups", ["add", "--name", "dg-temp"], ["{}"])
        sc("device-groups.py add-devices dg-temp D8", "device-groups", ["add-devices", "--name", "dg-temp", "--ids", D[8]], ["{}"])
        sc("device-groups.py view-devices dg-temp", "device-groups", ["view-devices", "--name", "dg-temp"], [D[8]])
        sc("device-groups.py remove-devices dg-temp D8", "device-groups", ["remove-devices", "--name", "dg-temp", "--ids", D[8]], ["{}"])
        rc, out = script("device-groups", "view-devices", "--name", "dg-temp")
        check("dg-temp is empty after remove-devices", "[]", out, rc == 0 and out.strip() == "[]")
        sc("device-groups.py delete dg-temp", "device-groups", ["delete", "--name", "dg-temp"], ["Success"])

        # --- devices.py view ---
        sc("devices.py view --offline_days 0 does not crash", "devices", ["view", "--offline_days", "0"], [D[1]])
        sc("devices.py view --device_group_name Lab", "devices", ["view", "--device_group_name", "Lab"], [D[4]])

        # --- client --assign simulation ---
        st, body = http("POST", "/api/devices/cli", {"id": D[9], "uuid": uuids[D[9]], "note": "cli-note", "device_name": "cli-pc"}, token=T)
        check("POST /api/devices/cli id+uuid -> empty body", "200 + empty", "%s %r" % (st, body), st == 200 and body == "")
        st, b = http("GET", "/api/devices?id=" + D[9], token=T)
        d9 = b["data"][0] if st == 200 and b.get("data") else {}
        check("cli assignment applied", "note=cli-note, device_name=cli-pc",
              "note=%s, device_name=%s" % (d9.get("note"), d9.get("info", {}).get("device_name")),
              d9.get("note") == "cli-note" and d9.get("info", {}).get("device_name") == "cli-pc")
        wrong = base64.b64encode(os.urandom(16)).decode()
        st, body = http("POST", "/api/devices/cli", {"id": D[9], "uuid": wrong, "note": "evil"}, token=T)
        st2, b = http("GET", "/api/devices?id=" + D[9], token=T)
        check("POST /api/devices/cli wrong uuid", "Device not found, note unchanged",
              "%s / note=%s" % (body, b["data"][0].get("note")), body == "Device not found" and b["data"][0].get("note") == "cli-note")

        # --- client "Group" tab as non-admin users ---
        def login(u):
            st, b = http("POST", "/api/login", {"username": u, "password": USER_PW, "type": "account", "deviceInfo": {}})
            return b.get("access_token") if isinstance(b, dict) else None

        alice, carol = login("alice"), login("carol")
        check("non-admin users can log in", "tokens", bool(alice and carol), bool(alice and carol))
        q = "current=1&pageSize=100&accessible=&status=1"
        st, b = http("GET", "/api/users?" + q, token=alice)
        check("alice: /api/users?accessible", "alice, bob, carol (own group + Support via rule; erin disabled, dave hidden)",
              "%s %s" % (st, sorted(names(b))), st == 200 and sorted(names(b)) == ["alice", "bob", "carol"] and b.get("total") == 3)
        st, b = http("GET", "/api/peers?" + q, token=alice)
        ids = sorted(d["id"] for d in b.get("data", [])) if isinstance(b, dict) else b
        check("alice: /api/peers?accessible", "D1, D2, D4 (D3 dave, D5 Secret, D6 disabled hidden)",
              "%s %s" % (st, ids), st == 200 and ids == sorted([D[1], D[2], D[4]]) and b.get("total") == 3)
        st, b = http("GET", "/api/device-group/accessible?current=1&pageSize=100", token=alice)
        check("alice: /api/device-group/accessible", "Lab only", "%s %s" % (st, names(b)), st == 200 and names(b) == ["Lab"])
        st, b = http("GET", "/api/users", token=alice)
        check("alice: /api/users without accessible", '403 {"error":"Admin required!"}', "%s %s" % (st, b),
              st == 403 and b == {"error": "Admin required!"})
        st, b = http("GET", "/api/devices", token=alice)
        own = sorted(d["id"] for d in b.get("data", [])) if isinstance(b, dict) else b
        check("alice: /api/devices lists only her own device", "D1 only", "%s %s" % (st, own), st == 200 and own == [D[1]])
        st, b = http("GET", "/api/user-groups", token=alice)
        check("alice: /api/user-groups (admin route)", "403", st, st == 403)
        st, b = http("GET", "/api/users?" + q, token=carol)
        check("carol: /api/users?accessible", "carol only", "%s %s" % (st, names(b)), st == 200 and names(b) == ["carol"])
        st, b = http("GET", "/api/peers?" + q, token=carol)
        ids = sorted(d["id"] for d in b.get("data", [])) if isinstance(b, dict) else b
        check("carol: /api/peers?accessible", "D2 only (D6 disabled)", "%s %s" % (st, ids), st == 200 and ids == [D[2]])
        st, b = http("GET", "/api/device-group/accessible?current=1&pageSize=100", token=carol)
        check("carol: /api/device-group/accessible", "none", "%s %s" % (st, names(b)), st == 200 and names(b) == [])

        # --- deleting a disabled device while hbbs runs ---
        sc("devices.py disable D7", "devices", ["disable", "--id", D[7]], ["Disable " + D[7]])
        sc("devices.py delete D7", "devices", ["delete", "--id", D[7]], ["Delete " + D[7]])
        st, body = http("POST", "/api/sysinfo", {"id": D[7], "uuid": uuids[D[7]], "version": "1.4.0", "hostname": "host-7"})
        left = db.execute("select count(*) from peer where id=?", (D[7],)).fetchone()[0]
        check("sysinfo from the deleted device", "ID_NOT_FOUND, peer row gone", "%s, rows=%d" % (body, left), body == "ID_NOT_FOUND" and left == 0)
        sc("devices.py view D7 after delete", "devices", ["view", "--id", D[7]], [])
        db.close()
        check("hbbs still running", "alive", "alive" if hbbs.poll() is None else "exit %s" % hbbs.poll(), hbbs.poll() is None)

        # --- UI smoke ---
        st, html, h = http("GET", "/", raw=True)
        check("GET / serves web UI", 'id="root"', "%s, root=%s" % (st, 'id="root"' in html), st == 200 and 'id="root"' in html)
        for route in ("/users", "/devices", "/device-groups"):
            st, h2, hd = http("GET", route, raw=True)
            check("GET %s falls back to index.html" % route, "200 + same index", "%s, same=%s" % (st, h2 == html), st == 200 and h2 == html)
        m = re.search(r'src="(/assets/[^"]+\.js)"', html)
        if m:
            st, js, hd = http("GET", m.group(1), raw=True)
            check("GET " + m.group(1), "200 + javascript", "%s %s" % (st, hd.get("Content-Type", "")), st == 200 and "javascript" in hd.get("Content-Type", ""))
        else:
            check("asset referenced in index.html", "found", "none", False)

        logtxt = "".join(open(logs[n], errors="replace").read() for n in logs)
        check("no panics in hbbs/hbbr logs", "no 'panicked'", "clean" if "panicked" not in logtxt else "PANIC", "panicked" not in logtxt)
    finally:
        for p in procs:
            p.terminate()
        for p in procs:
            try:
                p.wait(timeout=10)
            except Exception:
                p.kill()
        for f in files:
            f.close()
        for _ in range(10):
            shutil.rmtree(tmp, ignore_errors=True)
            if not os.path.exists(tmp):
                break
            time.sleep(1)
        check("processes stopped, temp dir removed", "gone", "removed" if not os.path.exists(tmp) else "STILL THERE", not os.path.exists(tmp))


def main():
    run()
    return p1.finish()


if __name__ == "__main__":
    sys.exit(main())
