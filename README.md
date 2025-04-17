# GRIM Engine

(Gui's Really ~Interesting~ Incomplete MUD Engine)

## Context

I've been playing/developing/involved with Multi-User Dungeon (MUD) text-based games for upwards of 15 years. I've always loved the idea of exploring worlds, and the allure of MUDs has been the simplicity of the format. Using words and symbols to describe characters, areas, items, and tell a story through words and colors also doesn't require that you be an amazing artist to convey ideas or concepts.

Based on this love for MUDs, I've tried in the past to [make an engine in Python](https://github.com/gvanderest/undermountain) and didn't love where it ended up, but this time I've started finding a passion for the Rust programming language and want to give it another try.

(Shameless plug: Check out the existing ROT-based (heavily modified) MUD we still keep running since 1997 at https://www.waterdeep.org/)

## Warning and Expectation-Setting

This is a hobby project for me and purely for fun to learn Rust and create something that others might enjoy, but priority one for me is to enjoy it. Development may never even remotely get off the ground or may one day halt. Feel free to fork this project and make something that goes in the direction you'd like.

## Initial Design / Thoughts

- Primarily library/composition-based, use modules where possible to try to allow systems to be layered
- Expectation would be that someone wanting a MUD would clone a template which uses all the library crates.. compiles.. runs.. and away they go
- If any database/querying is used, try to use something local to avoid dependencies on external spinups of DB's, etc.
    - Lean towards something like SQLite, if needed, but lean more towards a storage driver that's slightly agnostic to "where"
- Looking into Bevy's Entity-Component-System (ECS) implementation, might make a good base to use and save a lot of headache reinventing the wheel

## Getting Started

TODO

## Contributing

TODO

## Roadmap / Features

Keep in mind that this will be an overly extensive list of features that would be nice to have, with near-zero actual timeline for deliveries. Some features may be more obvious short-term versus long, but this is only meant to be an offloading ground for ideas at this time.

- Connecting to the server
    - Telnet
    - Websockets
    - SSH?
- Character creation/customization
- Builder support
    - OLC type commands
    - Multiple ports.. one for main and one for building, maybe more for staging, etc.
    - Copyover functionality
- Account system with recovery options
    - Email integration
    - Verification options
    - OTP?
- Areas, Rooms, Items, Exits
    - Resetting/respawn areas on timers
- Color codes and converting to ANSI
    - Learning more about other color options and extending this
- NPCs/Creatures/Spawning
- Combat
- Experience/Leveling
- Quest system
- Mail and notes system
- Titles and descriptions
- Equipment
- Stats system for computing power
- Skills and spells
- Auras/effects on your player
- Support for additional layered protocols (sound? images?)
- Races and Classes
- Extended Classes and ascendancy possibilities
- Skill trees
- Equipment set pieces for bonuses
- Randomized equipment stat ranges, suffixes and prefixes
- Area instancing (dungeons, but also just personal areas for events)
    - Dungeons on demand
    - Battlegrounds
    - Raid-style weekly group resets
    - Smaller events that are a little more in-the-moment
- Mob/Areas/Room/Global(?) scripts/progs and attachment/triggers
    - Lua scripting
    - Python scripting
    - Perhaps ROT scripting conversion tools
- Builder commands
- Grouping and shared experience
- Followers/pets
- Mounted combat
- Hired mercenaries
- Clans/Guilds/Factions with player management
- Player housing
- Administrative commands
    - Startup/shutdown/rebooting
    - Backup management and snapshots
    - Snooping
    - IP/hostname banning
    - Punishment management tools.. corner, silence, transport, force logout, freeze, etc.
- DDOS protections
