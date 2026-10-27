-- Hotswap bridge for Scrap Mechanic survival.
--
-- Hotswap adds one line to the end of a few survival scripts while the game runs (and removes it when
-- the game closes). That line loads this file, which wraps a few of the game's own functions. Mods are
-- read from small JSON files Hotswap writes to $SURVIVAL_DATA/hotswap, and use the game's own switches
-- (god mode, unlimited inventory, aggro, ...). Every hook runs inside pcall, so nothing here can break
-- the game: if something fails, the game's own code still runs as normal.

Hotswap = Hotswap or {}
local H = Hotswap
local DIR = "$SURVIVAL_DATA/hotswap/"
local clock = ( type( os ) == "table" and os.clock ) or function() return 0 end

H.seq = H.seq or 0 -- newest settings file read (in_<seq>.json)
H.s = H.s or { mods = {}, actions = {} }

local function log( msg )
	print( "[Hotswap] " .. tostring( msg ) )
end

local function try( f, ... )
	local ok, err = pcall( f, ... )
	if not ok and not H.reported then
		H.reported = true -- once per script environment, so the log doesn't fill up
		log( "error: " .. tostring( err ) )
	end
end

-- Settings: Hotswap writes a new numbered file on every change, so nothing depends on how the game
-- caches files it has opened before.
local function poll()
	local tick = sm.game.getCurrentTick()
	if H.lastPoll and tick - H.lastPoll < 10 and tick >= H.lastPoll then return end
	H.lastPoll = tick
	local n = H.seq
	while sm.json.fileExists( DIR .. "in_" .. ( n + 1 ) .. ".json" ) do
		n = n + 1
	end
	if n == H.seq then return end
	local data = sm.json.open( DIR .. "in_" .. n .. ".json" )
	if type( data ) ~= "table" then return end
	H.seq = n
	H.s = data
	H.s.mods = H.s.mods or {}
	H.s.actions = H.s.actions or {}
	-- Actions already done before this world loaded aren't repeated.
	if not H.done then
		H.done = {}
		for k, v in pairs( H.s.actions ) do H.done[k] = v end
		H.tpDone = H.s.tp and H.s.tp.n or 0
	end
end

local function val( key, default )
	local v = H.s.mods[key]
	if type( v ) ~= "number" then return default end
	return v
end
local function on( key )
	return val( key, 0 ) ~= 0
end

-- How many times an action was pressed since we last looked.
local function pressed( key )
	H.done = H.done or {}
	local want = H.s.actions[key] or 0
	local have = H.done[key] or 0
	if want > have then
		H.done[key] = want
		return want - have
	end
	H.done[key] = math.max( have, want )
	return 0
end

local function hostPlayer()
	for _, p in ipairs( sm.player.getAllPlayers() ) do
		if p.id == 1 then return p end
	end
	return sm.player.getAllPlayers()[1]
end

local function wrap( class, name, before, after )
	if type( class ) ~= "table" then return end
	H.wrapped = H.wrapped or {}
	H.wrapped[class] = H.wrapped[class] or {}
	if H.wrapped[class][name] then return end
	local orig = class[name]
	if type( orig ) ~= "function" then return end
	H.wrapped[class][name] = true
	class[name] = function( self, ... )
		if before then try( before, self, ... ) end
		local a, b, c = orig( self, ... )
		if after then try( after, self, ... ) end
		return a, b, c
	end
end

-- `v` while `cond`, otherwise nil (Lua's `cond and false or nil` would always give nil).
local function when( cond, v )
	if cond then return v end
	return nil
end

-- A game switch we change while a mod is on, and put back when it's turned off.
local function hold( key, wanted, get, set )
	H.held = H.held or {}
	if wanted ~= nil then
		if H.held[key] == nil then H.held[key] = { get() } end
		if get() ~= wanted then set( wanted ) end
	elseif H.held[key] ~= nil then
		set( H.held[key][1] )
		H.held[key] = nil
	end
end

local ITEMS = {
	["give-50-components"] = { "5530e6a0-4748-4926-b134-50ca9ecb9dcf", 50 },
	["give-100-potato-ammo"] = { "bfcfac34-db0f-42d6-bd0c-74a7a5c95e82", 100 },
	["give-20-batteries"] = { "910a7f2c-52b0-46eb-8873-ad13255539af", 20 },
	["give-20-gasoline"] = { "d4d68946-aa03-4b8f-b1af-96b81ad4e305", 20 },
	["give-20-glowsticks"] = { "3a3280e4-03b6-4a4d-9e02-e348478213c9", 20 },
	["give-50-water"] = { "869d4736-289a-4952-96cd-8a40117a2d28", 50 },
}

------------------------------------------------------------------------------------------------
-- The game (server): world switches, items, teleport, and status for Hotswap.
------------------------------------------------------------------------------------------------

local function gameTick( self, timeStep )
	poll()
	-- God mode is the game's own: no damage of any kind.
	if on( "infinite-health" ) then
		g_godMode = true
		H.god = true
	elseif H.god then
		g_godMode = false
		H.god = nil
	end
	hold( "inventory", when( on( "unlimited-items" ), false ), sm.game.getLimitedInventory, sm.game.setLimitedInventory )
	hold( "aggro", when( on( "no-aggro" ), false ), sm.game.getEnableAggro, sm.game.setEnableAggro )
	if on( "no-raids" ) then
		g_disableRaids = true
		H.raids = true
	elseif H.raids then
		g_disableRaids = false
		H.raids = nil
	end
	if type( sm.physics.setGravity ) == "function" then
		H.gravity = H.gravity or sm.physics.getGravity()
		local g = H.gravity * val( "gravity", 100 ) / 100
		if math.abs( sm.physics.getGravity() - g ) > 0.001 then sm.physics.setGravity( g ) end
	end

	-- Time of day: always noon while on (time stops), back to normal after.
	local t = self.sv and self.sv.time
	if t then
		if on( "always-day" ) then
			if t.timeProgress or math.abs( ( t.timeOfDay % 1 ) - 0.5 ) > 0.01 then
				t.timeOfDay = math.floor( t.timeOfDay ) + 0.5
				t.timeProgress = false
				H.day = true
				if self.sv.syncTimer then self.sv.syncTimer.count = self.sv.syncTimer.ticks end
			end
		elseif H.day then
			t.timeProgress = true
			H.day = nil
			if self.sv.syncTimer then self.sv.syncTimer.count = self.sv.syncTimer.ticks end
		end
	end

	local player = hostPlayer()
	if not player then return end
	-- Items
	for key, item in pairs( ITEMS ) do
		local n = pressed( key )
		if n > 0 then
			sm.container.beginTransaction()
			sm.container.collect( player:getInventory(), sm.uuid.new( item[1] ), item[2] * n, false )
			sm.container.endTransaction()
		end
	end
	-- Teleport (from the Hotswap map): onto the ground at x, y.
	local char = player:getCharacter()
	local tp = H.s.tp
	if tp and char and sm.exists( char ) and ( tp.n or 0 ) > ( H.tpDone or 0 ) then
		H.tpDone = tp.n
		local z = tp.z or 200
		local hit, res = sm.physics.raycast( sm.vec3.new( tp.x, tp.y, 1000 ), sm.vec3.new( tp.x, tp.y, -100 ) )
		if hit and res and res.pointWorld then z = res.pointWorld.z end
		local pos = sm.vec3.new( tp.x, tp.y, z + 1.5 )
		local ok = pcall( function() char:setWorldPosition( pos ) end )
		if not ok then
			local dir = char.direction
			local yaw = math.atan2( dir.y, dir.x ) - math.pi / 2
			local newChar = sm.character.createCharacter( player, char:getWorld(), pos, yaw, 0 )
			player:setCharacter( newChar )
		end
		log( string.format( "teleported to %.0f, %.0f, %.0f", tp.x, tp.y, z ) )
	end
	-- Status for Hotswap: where you are, about once a second.
	local tick = sm.game.getCurrentTick()
	if not H.lastOut or tick - H.lastOut >= 40 or tick < H.lastOut then
		H.lastOut = tick
		local out = { tick = tick, seq = H.seq }
		if char and sm.exists( char ) then
			local p, d = char.worldPosition, char.direction
			out.x, out.y, out.z = p.x, p.y, p.z
			out.dx, out.dy = d.x, d.y
		end
		out.hp, out.maxhp = H.hp, H.maxhp
		sm.json.save( out, DIR .. "out.json" )
	end
end

------------------------------------------------------------------------------------------------
-- The player (server): health and breath.
------------------------------------------------------------------------------------------------

local function playerTick( self, dt )
	poll()
	local saved = self.sv and self.sv.saved
	local stats = saved and saved.stats
	if not stats then return end
	if self.player and self.player.id == 1 then
		H.hp, H.maxhp = stats.hp, stats.maxhp
	end
	local dirty = false
	if on( "infinite-health" ) and saved.isConscious and stats.hp < stats.maxhp then
		stats.hp = stats.maxhp
		dirty = true
	end
	if on( "infinite-breath" ) and stats.breath < stats.maxbreath then
		stats.breath = stats.maxbreath
		dirty = true
	end
	-- The game only sends stats now and then; send ours when they change, at most twice a second.
	if dirty then
		local tick = sm.game.getCurrentTick()
		if not self.__hsSent or tick - self.__hsSent >= 20 or tick < self.__hsSent then
			self.__hsSent = tick
			self.network:setClientData( saved )
		end
	end
end

------------------------------------------------------------------------------------------------
-- Your character (client): run speed and jump height.
------------------------------------------------------------------------------------------------

local function isMine( self )
	local c = self.character
	return c and sm.exists( c ) and c:getPlayer() == sm.localPlayer.getPlayer()
end

local function characterUpdate( self, dt )
	if not isMine( self ) then return end
	poll()
	local speed = val( "run-speed", 100 ) / 100
	if speed ~= 1 then
		self.character.movementSpeedFraction = self.character.movementSpeedFraction * speed
	end
	local jump = val( "jump-height", 100 ) / 100
	if jump ~= 1 or self.__hsJump then
		local base = self.__hsJumpBase or 1.0
		self.character:setJumpSpeedFraction( base * jump )
		self.__hsJump = jump ~= 1 or nil
	end
end

local function characterData( self, data, channel )
	if channel == 2 and data then
		self.__hsJumpBase = data.jumpSpeedMultiplier or 1.0
	end
end

------------------------------------------------------------------------------------------------
-- Crafting: fast crafting at every craftbot, workbench and dispenser.
------------------------------------------------------------------------------------------------

local function craftBefore( self )
	local c = self.crafter
	if type( c ) ~= "table" then return end
	poll()
	H.craftSpeed = H.craftSpeed or {}
	if H.craftSpeed[c] == nil then H.craftSpeed[c] = c.speed end
	if on( "easy-craft" ) then
		c.speed = 10000
	else
		c.speed = H.craftSpeed[c]
	end
end

------------------------------------------------------------------------------------------------
-- Terrain: a picture of the whole world for the Hotswap map, made once per world.
------------------------------------------------------------------------------------------------

local function exportMap()
	if g_generatorIndex ~= 0 or type( g_cellData ) ~= "table" or not g_cellData.seed then return end
	if type( GetColorAt ) ~= "function" or type( GetHeightAt ) ~= "function" then return end
	local seed = string.format( "%.0f", g_cellData.seed )
	local file = DIR .. "map_" .. seed .. ".json"
	if not sm.json.fileExists( DIR .. "map_wanted.json" ) or sm.json.fileExists( file ) then return end
	local started = clock()
	-- The whole overworld (cells -64..63 by -48..47, 64 m each). Color every 4 m, height every 8 m.
	local x0, y0, x1, y1 = -64 * 64, -48 * 64, 64 * 64, 48 * 64
	local step = 4
	local fmt, floor, min, max = string.format, math.floor, math.min, math.max
	local colors, heights = {}, {}
	for py = 0, ( y1 - y0 ) / step - 1 do
		local y = y1 - ( py + 0.5 ) * step -- north at the top
		local row = {}
		for px = 0, ( x1 - x0 ) / step - 1 do
			local r, g, b = GetColorAt( x0 + ( px + 0.5 ) * step, y, 0 )
			row[px + 1] = fmt( "%02x%02x%02x", floor( min( max( r, 0 ), 1 ) * 255 ), floor( min( max( g, 0 ), 1 ) * 255 ), floor( min( max( b, 0 ), 1 ) * 255 ) )
		end
		colors[py + 1] = table.concat( row )
		if py % 2 == 0 then
			local hrow = {}
			for px = 0, ( x1 - x0 ) / ( step * 2 ) - 1 do
				local h = GetHeightAt( x0 + ( px + 0.5 ) * step * 2, y, 0 )
				hrow[px + 1] = fmt( "%03x", floor( min( max( ( h + 64 ) * 4, 0 ), 4095 ) ) )
			end
			heights[#heights + 1] = table.concat( hrow )
		end
	end
	sm.json.save( { version = 1, seed = seed, x0 = x0, y0 = y0, x1 = x1, y1 = y1, step = step, colors = colors, heights = heights }, file )
	log( string.format( "map saved for seed %s in %.1f s", seed, clock() - started ) )
end

------------------------------------------------------------------------------------------------

function H.hook()
	if SurvivalGame then wrap( SurvivalGame, "server_onFixedUpdate", gameTick ) end
	if SurvivalPlayer then wrap( SurvivalPlayer, "server_onFixedUpdate", playerTick ) end
	if MechanicCharacter then
		wrap( MechanicCharacter, "client_onUpdate", nil, characterUpdate )
		wrap( MechanicCharacter, "client_onClientDataUpdate", characterData )
	end
	for _, c in ipairs( { Crafter, Workbench, Dispenser, Craftbot, PortableCraftbot, MininghubDispenser } ) do
		wrap( c, "server_onFixedUpdate", craftBefore )
		wrap( c, "client_onUpdate", craftBefore )
	end
	-- Terrain script: Load runs for a saved world, Create for a new one.
	if type( Load ) == "function" and type( GetColorAt ) == "function" and not H.terrainHooked then
		H.terrainHooked = true
		local load, create = Load, Create
		Load = function( ... )
			local a, b, c = load( ... )
			try( exportMap )
			return a, b, c
		end
		if type( create ) == "function" then
			Create = function( ... )
				local a, b, c = create( ... )
				try( exportMap )
				return a, b, c
			end
		end
	end
end
