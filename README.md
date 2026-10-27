# bevy_fsm

[![CI](https://github.com/MolecularSadism/bevy_fsm/workflows/CI/badge.svg)](https://github.com/MolecularSadism/bevy_fsm/actions)
[![Crates.io](https://img.shields.io/crates/v/bevy_fsm.svg)](https://crates.io/crates/bevy_fsm)
[![Docs.rs](https://docs.rs/bevy_fsm/badge.svg)](https://docs.rs/bevy_fsm)
[![License](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue.svg)](https://github.com/MolecularSadism/bevy_fsm#license)
[![Bevy](https://img.shields.io/badge/Bevy-0.20-blue.svg)](https://bevyengine.org/)
[![Rust](https://img.shields.io/badge/rust-2024%20edition-orange.svg)](https://www.rust-lang.org/)

Observer-driven finite state machine framework for Bevy ECS.

## Bevy Compatibility

| Bevy | bevy_fsm  |
|------|-----------|
| 0.20 | 0.6, 0.7  |
| 0.19 | 0.4, 0.5  |
| 0.18 | 0.3, 0.5  |
| 0.17 | 0.2       |
| 0.16 | 0.1       |

## Features

- **Enum-based states**: Keep your states as simple enum variants
- **Observer-driven**: React to state changes via Bevy observers
- **Variant-specific observers**: `On<Enter<life_fsm::Dying>>` runs only when `Dying` is entered
- **Enforced rules**: The state is an immutable component, so `&mut` access does not compile; transitions go through a validated request
- **Flexible validation**: Per-entity and per-type transition rules
- **Clean API**: FSMPlugin for automatic setup
- **Initial state support**: Automatic enter events when FSM components are added
- **Organized hierarchy**: Observers automatically organized in entity hierarchy

## Quick Start

```rust
use bevy::prelude::*;
use bevy_fsm::{FSMState, FSMTransition, FSMPlugin, StateChangeRequest, Enter, Exit, fsm_observer};

fn plugin(app: &mut App) {
    app.add_plugins(FSMPlugin::<LifeFSM>::default());

    fsm_observer!(app, LifeFSM, on_enter_dying);
    fsm_observer!(app, LifeFSM, on_exit_alive);
    fsm_observer!(app, LifeFSM, on_exit_dying);
}

#[derive(Component, FSMState, Reflect, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[component(immutable)]
#[reflect(Component)]
enum LifeFSM {
    Alive,
    Dying,
    Dead,
}

impl FSMTransition for LifeFSM {
    fn can_transition(from: Self, to: Self) -> bool {
        matches!((from, to),
            (LifeFSM::Alive, LifeFSM::Dying) |
            (LifeFSM::Dying, LifeFSM::Alive) |
            (LifeFSM::Dying, LifeFSM::Dead)) || from == to
    }
}

#[derive(Component)]
struct DyingAnimation;

fn on_enter_dying(trigger: On<Enter<life_fsm::Dying>>, mut commands: Commands) {
    commands.entity(trigger.entity).insert(DyingAnimation);
}

fn on_exit_alive(trigger: On<Exit<life_fsm::Alive>>) {
    println!("Entity {} was unalived.", trigger.entity);
}

// Exit carries the destination, Enter the origin.
fn on_exit_dying(trigger: On<Exit<life_fsm::Dying>>) {
    if trigger.to == LifeFSM::Dead {
        println!("Entity {} has died.", trigger.entity);
    }
}

fn kill(commands: &mut Commands, entity: Entity) {
    commands.queue(StateChangeRequest { entity, next: LifeFSM::Dying });
}
```

## Core Concepts

### FSMTransition Trait

Implement this trait to define which state transitions are valid:

```rust
impl FSMTransition for MyFSM {
    fn can_transition(from: Self, to: Self) -> bool {
        matches!((from, to),
            (MyFSM::StateA, MyFSM::StateB) |
            (MyFSM::StateB, MyFSM::StateC)) || from == to
    }

    // Optional: context-aware validation with world access
    fn can_transition_ctx(world: &World, entity: Entity, from: Self, to: Self) -> bool {
        if !Self::can_transition(from, to) {
            return false;
        }
        world.get::<SomeComponent>(entity).is_some()
    }
}
```

### FSMState and FSMTransition Derives

Two derive macros do all the work:

- **`#[derive(FSMState)]`** - Generates a `snake_case` module of per-variant marker
  types (e.g. `block_fsm::Tile`). A marker names its variant in observer signatures:
  `On<Enter<block_fsm::Tile>>` runs only when `Tile` is entered. Markers are
  zero-sized components that cannot be constructed; bevy_fsm uses their component ids
  to route each event straight to the observers of that variant. The enum must be
  `#[component(immutable)]`, which the derive enforces at compile time.
- **`#[derive(FSMTransition)]`** - Provides "allow all" transition behavior. Skip it
  and implement `FSMTransition` manually for custom rules.

```rust
use bevy::prelude::*;
use bevy_fsm::{FSMState, FSMTransition, Enter, Exit};

#[derive(Component, FSMTransition, FSMState, Reflect, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[component(immutable)]
#[reflect(Component)]
enum BlockFSM {
    Tile,
    Loose,
    Disabled,
}

fn on_tile_enter(enter: On<Enter<block_fsm::Tile>>, /* ... */) { }
fn on_tile_exit(exit: On<Exit<block_fsm::Tile>>, /* ... */) { }
```

### Changing State

The state component is immutable, so `&mut` access does not compile. Change it
with one of two commands:

- **`StateChangeRequest { entity, next }`** - Validates the transition, fires `Exit`,
  writes the new state, then fires `Enter`. Queue it from any system with
  `commands.queue(...)`; rejected requests are dropped. In exclusive code,
  `request.try_apply(world)` returns `Ok(previous)` or a `TransitionError`
  (`NoState`, `Unchanged`, `Denied`, `Exiting`).
  Request a follow-up transition from an `Enter` observer: `Enter` → logic → next
  state chains as expected. A request for the same entity and FSM made from an
  `Exit` observer is rejected with a warning, since that entity is already
  mid-transition.
- **`ResetState { entity, state }`** - Overwrites the state without validation and
  without events, for re-initialising pooled or recycled entities.

Inserting the component directly (`entity.insert(BlockFSM::Loose)`) still compiles
and bypasses the transition rules and events; nothing detects it.

```rust
use bevy_fsm::{ResetState, StateChangeRequest};

fn loosen(commands: &mut Commands, entity: Entity) {
    commands.queue(StateChangeRequest { entity, next: BlockFSM::Loose });
}

fn recycle(commands: &mut Commands, entity: Entity) {
    commands.queue(ResetState { entity, state: BlockFSM::Tile });
}
```

### FSMPlugin - Automatic Setup

```rust
use bevy_fsm::FSMPlugin;

fn plugin(app: &mut App) {
    app.add_plugins(FSMPlugin::<MyFSM>::default());

    // Optional: Skip automatic on_fsm_added observer
    app.add_plugins(FSMPlugin::<MyFSM>::new().ignore_fsm_addition());
}
```

### fsm_observer! Macro

Register variant-specific observers with automatic hierarchy organization:

```rust
use bevy_fsm::{fsm_observer, Enter};

fn on_enter_loose(trigger: On<Enter<block_fsm::Loose>>, mut commands: Commands) {
    commands.entity(trigger.entity).insert(RigidBody::Dynamic);
}

fn plugin(app: &mut App) {
    app.add_plugins(FSMPlugin::<BlockFSM>::default());
    fsm_observer!(app, BlockFSM, on_enter_loose);
    fsm_observer!(app, BlockFSM, on_exit_loose);
}
```

### Manual Observer Registration

`fsm_observer!` only adds the hierarchy; a plain observer works the same way:

```rust
use bevy_fsm::on_fsm_added;

app.world_mut().add_observer(on_fsm_added::<MyFSM>);
app.world_mut().add_observer(on_enter_loose);
```

### Generic Event Observers

Key `Enter`/`Exit` by the FSM itself to observe every variant:

```rust
fn on_any_enter(trigger: On<Enter<BlockFSM>>) {
    match trigger.state {
        BlockFSM::Tile => { /* handle tile */ },
        BlockFSM::Loose => { /* handle loose */ },
        _ => {}
    }
}
```

## Advanced Features

### Per-Entity Configuration with Priority Model

FSMOverride allows per-entity transition control with a **priority-based system**.

#### Priority: Config Wins, Rules Fill Gaps

- **Whitelist**: Transitions ON the list are **immediately accepted**
- **Blacklist**: Transitions ON the list are **immediately denied**
- Transitions NOT decided by config use `FSMTransition` rules (if `with_rules()`)

```rust
use bevy_fsm::FSMOverride;

// Force allow specific transition
commands.entity(special_npc).insert((
    AnimationState::Idling,
    FSMOverride::whitelist([
        (AnimationState::Idling, AnimationState::Flying),
    ]),
));

// Whitelist + fallback to FSMTransition
commands.entity(npc).insert((
    AnimationState::Idling,
    FSMOverride::whitelist([
        (AnimationState::Idling, AnimationState::Flying),
    ]).with_rules(),
));

// Force deny specific transition
commands.entity(injured_npc).insert((
    AnimationState::Idling,
    FSMOverride::blacklist([
        (AnimationState::Idling, AnimationState::Running),
    ]),
));
```

#### FSMOverride Modes

- **`whitelist([...])`**: Only listed transitions pass immediately
- **`blacklist([...])`**: Listed transitions denied immediately
- **`allow_all()`**: All transitions pass (bypass FSMTransition unless `with_rules()`)
- **`deny_all()`**: All transitions denied (frozen state)

### Context-Aware Validation

Use world state in transition validation:

```rust
impl FSMTransition for AnimationState {
    fn can_transition_ctx(world: &World, entity: Entity, from: Self, to: Self) -> bool {
        if !Self::can_transition(from, to) {
            return false;
        }
        if let Some(animation) = world.get::<SpriteAnimation>(entity) {
            animation.has_state(to)
        } else {
            false
        }
    }
}
```

## Event Types

`On<Enter<K>>` and `On<Exit<K>>` deref to these `EntityEvent`s, where `K` is the FSM
(every variant) or one of its variant markers (that variant only):

- `EnterEvent<S>`: `entity`, `state`, `from` (`None` for the initial state)
- `ExitEvent<S>`: `entity`, `state`, `to`

Access the entity via `trigger.entity` (using Deref).

## How It Works

When a `StateChangeRequest` is applied:

1. The transition is validated (`FSMOverride`, then `FSMTransition`)
2. `Exit` fires for the current state; commands its observers queue are applied
3. The state component is written
4. `Enter` fires for the new state

Each event fires once, keyed by the variant's marker component, so only the observers
of that variant and the FSM-wide observers run.

When an FSM component is first added:

1. `on_fsm_added` observer detects the new component
2. `Enter` fires for the initial state with `from: None`

## Important: Timing of Initial Enter Events

When an FSM component is added during entity spawn, the initial `Enter` event fires **in the same frame**, before the entity is fully initialized.

```rust
let entity = commands.spawn((
    LifeFSM::Alive,  // Enter event fires immediately!
    Health::new(100),
)).id();
```

**Consider using `ignore_fsm_addition()`** if you don't need initial Enter events:

```rust
app.add_plugins(FSMPlugin::<LifeFSM>::new().ignore_fsm_addition());
```

## Testing

```rust
use bevy_fsm::{FSMPlugin, StateChangeRequest};

#[test]
fn test_state_transition() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.add_plugins(FSMPlugin::<LifeFSM>::default());

    let entity = app.world_mut().spawn(LifeFSM::Alive).id();
    app.update();

    app.world_mut().commands().queue(
        StateChangeRequest { entity, next: LifeFSM::Dying },
    );
    app.update();

    assert_eq!(*app.world().get::<LifeFSM>(entity).unwrap(), LifeFSM::Dying);
}
```

## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or <http://www.apache.org/licenses/LICENSE-2.0>)
- MIT License ([LICENSE-MIT](LICENSE-MIT) or <http://opensource.org/licenses/MIT>)

at your option.

### Contribution

Contributions are welcome! This crate is part of the [MolecularSadism](https://github.com/MolecularSadism) game development libraries.
