# Game recipes

Run the engine snippets with `cargo test --manifest-path game/Cargo.toml -p exact-game --doc recipes`.
Compile the renderer snippet with `cargo test --manifest-path game/Cargo.toml -p exact-game-render --doc blended_hook_geometry`.

The gameplay kit is what every game needs (`exact_game::steer`, `collect`, `spawn`,
and `Actions` for a shop); the [starter](../new/logic/src/lib.rs) uses all of it:

| Task | Recipe | Existing game or reference |
|---|---|---|
| Seek, flee, arrive, wander; a predator | [Steering](steering.md) | Starter's critter |
| Swim in a school | [Schooling](schooling.md) | Kit oracles (`engine/tests/kit.rs`) |
| Pickups that drift to the player, and a prompt | [Pickup and prompt](pickup.md) | Starter's coins |
| Spawn on world time with serials | [Spawner](spawner.md) | Starter's coins |
| A shop on the number keys | [Shop](shop.md) | Starter's shop and its bot-play test |

Everything else:

| Task | Recipe | Existing game or reference |
|---|---|---|
| Spawn props and sprites | [Literal builders](literal-builders.md) | Builder state tests |
| Follow a player | [Follow camera](follow-camera.md) | Starter |
| Display remaining time | [Timer HUD](timer-hud.md) | Starter HUD |
| Turn a first-person camera | [Mouse look](mouse-look.md) | Engine look tests |
| Play on a landing edge | [Sound on contact](sound-on-contact.md) | Audio fixture |
| Show touch controls on a touch screen | [Touch controls](touch-controls.md) | Starter |
| Answer a command | [Door message](door-message.md) | Engine HUD and events |
| Draw a ring and a translucent halo | [Halo](halo.md) | Beacons |
| Save a slot | [Save slot](save-slot.md) | Starter restore tests |
| Bubbles, a wake, a splash | [Emitter](emitter.md) | Particles fixture |
| Make sounds without files | [Synthesized sounds](synthesized-sounds.md) | Audio fixture |
| Orbit a third-person camera | [Orbit camera](orbit-camera.md) | Engine look tests |
| Flash what was hit | [Hit flash](hit-flash.md) | Engine presentation tests |
| Catch a tap inside one tick | [Same-tick press](same-tick-press.md) | Engine input tests |
| Cover a game until its first complete frame | [Loading cover](loading-cover.md) | [Surface publications](../README.md#publications-and-events) |
| Draw blended geometry in a hook | [Blended hook geometry](blended-hook-geometry.md) | [Render hooks](../render/README.md#game-render-hooks) |
| Export models from Blender | [Blender export](blender-export.md) | Asset fixture |
| Light a tank or a sea from inside | [Underwater](underwater.md) | Underwater GPU test |

Use the [engine reference](../engine/README.md) for borrowing and registration.
Names, units and defaults are available through `bun scripts/exact.mjs game vocab <name>`.
