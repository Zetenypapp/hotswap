# HotSwap: applies the mods you turn on in HotSwap. It asks HotSwap (on this PC only) twice a
# second, and puts everything back by itself when HotSwap closes. Safe to delete.

init 999 python:
    import socket as _hs_socket
    import json as _hs_json
    import time as _hs_time

    class _HotSwap(object):
        def __init__(self):
            self.sock = None
            self.last_ask = 0
            self.last_answer = 0
            self.orig = {}
            self.on = {}
            self.done = {}
            self.run = None
            self.last_mods = {}

        def ns(self):
            return renpy.store.__dict__

        # Every (getter, setter) a path points to: a store expression such as "money",
        # "persistent.gold" or "config.console"; "list[]" goes through every element.
        def targets(self, at):
            parts = at.split("[]")
            if len(parts) == 1:
                def get(at=at):
                    return eval(at, self.ns())
                def put(v, at=at):
                    self.ns()["_hs_v"] = v
                    exec(at + " = _hs_v", self.ns())
                return [(get, put)]
            objs = list(eval(parts[0], self.ns()) or [])
            for p in parts[1:-1]:
                objs = [x for o in objs for x in (self.walk(o, p) or [])]
            last = [k for k in parts[-1].split(".") if k]
            if not last:
                return []
            key = last[-1]
            out = []
            for o in objs:
                o = self.walk(o, ".".join(last[:-1]))
                if o is None:
                    continue
                if isinstance(o, dict):
                    out.append((lambda o=o: o.get(key), lambda v, o=o: o.__setitem__(key, v)))
                else:
                    out.append((lambda o=o: getattr(o, key, None), lambda v, o=o: setattr(o, key, v)))
            return out

        def walk(self, o, path):
            for k in [k for k in path.split(".") if k]:
                if o is None:
                    return None
                o = o.get(k) if isinstance(o, dict) else getattr(o, k, None)
            return o

        def read(self, t):
            v = t[0]()
            if isinstance(v, bool):
                return 1.0 if v else 0.0
            if isinstance(v, (int, float)):
                return float(v)
            try:
                if isinstance(v, long):
                    return float(v)
            except NameError:
                pass
            return None

        def write(self, t, v):
            cur = t[0]()
            if isinstance(cur, bool):
                v = bool(v)
            elif isinstance(cur, int):
                v = int(v)
            if cur != v:
                t[1](v)
                return True
            return False

        def apply(self, mid, specs, value):
            changed = False
            for k, s in enumerate(specs):
                slider = s["op"] in ("scale", "value")
                active = value != s["def"] if slider else value != 0
                key = "%s#%d" % (mid, k)
                if s["op"] == "js":
                    if active and not self.on.get(key):
                        exec(s["at"], self.ns())
                        changed = True
                    if not active and self.on.get(key) and s.get("from"):
                        exec(s["from"], self.ns())
                        changed = True
                    self.on[key] = active
                    continue
                try:
                    ts = self.targets(s["at"])
                    sib = self.targets(s["at"].rsplit(".", 1)[0] + "." + s["from"]) if s["op"] == "fill" and s.get("from") and "." in s["at"] else []
                except Exception:
                    continue
                for i, t in enumerate(ts):
                    tk = "%s#%d" % (key, i)
                    if not active:
                        if tk in self.orig:
                            if s["op"] in ("set", "scale", "value"):
                                changed = self.write(t, self.orig[tk]) or changed
                            del self.orig[tk]
                        continue
                    cur = self.read(t)
                    if cur is None:
                        continue
                    self.orig.setdefault(tk, cur)
                    o = self.orig[tk]
                    want = {"set": s["v"], "min": max(cur, s["v"]), "max": min(cur, s["v"]), "freeze": o,
                            "scale": o * value / 100.0, "value": value * s["mul"]}.get(s["op"])
                    if s["op"] == "fill":
                        want = self.read(sib[i]) if i < len(sib) else None
                    if want is not None:
                        changed = self.write(t, want) or changed
            return changed

        def tick(self, msg):
            if msg.get("run") != self.run:
                self.run = msg.get("run")
                self.done = {}
            changed = False
            now = {}
            for m in msg.get("mods", []):
                now[m["id"]] = m["specs"]
                changed = self.apply(m["id"], m["specs"], m["value"]) or changed
            # Mods no longer sent (HotSwap closed): off, which puts back what they changed.
            for mid, specs in self.last_mods.items():
                if mid not in now:
                    changed = self.apply(mid, specs, specs[0]["def"] if specs else 0) or changed
            self.last_mods = now
            for a in msg.get("actions", []):
                n = int(a["n"])
                before = self.done.get(a["id"], 0)
                self.done[a["id"]] = n
                for _ in range(n - before):
                    for s in a["specs"]:
                        try:
                            if s["op"] == "js":
                                exec(s["at"], self.ns())
                            else:
                                for t in self.targets(s["at"]):
                                    cur = self.read(t)
                                    if cur is not None:
                                        self.write(t, cur + s["v"] if s["op"] == "add" else s["v"])
                            changed = True
                        except Exception:
                            pass
            if changed:
                renpy.restart_interaction()

        def periodic(self):
            try:
                if self.sock is None:
                    self.sock = _hs_socket.socket(_hs_socket.AF_INET, _hs_socket.SOCK_DGRAM)
                    self.sock.setblocking(False)
                now = _hs_time.time()
                if now - self.last_ask > 0.5:
                    self.last_ask = now
                    self.sock.sendto(b"hs", ("127.0.0.1", 47831))
                while True:
                    try:
                        data = self.sock.recv(65536)
                    except Exception:
                        break
                    self.last_answer = now
                    self.tick(_hs_json.loads(data.decode("utf-8")))
                if self.last_answer and now - self.last_answer > 5:
                    self.last_answer = 0
                    self.tick({"run": self.run, "mods": [], "actions": []})
            except Exception:
                pass

    _hotswap = _HotSwap()
    config.periodic_callbacks.append(_hotswap.periodic)
