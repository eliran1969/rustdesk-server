#!/usr/bin/env python3
"""Phase 1 acceptance check for the hbbs web console (stdlib only).

Run:  python -I docs/webconsole/phase1-acceptance-check.py [--skip-build] [--devices-py PATH]
It builds the release binaries, starts hbbr+hbbs in a fresh temp dir, simulates a
RustDesk client over HTTP exactly as the client code does, prints PASS/FAIL per check,
stops everything and deletes the temp dir. Exit code 0 only if every check passed.
"""
import argparse, base64, json, os, re, shutil, sqlite3, subprocess, sys, tempfile, time
import urllib.error, urllib.request

REPO = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
BASE = "http://127.0.0.1:21114"
RESULTS = []


def check(name, expected, actual, ok):
    RESULTS.append((name, expected, str(actual), bool(ok)))
    print(("PASS " if ok else "FAIL ") + name + " | expected: " + expected + " | actual: " + str(actual)[:300], flush=True)


def http(method, path, body=None, token=None, raw=False):
    data = None if body is None else json.dumps(body).encode()
    req = urllib.request.Request(BASE + path, data=data, method=method)
    if body is not None:
        req.add_header("Content-Type", "application/json")
    if token:
        req.add_header("Authorization", "Bearer " + token)
    try:
        with urllib.request.urlopen(req, timeout=10) as r:
            status, text, hdrs = r.status, r.read().decode(), r.headers
    except urllib.error.HTTPError as e:
        status, text, hdrs = e.code, e.read().decode(), e.headers
    if raw:
        return status, text, hdrs
    try:
        return status, json.loads(text)
    except ValueError:
        return status, text


def netstat(pids):
    out = subprocess.run(["netstat", "-ano"], capture_output=True, text=True).stdout
    found = set()
    for line in out.splitlines():
        p = line.split()
        if len(p) >= 4 and p[-1] in pids:
            if p[0] == "TCP" and p[3] != "LISTENING":
                continue
            found.add((p[0], p[1].rsplit(":", 1)[-1], int(p[-1])))
    return found


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--skip-build", action="store_true")
    ap.add_argument("--devices-py", default=r"E:\RustDesk\res\devices.py")
    args = ap.parse_args()
    exe = ".exe" if os.name == "nt" else ""
    rel = os.path.join(REPO, "target", "release")

    if not args.skip_build:
        if not os.path.isfile(os.path.join(REPO, "console-ui", "dist", "index.html")):
            subprocess.run("npm ci && npm run build", shell=True, cwd=os.path.join(REPO, "console-ui"), check=True)
        r = subprocess.run(["cargo", "build", "--release", "--bin", "hbbs", "--bin", "hbbr", "--bin", "rustdesk-utils"], cwd=REPO)
        check("cargo build --release (hbbs, hbbr, rustdesk-utils)", "exit 0", r.returncode, r.returncode == 0)
        if r.returncode:
            return finish()

    tmp = tempfile.mkdtemp(prefix="hbbs-accept-")
    procs = []
    files = []
    try:
        env = dict(os.environ, DB_URL=os.path.join(tmp, "db_v2.sqlite3"), API_PORT="21114", RUST_LOG="info")
        logs = {}
        for name in ("hbbr", "hbbs"):
            logs[name] = os.path.join(tmp, name + ".log")
            f = open(logs[name], "wb")
            files.append(f)
            procs.append(subprocess.Popen([os.path.join(rel, name + exe)], cwd=tmp, env=env, stdout=f, stderr=subprocess.STDOUT))
            time.sleep(1.5)
        hbbr, hbbs = procs
        pw = None
        for _ in range(40):
            time.sleep(0.5)
            m = re.search(r"Console initial admin password: (\S+)", open(logs["hbbs"], errors="replace").read())
            if m:
                pw = m.group(1)
                break
        check("hbbs log prints initial admin password", "line found", "found" if pw else "missing", pw)
        if not pw:
            return finish()
        time.sleep(1)

        # simulated client registration (peer row) + client HTTP calls
        uuid_raw = os.urandom(16)
        uuid_b64 = base64.b64encode(uuid_raw).decode()
        db = sqlite3.connect(os.path.join(tmp, "db_v2.sqlite3"), timeout=10)
        db.execute("insert into peer(guid,id,uuid,pk,info) values(?,?,?,?,?)",
                   (os.urandom(16), "123456789", uuid_raw, b"", '{"ip":"127.0.0.1"}'))
        db.commit()
        sysinfo = {"id": "123456789", "uuid": uuid_b64, "version": "1.4.0", "hostname": "accept-test-pc",
                   "username": "tester", "os": "Windows / Windows 11 Pro", "cpu": "Test CPU, 8 cores", "memory": "16.0GB"}
        st, body = http("POST", "/api/sysinfo", sysinfo)
        check("POST /api/sysinfo (valid)", "SYSINFO_UPDATED", body, body == "SYSINFO_UPDATED")
        st, body = http("POST", "/api/heartbeat", {"id": "123456789", "uuid": uuid_b64, "ver": 1004000, "modified_at": 0})
        check("POST /api/heartbeat after sysinfo", "{}", body, body == {})
        bad = dict(sysinfo, uuid=base64.b64encode(os.urandom(16)).decode(), hostname="evil-host")
        st, body = http("POST", "/api/sysinfo", bad)
        host = json.loads(db.execute("select sysinfo from console_peer_sysinfo").fetchone()[0]).get("hostname")
        check("POST /api/sysinfo wrong uuid", "ID_NOT_FOUND, hostname unchanged",
              "%s / hostname=%s" % (body, host), body == "ID_NOT_FOUND" and host == "accept-test-pc")
        db.close()

        login_body = {"username": "admin", "password": pw, "id": "123456789", "uuid": uuid_b64, "type": "account", "deviceInfo": {}}
        st, body = http("POST", "/api/login", login_body)
        tok = body.get("access_token") if isinstance(body, dict) else None
        check("POST /api/login admin", "200 + access_token", "%s %s" % (st, "token" if tok else body), st == 200 and tok)
        st, body = http("GET", "/api/devices", token=tok)
        dev = (body.get("data") or [{}])[0] if isinstance(body, dict) else {}
        check("GET /api/devices shows device + assigned user",
              "device_name=accept-test-pc, user_name=admin",
              json.dumps({"device_name": dev.get("info", {}).get("device_name"), "user_name": dev.get("user_name"), "id": dev.get("id")}),
              dev.get("info", {}).get("device_name") == "accept-test-pc" and dev.get("user_name") == "admin")
        st, body = http("POST", "/api/currentUser", {"id": "123456789", "uuid": uuid_b64}, token=tok)
        check("POST /api/currentUser", "name=admin", "%s %s" % (st, body.get("name") if isinstance(body, dict) else body),
              st == 200 and isinstance(body, dict) and body.get("name") == "admin")

        # second session for devices.py so the logout below cannot affect it
        st, b2 = http("POST", "/api/login", login_body)
        tok2 = b2.get("access_token")
        if os.path.isfile(args.devices_py):
            r = subprocess.run([sys.executable, "-I", args.devices_py, "--url", BASE, "--token", tok2, "view"],
                               capture_output=True, text=True, timeout=60)
            check("res/devices.py view", "prints the device", (r.stdout + r.stderr).strip()[:300],
                  r.returncode == 0 and "accept-test-pc" in r.stdout)
        else:
            check("res/devices.py view", "script present", "missing: " + args.devices_py, False)

        st, body = http("POST", "/api/logout", {"id": "123456789", "uuid": uuid_b64}, token=tok)
        check("POST /api/logout", "{}", body, body == {})
        st, body = http("POST", "/api/currentUser", {"id": "123456789", "uuid": uuid_b64}, token=tok)
        check("currentUser after logout", "401", st, st == 401)
        st, body = http("GET", "/api/devices", token=tok2)
        dev = (body.get("data") or [{}])[0] if isinstance(body, dict) else {}
        check("device stays assigned after logout", "user_name=admin", dev.get("user_name"), dev.get("user_name") == "admin")

        st, me = http("GET", "/api/console/me", token=tok2)
        check("/api/console/me fresh admin", "must_change_password=true", me.get("must_change_password") if isinstance(me, dict) else me,
              isinstance(me, dict) and me.get("must_change_password") is True)
        newpw = "Accept-Test-New-Pw-1"
        st, body = http("POST", "/api/console/change-password", {"old_password": pw, "new_password": newpw}, token=tok2)
        check("POST /api/console/change-password", "200", "%s %s" % (st, body), st == 200)
        st, tk3 = http("POST", "/api/login", dict(login_body, password=newpw))
        tok3 = tk3.get("access_token") if isinstance(tk3, dict) else None
        st, me = http("GET", "/api/console/me", token=tok3)
        check("must_change_password after change", "false (login with new password works)",
              me.get("must_change_password") if isinstance(me, dict) else (st, me),
              isinstance(me, dict) and me.get("must_change_password") is False)

        st, html, h = http("GET", "/", raw=True)
        check("GET / serves web UI", 'id="root"', "%s, root=%s" % (st, 'id="root"' in html), st == 200 and 'id="root"' in html)
        m = re.search(r'src="(/assets/[^"]+\.js)"', html)
        if m:
            st, js, h = http("GET", m.group(1), raw=True)
            ct = h.get("Content-Type", "")
            check("GET " + m.group(1), "200 + javascript", "%s %s" % (st, ct), st == 200 and "javascript" in ct)
        else:
            check("asset referenced in index.html", "found", "none", False)

        ports = netstat({str(hbbs.pid), str(hbbr.pid)})
        want = {("TCP", "21114", hbbs.pid), ("TCP", "21115", hbbs.pid), ("TCP", "21116", hbbs.pid), ("UDP", "21116", hbbs.pid),
                ("TCP", "21118", hbbs.pid), ("TCP", "21117", hbbr.pid), ("TCP", "21119", hbbr.pid)}
        missing = sorted(want - ports)
        check("hbbs/hbbr listening ports", "hbbs 21114/21115/21116 tcp+udp/21118, hbbr 21117/21119",
              ("missing: %s" % missing) if missing else "all present", not missing)
        try:
            r = subprocess.run([os.path.join(rel, "rustdesk-utils" + exe), "doctor", "127.0.0.1"], capture_output=True,
                               text=True, timeout=60, cwd=tmp, env=env)
            out = r.stdout + r.stderr
            check("rustdesk-utils doctor 127.0.0.1", "runs without error", out.strip()[-300:], r.returncode == 0)
        except Exception as e:
            check("rustdesk-utils doctor 127.0.0.1", "runs", repr(e), False)

        # rate limit last: it blocks this IP for a minute
        codes = [http("POST", "/api/login", dict(login_body, password="wrong-%d" % i))[0] for i in range(10)]
        st, _ = http("POST", "/api/login", dict(login_body, password=newpw))
        check("10 wrong logins, then the 11th (correct) is blocked", "10x 401, then 429",
              "%s then %s" % (sorted(set(codes)), st), set(codes) == {401} and st == 429)

        logtxt = "".join(open(logs[n], errors="replace").read() for n in logs)
        check("no panics in hbbs/hbbr logs", "no 'panicked'", "clean" if "panicked" not in logtxt else "PANIC",
              "panicked" not in logtxt)
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
        check("processes stopped, temp dir removed", "gone", "removed" if not os.path.exists(tmp) else "STILL THERE",
              not os.path.exists(tmp))
    return finish()


def finish():
    ok = sum(1 for r in RESULTS if r[3])
    print("\n%d/%d checks passed" % (ok, len(RESULTS)))
    return 0 if ok == len(RESULTS) else 1


if __name__ == "__main__":
    sys.exit(main())
