// Mod catalog and Home feed. Mods are UI-only for now: nothing here touches a game.

const T = (name, info) => ({ type: 'toggle', name, info });
const S = (name, min, max, step, def, info) => ({ type: 'slider', name, min, max, step, def, info });
const A = (name, info) => ({ type: 'action', name, info });

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
  304930: { genre: 'Survival', solo: true, cats: [
    { name: 'Player', mods: [T('Infinite Health'), T('No Hunger'), T('No Thirst'), T('Infinite Stamina')] },
    { name: 'Weapons', mods: [T('Infinite Ammo'), T('No Reload'), T('No Weapon Decay')] },
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
    { name: 'Research', mods: [T('Instant Research', 'The current research finishes as soon as a duplicant works on it.')] },
    { name: 'Colony', mods: [T('Printing Pod Always Ready', 'New duplicants and care packages to pick from at any time.'), S('Max Game Speed', 100, 1000, 50, 100, 'Percent of the fastest speed setting. Click the speed 3 button again after changing it.')] },
  ]},
  1520370: { genre: 'Driving sim', cats: [
    { name: 'Needs', mods: [T('No Hunger', 'Your poutine bar stays full.'), T('No Thirst', 'Your maple syrup bar stays full.'), T('No Sleep', 'Your energy bar stays full.'), T('No Anxiety', 'Your anxiety bar stays full.')] },
    { name: 'Money', mods: [T('Unlimited Money', 'Keeps your cash topped up. Turning it off puts your balance back.'), A('Add $10,000')] },
    { name: 'Vehicles', mods: [T('Infinite Fuel', 'Every fuel tank stays full.')] },
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
  227300: { genre: 'Driving sim', cats: [
    { name: 'Truck', mods: [T('No Damage'), T('Infinite Fuel'), T('No Fatigue')] },
    { name: 'Career', mods: [A('Add €100,000'), A('Add 5 Skill Points'), A('Add 10,000 XP')] },
  ]},
};

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
  457140: ['instant-build', 'sandbox-tools', 'printing-pod-always-ready', 'max-game-speed', 'no-stress', 'no-hunger', 'infinite-breath', 'no-fatigue', 'no-bladder', 'full-health', 'instant-research'],
  1520370: ['no-hunger', 'no-thirst', 'no-sleep', 'no-anxiety', 'unlimited-money', 'add-10-000', 'infinite-fuel'],
  758690: ['immortality', 'no-hunger', 'no-thirst', 'infinite-stamina', 'movement-speed', 'infinite-oxygen', 'infinite-suit-energy', 'no-radiation', 'free-building', 'infinite-rover-battery'],
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
