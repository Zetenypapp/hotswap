-- HotSwap for Stormworks: an addon that asks HotSwap (on this PC) twice a second which mods are
-- on, and applies them with the game's own addon API. Only talks to localhost, as the game allows.
-- Mods are switched with game settings, so turning one off puts the setting back as it was.

local PORT = 47823

-- mod id -> { game setting, value while the mod is on }
local SETTINGS = {
  ["infinite-money"] = { "infinite_money", true },
  ["infinite-fuel"] = { "infinite_fuel", true },
  ["infinite-batteries"] = { "infinite_batteries", true },
  ["infinite-ammo"] = { "infinite_ammo", true },
  ["no-vehicle-damage"] = { "vehicle_damage", false },
  ["no-engine-overheating"] = { "engine_overheating", false },
  ["no-player-damage"] = { "player_damage", false },
  ["no-clip"] = { "no_clip", true },
  ["map-teleport"] = { "map_teleport", true },
  ["unlock-all-components"] = { "unlock_all_components", true },
  ["unlock-all-islands"] = { "unlock_all_islands", true },
  ["clear-fog-of-war"] = { "clear_fow", true },
  ["no-sharks"] = { "sharks", false },
  ["no-megalodon"] = { "megalodon", false },
}

local timer = 0
local waiting = false
local seen = nil -- action counts from HotSwap; the first reply only sets the baseline
local run = nil
local misses = 0 -- replies in a row without HotSwap (closed, or not running)

-- Settings HotSwap changed: setting -> the value before (kept in the save, so a reload puts them back).
g_savedata = g_savedata or {}

local function applied() g_savedata.hs_orig = g_savedata.hs_orig or {} return g_savedata.hs_orig end

function onTick(ticks)
  timer = timer + 1
  if timer < 30 or waiting then return end
  timer = 0
  waiting = true
  server.httpGet(PORT, "/hs?m=" .. math.floor(server.getCurrency()) .. "&r=" .. math.floor(server.getResearchPoints()))
end

function httpReply(port, request, reply)
  if port ~= PORT then return end
  waiting = false
  local mods, counts, newrun = {}, {}, nil
  if type(reply) ~= "string" or reply:sub(1, 3) ~= "hs\n" then
    -- No HotSwap for about 10 seconds: put every setting back (the mods list stays empty).
    misses = misses + 1
    if misses < 20 then return end
    reply = ""
  else
    misses = 0
  end
  for line in reply:gmatch("[^\n]+") do
    local k, v = line:match("^m ([%w%-]+) ([%-%d%.]+)$")
    if k then mods[k] = tonumber(v) end
    local a, n = line:match("^a ([%w%-]+) (%d+)$")
    if a then counts[a] = tonumber(n) end
    local r = line:match("^run (%d+)$")
    if r then newrun = r end
  end

  -- Game settings: on -> remember the old value and set ours; off -> put the old value back.
  local now, orig = server.getGameSettings(), applied()
  for id, s in pairs(SETTINGS) do
    local name, want = s[1], s[2]
    if (mods[id] or 0) > 0 then
      if orig[name] == nil then orig[name] = now[name] end
      if now[name] ~= want then server.setGameSetting(name, want) end
    elseif orig[name] ~= nil then
      server.setGameSetting(name, orig[name])
      orig[name] = nil
    end
  end

  -- Clear Weather: no fog, rain or wind while on.
  if (mods["clear-weather"] or 0) > 0 then
    if not g_savedata.hs_weather then g_savedata.hs_weather = true server.setGameSetting("override_weather", true) end
    server.setWeather(0, 0, 0)
  elseif g_savedata.hs_weather then
    g_savedata.hs_weather = nil
    server.setGameSetting("override_weather", false)
  end

  -- Actions: HotSwap counts presses; act on the ones made since the last reply.
  if misses > 0 then return end
  if newrun ~= run then run = newrun seen = nil end
  if seen == nil then seen = counts return end
  local function pressed(id) local n = (counts[id] or 0) - (seen[id] or 0) return n > 0 and n or 0 end
  local money, research = server.getCurrency(), server.getResearchPoints()
  local m = pressed("add-100-000")
  local r = pressed("add-100-research-points")
  if m > 0 or r > 0 then server.setCurrency(money + m * 100000, research + r * 100) end
  seen = counts
end
