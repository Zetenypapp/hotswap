-- HotSwap for Farming Simulator 19 and 22 (see src-tauri/src/farming.rs). HotSwap writes
-- modSettings/HotSwap/in.xml with the mods that are on and how often each action was pressed; this
-- reads it once a second and writes out.xml, so the app knows a save with the mod is running and
-- whether this game is the server. Only the server (single-player, or the host of a multiplayer
-- game) changes anything; joined players' games do nothing.

HotSwap = { timer = 0, run = nil, done = {} }

local root = getUserProfileAppPath() .. "modSettings/"
local dir = root .. "HotSwap/"

function HotSwap:loadMap(name)
    createFolder(root)
    createFolder(dir)
end

-- The app's file: <hs run="..."><m k="infinite-money" v="1"/><a k="add-100-000" n="2"/></hs>
local function readIn()
    local path = dir .. "in.xml"
    if not fileExists(path) then
        return nil
    end
    local xml = loadXMLFile("HotSwapIn", path)
    if xml == nil or xml == 0 then
        return nil
    end
    local r = { run = getXMLString(xml, "hs#run"), m = {}, a = {} }
    local i = 0
    while true do
        local key = string.format("hs.m(%d)", i)
        if not hasXMLProperty(xml, key) then
            break
        end
        r.m[getXMLString(xml, key .. "#k")] = getXMLFloat(xml, key .. "#v") or 0
        i = i + 1
    end
    i = 0
    while true do
        local key = string.format("hs.a(%d)", i)
        if not hasXMLProperty(xml, key) then
            break
        end
        r.a[getXMLString(xml, key .. "#k")] = getXMLInt(xml, key .. "#n") or 0
        i = i + 1
    end
    delete(xml)
    return r
end

local function farmId()
    return g_currentMission:getFarmId()
end

local function money()
    local farm = g_farmManager:getFarmById(farmId())
    return farm ~= nil and farm.money or 0
end

local function addMoney(amount)
    g_currentMission:addMoney(amount, farmId(), MoneyType.OTHER, true, true)
end

-- Every fuel tank of the vehicle (diesel, DEF, electric charge, methane), filled up.
local FUELS = { "DIESEL", "DEF", "ELECTRICCHARGE", "METHANE" }
local function refuel(vehicle)
    if vehicle == nil or vehicle.getFillUnits == nil then
        return
    end
    for i, _ in ipairs(vehicle:getFillUnits()) do
        local ft = vehicle:getFillUnitFillType(i)
        for _, name in ipairs(FUELS) do
            if FillType[name] ~= nil and ft == FillType[name] then
                local missing = vehicle:getFillUnitCapacity(i) - vehicle:getFillUnitFillLevel(i)
                if missing > 0.01 then
                    vehicle:addFillUnitFillLevel(vehicle:getOwnerFarmId(), i, missing, ft, ToolType.UNDEFINED, nil)
                end
            end
        end
    end
end

local function repair(vehicle)
    if vehicle ~= nil and vehicle.repairVehicle ~= nil then
        vehicle:repairVehicle()
    end
end

function HotSwap:update(dt)
    self.timer = self.timer + dt
    if self.timer < 1000 or g_currentMission == nil then
        return
    end
    self.timer = 0
    local server = g_currentMission:getIsServer()
    local out = createXMLFile("HotSwapOut", dir .. "out.xml", "hs")
    if out ~= nil and out ~= 0 then
        setXMLInt(out, "hs#beat", math.floor(g_time or 0))
        setXMLBool(out, "hs#server", server)
        saveXMLFile(out)
        delete(out)
    end
    if not server then
        return
    end
    local s = readIn()
    if s == nil then
        return
    end
    -- A new HotSwap run counts its actions from zero; presses from before this save opened don't count.
    if s.run ~= self.run then
        self.run = s.run
        self.done = {}
        for k, n in pairs(s.a) do
            self.done[k] = n
        end
    end
    local vehicle = g_currentMission.controlledVehicle
    if (s.m["infinite-money"] or 0) ~= 0 and money() < 1000000 then
        addMoney(1000000 - money())
    end
    if (s.m["infinite-fuel"] or 0) ~= 0 then
        refuel(vehicle)
    end
    for k, n in pairs(s.a) do
        local had = self.done[k] or 0
        for _ = had + 1, n do
            if k == "add-100-000" then
                addMoney(100000)
            elseif k == "refuel-vehicle" then
                refuel(vehicle)
            elseif k == "repair-vehicle" then
                repair(vehicle)
            end
        end
        self.done[k] = math.max(had, n)
    end
end

addModEventListener(HotSwap)
