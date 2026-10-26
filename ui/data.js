// Mod catalog and Home feed. Mods are UI-only for now: nothing here touches a game.

const T = (name, info) => ({ type: 'toggle', name, info });
const S = (name, min, max, step, def, info) => ({ type: 'slider', name, min, max, step, def, info });
const A = (name, info) => ({ type: 'action', name, info });

// Keyed by Steam app ID.
const CATALOG = {
  526870: { genre: 'Factory building', cats: [
    { name: 'Player', mods: [T('Infinite Health'), T('Unlimited Jetpack Fuel'), T('No Fall Damage'), T('Instant Pod Return', 'Drop pods and crates return to you immediately.')] },
    { name: 'Inventory', mods: [T('Infinite Items'), T('Free Crafting', 'Craft at the workbench without consuming parts.'), A('Add 1,000 Coupons')] },
    { name: 'Physics', mods: [S('Walking Speed', 10, 1000, 10, 100, 'Percent of normal speed. 100 is the game default.'), S('Jump Height', 10, 1000, 10, 100, 'Percent of normal jump height.'), S('Game Speed', 0.1, 5, 0.1, 1), T('Super Speed'), S('Gravity Strength', 0, 200, 5, 100, 'Percent of normal gravity. 0 lets you float.')] },
    { name: 'Building', mods: [T('Free Building'), T('No Power Required'), T('Unlock All Milestones', 'Changes your save. Back it up first.')] },
  ]},
  3241660: { genre: 'Horror co-op', solo: true, cats: [
    { name: 'Player', mods: [T('Infinite Health'), T('Infinite Stamina'), S('Movement Speed', 50, 300, 10, 100)] },
    { name: 'World', mods: [T('Valuables Never Break'), A('Add $10,000')] },
  ]},
  304930: { genre: 'Survival', solo: true, cats: [
    { name: 'Player', mods: [T('Infinite Health'), T('No Hunger'), T('No Thirst'), T('Infinite Stamina')] },
    { name: 'Weapons', mods: [T('Infinite Ammo'), T('No Reload'), T('No Weapon Decay')] },
  ]},
  220200: { genre: 'Space sim', cats: [
    { name: 'Vessel', mods: [T('Infinite Fuel'), T('Infinite Electricity'), T('Unbreakable Parts'), T('No Overheating')] },
    { name: 'Career', mods: [A('Add 100,000 Funds'), A('Add 1,000 Science'), A('Add 100 Reputation')] },
    { name: 'Time', mods: [S('Physics Warp', 1, 4, 1, 1)] },
  ]},
  387990: { genre: 'Sandbox', cats: [
    { name: 'Player', mods: [T('Infinite Health'), T('No Hunger'), T('No Thirst')] },
    { name: 'Building', mods: [T('Free Crafting'), T('Infinite Components'), T('Infinite Batteries')] },
  ]},
  1962700: { genre: 'Underwater survival', cats: [
    { name: 'Player', mods: [T('Infinite Oxygen'), T('Infinite Health'), T('No Hunger'), T('No Thirst'), S('Swim Speed', 50, 400, 10, 100)] },
    { name: 'Crafting', mods: [T('Free Crafting'), T('Free Building'), T('Infinite Battery')] },
  ]},
  227300: { genre: 'Driving sim', cats: [
    { name: 'Truck', mods: [T('No Damage'), T('Infinite Fuel'), T('No Fatigue')] },
    { name: 'Career', mods: [A('Add €100,000'), A('Add 5 Skill Points'), A('Add 10,000 XP')] },
  ]},
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
