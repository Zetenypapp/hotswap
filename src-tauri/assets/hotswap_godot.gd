extends Node
# HotSwap: applies the mods you turn on in HotSwap. It asks HotSwap (on this PC only) twice a
# second, and puts everything back by itself when HotSwap closes. Safe to delete.

var udp = PacketPeerUDP.new()
var last_ask = 0
var last_answer = 0
var orig = {}
var on = {}
var done = {}
var run = null
var last_mods = {}

func _ready():
	process_mode = Node.PROCESS_MODE_ALWAYS
	udp.connect_to_host("127.0.0.1", 47831)

func _process(_delta):
	# Real time: the game's own clock may be slowed down by a mod.
	var now = Time.get_ticks_msec()
	if now - last_ask > 500:
		last_ask = now
		udp.put_packet("hs".to_utf8_buffer())
	while udp.get_available_packet_count() > 0:
		var msg = JSON.parse_string(udp.get_packet().get_string_from_utf8())
		if msg is Dictionary:
			last_answer = now
			_tick(msg)
	if last_answer > 0 and now - last_answer > 5000:
		last_answer = 0
		_tick({"run": run, "mods": [], "actions": []})

func _root(name):
	if name == "Engine":
		return Engine
	var tree = get_tree().root
	if tree.has_node(name):
		return tree.get_node(name)
	return tree.find_child(name, true, false)

# Every [object, property] a path points to: "Node.prop", "/root/Path/To/Node.prop",
# "Engine.time_scale"; "list[]" goes through every element of an array.
func _targets(at):
	var dot = at.find(".", at.rfind("/") + 1)
	if dot < 0:
		return []
	var objs = [_root(at.substr(0, dot))]
	var parts = at.substr(dot + 1).split(".")
	for i in range(parts.size() - 1):
		var next = []
		for o in objs:
			if o == null:
				continue
			var p = parts[i]
			if p.ends_with("[]"):
				var arr = o.get(p.substr(0, p.length() - 2))
				if arr is Array:
					for x in arr:
						next.append(x)
			else:
				next.append(o.get(p))
		objs = next
	var out = []
	for o in objs:
		if o != null:
			out.append([o, parts[parts.size() - 1]])
	return out

func _read(t):
	var v = t[0].get(t[1])
	if v is bool:
		return 1.0 if v else 0.0
	if v is int or v is float:
		return float(v)
	return null

func _write(t, v):
	var cur = t[0].get(t[1])
	if cur is bool:
		v = v != 0
	elif cur is int:
		v = int(v)
	if cur != v:
		t[0].set(t[1], v)

func _code(code):
	var e = Expression.new()
	if e.parse(code) == OK:
		e.execute([], self)

func _apply(id, specs, value):
	for k in range(specs.size()):
		var s = specs[k]
		var slider = s.op == "scale" or s.op == "value"
		var active = value != s.def if slider else value != 0
		var key = str(id) + "#" + str(k)
		if s.op == "js":
			if active and not on.get(key, false):
				_code(s.at)
			if not active and on.get(key, false) and s.from != null:
				_code(s.from)
			on[key] = active
			continue
		var ts = _targets(s.at)
		var sib = []
		if s.op == "fill" and s.from != null:
			sib = _targets(s.at.substr(0, s.at.rfind(".") + 1) + s.from)
		for i in range(ts.size()):
			var tk = key + "#" + str(i)
			if not active:
				if orig.has(tk):
					if s.op in ["set", "scale", "value"]:
						_write(ts[i], orig[tk])
					orig.erase(tk)
				continue
			var cur = _read(ts[i])
			if cur == null:
				continue
			if not orig.has(tk):
				orig[tk] = cur
			var want = null
			match s.op:
				"set": want = s.v
				"min": want = max(cur, s.v)
				"max": want = min(cur, s.v)
				"freeze": want = orig[tk]
				"scale": want = orig[tk] * value / 100.0
				"value": want = value * s.mul
				"fill":
					if i < sib.size():
						want = _read(sib[i])
			if want != null:
				_write(ts[i], want)

func _tick(msg):
	if msg.run != run:
		run = msg.run
		done = {}
	var now_mods = {}
	for m in msg.mods:
		now_mods[m.id] = m.specs
		_apply(m.id, m.specs, m.value)
	# Mods no longer sent (HotSwap closed): off, which puts back what they changed.
	for id in last_mods.keys():
		if not now_mods.has(id):
			var specs = last_mods[id]
			_apply(id, specs, specs[0].def if specs.size() > 0 else 0)
	last_mods = now_mods
	for a in msg.actions:
		var n = int(a.n)
		var before = done.get(a.id, 0)
		done[a.id] = n
		for _i in range(n - before):
			for s in a.specs:
				if s.op == "js":
					_code(s.at)
					continue
				for t in _targets(s.at):
					var cur = _read(t)
					if cur != null:
						_write(t, cur + s.v if s.op == "add" else s.v)
