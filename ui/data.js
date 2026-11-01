// Every game's mod list (names, descriptions, hotkeys), which mods work (LIVE), and the Home feed.

// `spec` (optional): what the mod changes, for games run by the engine readers (engines.rs), e.g.
// { at: 'GameManager.instance.money', min: 999999 }. One spec or a list. The key names the change:
// toggles set/min/max (a value), freeze (true), fill (a sibling field to copy); sliders scale
// (true: percent of the original) or value (true, times `mul`); actions add/put (a value).
// Optional `ty` ('i32', 'f32', 'bool', ...) when the engine can't tell the type.
const T = (name, info, spec) => ({ type: 'toggle', name, info, spec });
const S = (name, min, max, step, def, info, spec) => ({ type: 'slider', name, min, max, step, def, info, spec });
const A = (name, info, spec) => ({ type: 'action', name, info, spec });

// GTA V's mods (the plugin knows each `native` key).
const GTA_CATS = [
  { name: 'Player', mods: [T('God Mode', 'Nothing can hurt you.', { native: 'god' }), T('Never Wanted', 'The police never come after you.', { native: 'never-wanted' }),
    T('Fast Run', 'Run and sprint about 50% faster.', { native: 'fast-run' }), T('Fast Swim', '', { native: 'fast-swim' }), T('Super Jump', '', { native: 'super-jump' }),
    A('Heal and Armor', 'Full health and full armor.', { native: 'heal' })] },
  { name: 'Weapons', mods: [T('Infinite Ammo', 'No reloading, no running out.', { native: 'ammo' })] },
  { name: 'Vehicles', mods: [T('Vehicle God Mode', 'The vehicle you drive takes no damage.', { native: 'vehicle-god' }), A('Repair Vehicle', 'Fixes and cleans the vehicle you’re in.', { native: 'repair' }),
    A('Spawn Sports Car', 'A Truffade Adder, with you in it.', { native: 'spawn:adder' }), A('Spawn Helicopter', 'A Buzzard, with you in it.', { native: 'spawn:buzzard' })] },
  { name: 'Money', mods: [A('Add $1,000,000', 'For the character you’re playing (Michael, Franklin or Trevor).', { native: 'money:1000000' })] },
  { name: 'World', mods: [A('Teleport to Waypoint', 'Set a waypoint on the map first.', { native: 'waypoint' }), A('Set Time to Noon', '', { native: 'time:12' }),
    A('Set Time to Midnight', '', { native: 'time:0' }), A('Clear Weather', '', { native: 'weather:CLEAR' })] },
];

// One-time setup guides shown on a game's page until done (`setup` in the catalog). `check` asks
// the app whether it's done.
const SETUPS = {
  scripthookv: {
    title: 'Set up story mode mods',
    intro: 'GTA V needs Script Hook V, a free tool almost every story mode mod uses. HotSwap can’t include it, so it’s a one-time install.',
    steps: [
      'Download Script Hook V from its official page (button below). Pick the version for your game: Legacy or Enhanced.',
      'Open the zip, go into its <b>bin</b> folder and copy everything there (ScriptHookV.dll and the loader next to it) into your GTA V folder.',
      'Turn BattlEye off for story mode: in Steam, right-click GTA V → Properties → Launch options, type <code>-nobattleye</code>. (Or untick BattlEye in the Rockstar Games Launcher settings.) GTA Online won’t start this way; remove it to play online.',
      'Start GTA V. HotSwap adds its own plugin by itself and connects in story mode.',
    ],
    link: { url: 'https://www.dev-c.com/gtav/scripthookv/', label: 'Get Script Hook V' },
    done: 'Script Hook V is installed.',
  },
};

// Keyed by Steam app ID.
const CATALOG = {
  526870: { genre: 'Factory building', cats: [
    { name: 'Player', mods: [T('Infinite Health', "You can't die from damage. Falling off the map still kills you."), T('Unlimited Jetpack Fuel'), T('No Fall Damage', 'Landing from any height does no damage.')] },
    { name: 'Inventory', mods: [T('Infinite Items', 'Stacks in your inventory never go down, and used-up items come back.'), T('Easy Crafting', 'Building and workbench crafting need no materials at all. Reopen the workbench after picking a recipe.')] },
    { name: 'Physics', mods: [S('Walking Speed', 10, 1000, 10, 100, 'Percent of normal speed. 100 is the game default.'), S('Jump Height', 10, 1000, 10, 100, 'Percent of normal jump height.'), S('Game Speed', 0.1, 5, 0.1, 1), T('Super Speed'), S('Gravity Strength', 0, 200, 5, 100, 'Percent of normal gravity. 0 lets you float.')] },
    { name: 'Building', mods: [T('Free Production', 'Machines need no ingredients. Their outputs are real items.'), T('No Power Required'), T('No Fuel Needed', 'Generators and vehicles run without fuel.'), T('Turbo Production', 'Machines produce much faster.')] },
 
    { name: 'Teleport', mods: [A('Save Position', 'Remembers where you stand, even after a restart.'), A('Teleport to Saved Position', 'Jumps back to the saved spot. Fall damage is off for a moment after.')] },
  ]},
  3241660: { genre: 'Horror co-op', solo: true, cats: [
    { name: 'Player', mods: [T('Infinite Health', 'The game\'s own god mode, and health kept full.'), T('Infinite Stamina'), T('Infinite Jumps', 'Jump again in mid-air as often as you like.'), S('Movement Speed', 50, 300, 10, 100)] },
    { name: 'Carrying', mods: [S('Grab Strength', 100, 1000, 50, 100, 'Carry heavy things without strength upgrades. Percent of your current strength.'), S('Throw Strength', 100, 1000, 50, 100)] },
    { name: 'Vision', mods: [T('Enemy Radar', 'A radar in the top-right corner of your screen showing every enemy, through walls. Borderless or windowed mode.')] },
    { name: 'Round', mods: [T('Infinite Battery', 'Items and weapons never run out of battery.'), A('Complete Extractions', 'Skips every extraction point, so you can head straight to the truck.')] },
    { name: 'World', mods: [T('Valuables Never Break'), A('Add $10,000')] },
  ]},
  // Unturned: Unity on Mono (engines.rs). Its servers use VAC and BattlEye, so mods only run in
  // singleplayer: `offline` must hold every tick, and an anti-cheat loading stops HotSwap for good.
  304930: { genre: 'Survival', solo: true, experimental: true, offline: [
    { at: 'SDG.Unturned/Provider._isServer', is: 1 }, { at: 'SDG.Unturned/Provider.isVacActive', is: 0 }, { at: 'SDG.Unturned/Provider.isThirdpartyAntiCheatActive', is: 0 },
  ], cats: [
    { name: 'Player', mods: [T('Infinite Health', '', { at: 'SDG.Unturned/Player._localPlayer._life._health', min: 100 }),
      T('No Hunger', '', { at: 'SDG.Unturned/Player._localPlayer._life._food', min: 100 }),
      T('No Thirst', '', { at: 'SDG.Unturned/Player._localPlayer._life._water', min: 100 }),
      T('Infinite Stamina', '', { at: 'SDG.Unturned/Player._localPlayer._life._stamina', min: 100 }),
      T('No Infection', 'Immunity stays full.', { at: 'SDG.Unturned/Player._localPlayer._life._virus', min: 100 }),
      T('Infinite Oxygen', '', { at: 'SDG.Unturned/Player._localPlayer._life._oxygen', min: 100 })] },
    { name: 'Weapons', mods: [T('Infinite Ammo', 'The magazine in your gun never empties.', { at: 'SDG.Unturned/Player._localPlayer._equipment._useable.ammo', best: true }),
      T('No Reload'), T('No Weapon Decay', 'The item in your hands stays at full quality.', { at: 'SDG.Unturned/Player._localPlayer._equipment._quality', min: 100 })] },
    { name: 'Skills', mods: [A('Add 1,000 Experience', '', { at: 'SDG.Unturned/Player._localPlayer._skills._experience', add: 1000 })] },
  ]},
  220200: { genre: 'Space sim', cats: [
    { name: 'Vessel', mods: [T('Unlimited Fuel', 'Liquid fuel, oxidizer, solid fuel, xenon and EVA propellant stay full on the vessel you fly.'), T('Unlimited Monopropellant', 'RCS fuel stays full on the vessel you fly.'),
      T('Unlimited Flight Resources', 'Electric charge never runs out (the game’s own cheat), and ablator stays full.'),
      T('Invincible Craft', 'No crash damage, joints never break, and parts never overheat.'), T('No Overheating', 'Parts ignore their maximum temperature, but can still crash.')] },
    { name: 'Career', mods: [T('Unlimited Funds', 'Keeps funds topped up. Turning it off puts your balance back to what it was.'), T('Unlimited Research Points', 'Keeps science topped up. Turning it off puts it back to what it was.'),
      T('Max Reputation', 'Keeps reputation at the top. Turning it off puts it back.'), T('Any Contract', 'Agencies offer contracts no matter their mindset.'),
      A('Add 100,000 Funds'), A('Add 1,000 Science'), A('Add 100 Reputation')] },
    { name: 'Building', mods: [T('Part Clipping', 'Place parts inside each other in the VAB and SPH.'), T('Free Attachment', 'Attach parts at any angle.'),
      T('Unlimited Inventory', 'No kerbal inventory limits and no EVA construction mass limit.')] },
    { name: 'World', mods: [S('Gravity', 0, 200, 10, 100, 'Percent of normal gravity everywhere.'), T('Show Biomes', 'Colors planets by biome, for science hunting.')] },
  ]},
  387990: { genre: 'Sandbox', cats: [
    { name: 'Player', mods: [T('Infinite Health', 'The game’s own god mode: no damage of any kind, and health stays full.'), T('Infinite Breath', 'Never run out of air underwater.'),
      S('Run Speed', 100, 500, 25, 100, 'Percent of normal walking and sprinting speed.'), S('Jump Height', 100, 500, 25, 100, 'Percent of normal jump strength.')] },
    { name: 'Inventory', mods: [T('Unlimited Items', 'The game’s unlimited inventory: build with anything, nothing runs out.'), T('Easy Craft', 'Craftbots, workbenches and dispensers finish in about a second. With Unlimited Items, crafting is free too.'),
      A('Give 50 Components'), A('Give 100 Potato Ammo'), A('Give 20 Batteries'), A('Give 20 Gasoline'), A('Give 20 Glowsticks'), A('Give 50 Water')] },
    { name: 'World', mods: [T('No Aggro', 'Farmbots, haybots and other enemies ignore you.'), T('No Raids', 'No crop raids on your farm.'), T('Always Day', 'Time stops at noon. Turning it off lets the day run again.'),
      S('Gravity', 0, 200, 10, 100, 'Percent of normal gravity, for you and your creations.')] },
  ]},
  264710: { genre: 'Underwater survival', cats: [
    { name: 'Player', mods: [T('Infinite Health', 'No damage, and health stays full.'), T('Infinite Oxygen'), T('No Hunger'), T('No Thirst')] },
    { name: 'Movement', mods: [S('Swim Speed', 10, 500, 10, 100, 'Percent of normal swim speed.'), S('Walk Speed', 10, 500, 10, 100, 'Percent of normal walking speed.')] },
    { name: 'Crafting', mods: [T('Free Crafting', 'Crafting and building need no materials.'), T('Unlock All Blueprints'), T('Instant Building'), T('Instant Scanning'), T('Fast Growing', 'Plants grow and eggs hatch almost instantly.')] },
    { name: 'World', mods: [T('Infinite Energy', 'Tools, vehicles and bases never run out of power.'), T('No Crush Depth', 'Vehicles can dive to any depth.'), T('Passive Creatures', "Creatures don't attack.")] },
  ]},
  457140: { genre: 'Colony sim', cats: [
    { name: 'Building', mods: [T('Instant Build', 'The developers’ debug mode: buildings, digging and deconstruction finish instantly and need no materials.'),
      T('Sandbox Tools', 'Unlocks the game’s sandbox tools for this colony (press Shift+S in game). The game remembers the colony used sandbox.')] },
    { name: 'Duplicants', mods: [T('No Stress'), T('No Hunger', 'Calories stay as full as they’ve been.'), T('Infinite Breath', 'Duplicants never run out of breath.'),
      T('No Fatigue', 'Stamina stays full: no need to sleep.'), T('No Bladder', 'No bathroom breaks.'), T('Full Health', 'Health stays at its best.')] },
    { name: 'Research', mods: [T('Instant Research', 'The current research finishes as soon as a duplicant works on it.'), A('Finish Research Queue', 'Marks everything in your research queue as researched, no duplicant needed.')] },
    { name: 'Colony', mods: [T('Printing Pod Always Ready', 'New duplicants and care packages to pick from at any time.'), S('Max Game Speed', 100, 1000, 50, 100, 'Percent of the fastest speed setting. Click the speed 3 button again after changing it.')] },
  ]},
  1520370: { genre: 'Driving sim', cats: [
    { name: 'Needs', mods: [T('No Hunger', 'Your poutine need never builds up.'), T('No Thirst', 'Your maple syrup need never builds up.'), T('No Sleep', 'You never get tired.'), T('No Anxiety', 'Your anxiety never builds up.')] },
    { name: 'Money', mods: [T('Unlimited Money', 'Keeps your cash topped up. The cash on screen catches up at your next buy or sale. Turning it off puts your balance back.'), A('Add $10,000')] },
    { name: 'Player', mods: [S('Walk Speed', 50, 400, 10, 100, 'Percent of normal walking speed.'), S('Jump Height', 50, 400, 10, 100, 'Percent of normal jump strength.')] },
    { name: 'Vehicles', mods: [T('Infinite Fuel', 'Every fuel tank stays full, the home generator’s too.'), T('No Derby Damage', 'Your car takes no damage in the demolition derby.')] },
    { name: 'World', mods: [T('Freeze Time', 'The clock stops.'), T('Always Day', 'Noon, all the time.')] },
  ]},
  758690: { genre: 'Survival', cats: [
    { name: 'Player', mods: [T('Immortality', 'The game’s own cheat: you can’t die.'), T('No Hunger', 'Hunger doesn’t go down.'), T('No Thirst', 'Thirst doesn’t go down.'), T('Infinite Stamina', 'Running and jumping cost no stamina.'),
      S('Movement Speed', 50, 300, 10, 100, 'Percent of normal walking and running speed.')] },
    { name: 'Suit', mods: [T('Infinite Oxygen', 'No suffocation, and your oxygen stays full.'), T('Infinite Suit Energy', 'The suit uses no energy and its battery stays full.'), T('No Radiation', 'Radiation stays at zero.')] },
    { name: 'Building', mods: [T('Free Building', 'Build without materials: the building system’s own free-build switch.')] },
    { name: 'Vehicles', mods: [T('Infinite Rover Battery', 'Rover batteries stay charged.')] },
  ]},
  848450: { genre: 'Underwater survival', cats: [
    { name: 'Player', mods: [T('Infinite Health', 'No damage, and health stays full.'), T('Infinite Oxygen'), T('No Hunger'), T('No Thirst'), T('No Freezing', 'Your body heat never drops, even outside in the cold.')] },
    { name: 'Movement', mods: [S('Swim Speed', 10, 500, 10, 100, 'Percent of normal swim speed.'), S('Walk Speed', 10, 500, 10, 100, 'Percent of normal walking speed.')] },
    { name: 'Crafting', mods: [T('Free Crafting', 'Crafting and building need no materials.'), T('Unlock All Blueprints'), T('Fast Growing', 'Plants grow and eggs hatch almost instantly.')] },
    { name: 'World', mods: [T('Infinite Energy', 'Tools, vehicles and bases never run out of power.'), T('No Crush Depth', 'Vehicles and bases take no water pressure damage.'), T('Passive Creatures', "Creatures don't attack.")] },
  ]},
  1962700: { genre: 'Underwater survival', cats: [
    { name: 'Player', mods: [T('Infinite Health', 'Health stays full and damage is ignored.'), T('Infinite Oxygen'), T('No Hunger'), T('No Thirst'), T('No Temperature Damage', 'Heat and cold do no damage.'), T('Free Dash', 'Dashing costs no oxygen.')] },
    { name: 'Movement', mods: [S('Swim Speed', 10, 500, 10, 100, 'Percent of normal swim speed.'), S('Walk Speed', 10, 500, 10, 100, 'Percent of normal walking speed.')] },
    { name: 'Crafting', mods: [T('Free Crafting', 'Crafting and building need no materials. Reopen the crafting menu after turning it on.'), T('Instant Crafting'), T('Infinite Energy', 'Tools, vehicles and batteries never run out.')] },
  ]},
  // Euro Truck and American Truck Simulator: live, from the game's own save metadata (ets2.rs).
  227300: { genre: 'Driving sim', experimental: true, cats: [
    { name: 'Truck', mods: [T('No Damage', 'Your truck, trailer and cargo take no new damage or wear. Existing damage stays: use Repair.'), T('Infinite Fuel'), T('No Fatigue', 'You never need to sleep.'),
      A('Repair Truck and Trailer'), A('Refuel')] },
    { name: 'Career', mods: [T('Infinite Money', 'Your money never goes down; earnings still add up.'), A('Add €100,000'), A('Add 10,000 XP'), A('Level Up', 'Straight to the next level, for one more skill point.')] },
  ]},
  270880: { genre: 'Driving sim', experimental: true, cats: [
    { name: 'Truck', mods: [T('No Damage', 'Your truck, trailer and cargo take no new damage or wear. Existing damage stays: use Repair.'), T('Infinite Fuel'), T('No Fatigue', 'You never need to sleep.'),
      A('Repair Truck and Trailer'), A('Refuel')] },
    { name: 'Career', mods: [T('Infinite Money', 'Your money never goes down; earnings still add up.'), A('Add $100,000'), A('Add 10,000 XP'), A('Level Up', 'Straight to the next level, for one more skill point.')] },
  ]},
  // GTA V Legacy and Enhanced: story mode through Script Hook V and HotSwap's plugin (bridge.rs,
  // gta-asi). BattlEye has to be off, which also keeps the game out of GTA Online.
  271590: { genre: 'Action', experimental: true, setup: 'scripthookv', offline: true,
    offlineNote: 'Story mode only. GTA Online uses BattlEye: mods need BattlEye off, which keeps the game out of GTA Online.', cats: GTA_CATS },
  3240220: { genre: 'Action', experimental: true, setup: 'scripthookv', offline: true,
    offlineNote: 'Story mode only. GTA Online uses BattlEye: mods need BattlEye off, which keeps the game out of GTA Online.', cats: GTA_CATS },
  // Mr.Mine: an Electron game, modded through its page (cdp.rs). Start it from HotSwap.
  1397920: { genre: 'Idle', experimental: true, cats: [
    { name: 'Money', mods: [T('Infinite Money', 'Buying things costs nothing.', { js: 'window.__hsSub = window.__hsSub || subtractMoney; subtractMoney = () => {}', off: 'if (window.__hsSub) subtractMoney = window.__hsSub' }),
      A('Double Money', 'Adds as much money as you have now.', { js: 'addMoney(money)' })] },
  ]},
  // Stormworks: through the game's own addon API (stormworks.rs). The addon has to be on in the world.
  573090: { genre: 'Sandbox', experimental: true, cats: [
    { name: 'Money', mods: [T('Infinite Money', 'The game’s own infinite money setting.'), A('Add $100,000'), A('Add 100 Research Points')] },
    { name: 'Vehicles', mods: [T('Infinite Fuel'), T('Infinite Batteries'), T('Infinite Ammo'), T('No Vehicle Damage'), T('No Engine Overheating')] },
    { name: 'Player', mods: [T('No Player Damage'), T('No Clip', 'Walk and fly through anything.'), T('Map Teleport', 'Click the map to teleport there.')] },
    { name: 'World', mods: [T('Unlock All Components', 'Every vehicle part in the editor.'), T('Unlock All Islands'), T('Clear Fog of War', 'Reveals the whole map.'),
      T('Clear Weather', 'No fog, rain or wind.'), T('No Sharks'), T('No Megalodon')] },
  ]},
};

// Mods for any single-player game of an engine (without its own list above), from the engine itself.
const ENGINE_NAMES = { 'unity-mono': 'Unity', 'unity-il2cpp': 'Unity', unreal: 'Unreal Engine', 'rpgmaker-mv': 'RPG Maker MV', 'rpgmaker-mz': 'RPG Maker MZ', renpy: "Ren'Py", godot: 'Godot' };
// Engines whose mods can be described as data (`spec`).
const SPEC_ENGINES = ['unity-mono', 'unity-il2cpp', 'unreal', 'electron', 'nwjs', 'rpgmaker-mv', 'rpgmaker-mz', 'renpy', 'godot', 'gta'];
const ENGINE_CATS = {
  'unity-mono': [
    { name: 'Game', mods: [S('Game Speed', 10, 500, 10, 100, 'Percent of normal speed, for everything in the game.', { at: 'UnityEngine/Time.timeScale()', value: true, mul: 0.01 })] },
    { name: 'Display', mods: [
      T('Unlock FPS', 'No frame rate cap and no VSync from the game, so it runs as fast as your PC can.', [
        { at: 'UnityEngine/QualitySettings.vSyncCount()', set: 0, ty: 'i32' }, { at: 'UnityEngine/Application.targetFrameRate()', set: 1000, ty: 'i32' }]),
      S('Field of View', 50, 150, 5, 100, 'Percent of the game’s own field of view, for the main camera.', { at: 'UnityEngine/Camera.main().fieldOfView()', scale: true })] },
  ],
  unreal: [
    { name: 'Game', mods: [S('Game Speed', 10, 500, 10, 100, 'Percent of normal speed, for everything in the game.', { at: 'WorldSettings.TimeDilation', value: true, mul: 0.01 })] },
    { name: 'Display', mods: [S('Field of View', 0, 130, 5, 0, 'Degrees, for the player’s camera. 0 is the game’s own.', { at: 'PlayerCameraManager.LockedFOV', value: true })] },
  ],
};
// RPG Maker MV and MZ share their game objects, so these work in any of their games.
ENGINE_CATS['rpgmaker-mv'] = [
  { name: 'Party', mods: [T('Infinite HP', 'Everyone in the party stays at full HP.', { at: '$gameParty.members()[]._hp', fill: 'mhp' }),
    T('Infinite MP', 'Everyone in the party stays at full MP.', { at: '$gameParty.members()[]._mp', fill: 'mmp' }),
    A('Level Up Party', 'Every party member goes up one level.', { js: '$gameParty.members().forEach(a => a.changeLevel(a.level + 1, false))' })] },
  { name: 'Gold', mods: [T('Infinite Gold', 'Gold stays at 999,999 or more.', { at: '$gameParty._gold', min: 999999 }), A('Add 10,000 Gold', '', { at: '$gameParty._gold', add: 10000 })] },
  { name: 'Battle', mods: [A('Defeat All Enemies', 'Every enemy in the current battle falls.', { js: '$gameTroop.members().forEach(e => e.addState(e.deathStateId()))' })] },
  { name: 'World', mods: [T('No Random Encounters', 'Walk anywhere without random battles.', { at: '$gamePlayer._encounterCount', min: 999 }),
    T('Walk Through Walls', '', { at: '$gamePlayer._through', set: 1 }), S('Move Speed', 4, 6, 1, 4, '4 is the game default.', { at: '$gamePlayer._moveSpeed', value: true }),
    T('Save Anywhere', 'Saving in the menu is always allowed.', { at: '$gameSystem._saveEnabled', set: 1 })] },
];
ENGINE_CATS['rpgmaker-mz'] = ENGINE_CATS['rpgmaker-mv'];
// Ren'Py and Godot: HotSwap adds a small script to the game (bridge.rs); the game needs one restart.
ENGINE_CATS.renpy = [
  { name: 'Game', mods: [T('Developer Console', "Ren'Py's own console: press Shift+O in game.", { at: 'config.console', set: 1 }),
    T('Skip Unseen Text', 'Skipping also skips text you haven’t read yet.', { at: '_preferences.skip_unseen', set: 1 }),
    T('Rollback Anywhere', 'Scroll back even where the game turns rollback off.', { at: 'config.rollback_enabled', set: 1 })] },
];
ENGINE_CATS.godot = [
  { name: 'Game', mods: [S('Game Speed', 10, 500, 10, 100, 'Percent of normal speed, for everything in the game.', { at: 'Engine.time_scale', value: true, mul: 0.01 })] },
];
ENGINE_CATS['unity-il2cpp'] = ENGINE_CATS['unity-mono'];

// Mods the trainer can actually apply, by Steam app ID. Everything else shows as "Soon".
const LIVE = {
  526870: [
    'infinite-health', 'unlimited-jetpack-fuel', 'no-fall-damage', 'infinite-items', 'easy-crafting',
    'walking-speed', 'jump-height', 'game-speed', 'super-speed', 'gravity-strength',
    'free-production', 'no-power-required', 'no-fuel-needed', 'turbo-production',
    'save-position', 'teleport-to-saved-position',
  ],
  220200: ['unlimited-fuel', 'unlimited-monopropellant', 'unlimited-flight-resources', 'invincible-craft', 'no-overheating', 'unlimited-funds', 'unlimited-research-points', 'max-reputation', 'any-contract',
    'add-100-000-funds', 'add-1-000-science', 'add-100-reputation', 'part-clipping', 'free-attachment', 'unlimited-inventory', 'gravity', 'show-biomes'],
  3241660: ['infinite-health', 'infinite-stamina', 'infinite-jumps', 'movement-speed', 'grab-strength', 'throw-strength', 'enemy-radar', 'infinite-battery', 'complete-extractions'],
  457140: ['instant-build', 'sandbox-tools', 'printing-pod-always-ready', 'max-game-speed', 'no-stress', 'no-hunger', 'infinite-breath', 'no-fatigue', 'no-bladder', 'full-health', 'instant-research', 'finish-research-queue'],
  1520370: ['no-hunger', 'no-thirst', 'no-sleep', 'no-anxiety', 'unlimited-money', 'add-10-000', 'walk-speed', 'jump-height', 'infinite-fuel', 'no-derby-damage', 'freeze-time', 'always-day'],
  848450: ['infinite-health', 'infinite-oxygen', 'no-hunger', 'no-thirst', 'no-freezing', 'swim-speed', 'walk-speed', 'free-crafting', 'unlock-all-blueprints',
    'fast-growing', 'infinite-energy', 'no-crush-depth', 'passive-creatures'],
  264710: [
    'infinite-health', 'infinite-oxygen', 'no-hunger', 'no-thirst', 'swim-speed', 'walk-speed',
    'free-crafting', 'unlock-all-blueprints', 'instant-building', 'instant-scanning', 'fast-growing',
    'infinite-energy', 'no-crush-depth', 'passive-creatures',
  ],
  1962700: [
    'infinite-health', 'infinite-oxygen', 'no-hunger', 'no-thirst', 'no-temperature-damage', 'free-dash',
    'swim-speed', 'walk-speed', 'free-crafting', 'instant-crafting', 'infinite-energy',
  ],
  227300: ['no-damage', 'infinite-fuel', 'no-fatigue', 'repair-truck-and-trailer', 'refuel', 'infinite-money', 'add-100-000', 'add-10-000-xp', 'level-up'],
  270880: ['no-damage', 'infinite-fuel', 'no-fatigue', 'repair-truck-and-trailer', 'refuel', 'infinite-money', 'add-100-000', 'add-10-000-xp', 'level-up'],
  573090: [
    'infinite-money', 'add-100-000', 'add-100-research-points', 'infinite-fuel', 'infinite-batteries', 'infinite-ammo',
    'no-vehicle-damage', 'no-engine-overheating', 'no-player-damage', 'no-clip', 'map-teleport',
    'unlock-all-components', 'unlock-all-islands', 'clear-fog-of-war', 'clear-weather', 'no-sharks', 'no-megalodon',
  ],
};

// Used when the page is opened outside the app (no Steam scan available).
const DEMO_LIBRARY = [
  { appId: 526870, name: 'Satisfactory' }, { appId: 1962700, name: 'Subnautica 2' }, { appId: 227300, name: 'Euro Truck Simulator 2' },
  { appId: 220200, name: 'Kerbal Space Program' }, { appId: 387990, name: 'Scrap Mechanic' }, { appId: 304930, name: 'Unturned' },
  { appId: 3241660, name: 'R.E.P.O.' },
];

// Home feed snapshot from 2026-10-26 (Steam News API, Steam store, CheapShark). Live loading comes later.
const SNAPSHOT = 'October 26, 2026';
const NEWS = [
  { app: 227300, title: 'Iceland expansion gets a trailer and a November 10 release date', text: 'SCS Software showed off the Iceland map DLC for Euro Truck Simulator 2.', date: '2026-10-26', src: 'PlayGround.ru, translated', feature: true },
  { app: 227300, title: 'Iceland: Viewpoints', date: '2026-10-26', src: 'Steam announcement' },
  { app: 1962700, title: 'Beyond the Surface: Voyage to Early Access 2', date: '2026-10-21', src: 'Steam announcement' },
  { app: 1962700, title: 'New update reveals the colonists\' story and Aksum\'s past', date: '2026-10-21', src: 'PlayGround.ru, translated' },
  { app: 526870, title: 'Satisfactory: The board game campaign is live', date: '2026-10-13', src: 'Steam announcement' },
  { app: 526870, title: '1.2 Update Fixes v1.2.4.1', date: '2026-10-12', src: 'Steam patch notes' },
  { app: 526870, title: 'Core Values, the first major expansion, takes the game underground', date: '2026-09-30', src: 'PlayGround.ru, translated' },
  { app: 3241660, title: 'A new R.E.P.O. level?!', date: '2026-06-05', src: 'Steam announcement' },
  { app: 220200, title: 'KSP hits its highest concurrent player count on Steam in over 10 years', date: '2026-04-07', src: 'PC Gamer' },
];
const DEALS = [
  { t: 'Rise of the Tomb Raider', app: 391220, sale: 2.99, was: 29.99, off: 90, rating: 'Very Positive' },
  { t: 'Metro 2033 Redux', app: 286690, sale: 1.99, was: 19.99, off: 90, rating: 'Very Positive' },
  { t: 'Green Hell', app: 815370, sale: 2.49, was: 24.99, off: 90, rating: 'Very Positive' },
  { t: 'Killing Floor 2', app: 232090, sale: 2.99, was: 29.99, off: 90, rating: 'Very Positive' },
  { t: 'Super Meat Boy', app: 40800, sale: 1.19, was: 14.99, off: 92, rating: 'Very Positive' },
  { t: 'Invincible Presents: Atom Eve', app: 2060870, sale: 0.99, was: 9.99, off: 90, rating: 'Overwhelmingly Positive' },
  { t: 'Grim Fandango Remastered', app: 316790, sale: 2.24, was: 14.99, off: 85, rating: 'Very Positive' },
  { t: 'BioShock', app: 7670, sale: 4.99, was: 19.99, off: 75, rating: 'Very Positive' },
  { t: 'Project Warlock', app: 893680, sale: 1.56, was: 12.00, off: 87, rating: 'Very Positive' },
];
const TOP = [
  { t: 'Steam Frame', app: 4165890, price: 1059, note: 'Hardware' },
  { t: 'Call of Duty: Modern Warfare 4', app: 4435490, price: 69.99 },
  { t: 'Steam Machine', app: 4165910, price: 1049, note: 'Hardware' },
  { t: 'Moon Sweepers', app: 5335510, price: 7.64, off: 15 },
  { t: 'Cyberpunk 2077', app: 1091500, price: 17.99, off: 70 },
  { t: 'Cyberpunk 2077: Ultimate Edition', app: 32470, price: 33.10, off: 60 },
  { t: 'Cyberpunk 2077: Phantom Liberty', app: 2138330, price: 17.99, off: 40, note: 'DLC' },
];
const NEW_RELEASES = [
  { t: 'The Mailroom', app: 3965430, price: 6.79, off: 15 },
  { t: 'Nitro Nonsense', app: 5000550, price: 3.59, off: 40 },
  { t: 'Chain of Command', app: 4439360, price: 4.24, off: 15 },
  { t: 'Adventurer 95', app: 5278300, price: 9.99 },
  { t: 'Trials of Dash', app: 5174650, price: 3.99, off: 20 },
];
