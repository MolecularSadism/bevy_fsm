# bevy_fsm

[![CI](https://github.com/MolecularSadism/bevy_fsm/workflows/CI/badge.svg)](https://github.com/MolecularSadism/bevy_fsm/actions)
[![Crates.io](https://img.shields.io/crates/v/bevy_fsm.svg)](https://crates.io/crates/bevy_fsm)
[![Docs.rs](https://docs.rs/bevy_fsm/badge.svg)](https://docs.rs/bevy_fsm)
[![License](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue.svg)](https://github.com/MolecularSadism/bevy_fsm#license)
[![Bevy](https://img.shields.io/badge/Bevy-0.18%20%E2%80%93%200.19-blue.svg)](https://bevyengine.org/)
[![Rust](https://img.shields.io/badge/rust-2024%20edition-orange.svg)](https://www.rust-lang.org/)

Observer-driven finite state machine framework for Bevy ECS.

## Bevy Compatibility

| Bevy | bevy_fsm  |
|------|-----------|
| 0.19 | 0.4, 0.5  |
| 0.18 | 0.3, 0.5  |
| 0.17 | 0.2       |
| 0.16 | 0.1       |

## Features

- **Enum-based states**: Keep your states as simple enum variants
- **Observer-driven**: React to state changes via Bevy observers
- **Variant-specific events**: No runtime state checks needed in observers
- **Flexible validation**: Per-entity and per-type transition rules
- **Clean API**: FSMPlugin for automatic setup
- **Initial state support**: Automatic enter events when FSM components are added
- **Organized hierarchy**: Observers automatically organized in entity hierarchy

## Quick Start

```rust
use bevy::prelude::*;
use bevy_fsm::{FSMState, FSMTransition, FSMPlugin, StateChangeRequest, Enter, Exit, Transition, fsm_observer};

fn plugin(app: &mut App) {
    app.add_plugins(FSMPlugin::<LifeFSM>::default());

    fsm_observer!(app, LifeFSM, on_enter_dying);
    fsm_observer!(app, LifeFSM, on_exit_alive);
    fsm_observer!(app, LifeFSM, on_transition_dying_dead);
}

#[derive(Component, FSMState, Reflect, Clone, Copy, Debug, PartialEq, Eq, Hash)]
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

fn on_transition_dying_dead(trigger: On<Transition<life_fsm::Dying, life_fsm::Dead>>) {
    println!("Entity {} has died.", trigger.entity);
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
  types (e.g. `block_fsm::Tile`) plus the FSM trigger methods. The markers are the
  type parameters you plug into `Enter<T>` / `Exit<T>` / `Transition<F, T>`. They are
  deliberately *not* standalone Bevy events, so a variant can never be triggered on
  its own and bypass the state machine.
- **`#[derive(FSMTransition)]`** - Provides "allow all" transition behavior. Skip it
  and implement `FSMTransition` manually for custom rules.

```rust
use bevy::prelude::*;
use bevy_fsm::{FSMState, FSMTransition, Enter, Exit};

#[derive(Component, FSMTransition, FSMState, Reflect, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[reflect(Component)]
enum BlockFSM {
    Tile,
    Loose,
    Disabled,
}

fn on_tile_enter(enter: On<Enter<block_fsm::Tile>>, /* ... */) { }
fn on_tile_exit(exit: On<Exit<block_fsm::Tile>>, /* ... */) { }
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

```rust
use bevy_fsm::{apply_state_request, on_fsm_added};

app.world_mut().add_observer(apply_state_request::<MyFSM>);
app.world_mut().add_observer(on_fsm_added::<MyFSM>);
app.world_mut().add_observer(on_enter_loose);
```

### Generic Event Observers

Observe generic events for runtime state checking:

```rust
fn on_any_enter(trigger: On<Enter<BlockFSM>>, mut commands: Commands) {
    let state = trigger.event().state;
    match state {
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
- **`deny_all()`**: All transitions denied (immutable state)

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

All transition events implement `EntityEvent` and contain an `entity` field:

- `StateChangeRequest<S>`: Request to change state (`entity`, `next`)
- `Enter<S>`: Enter event (`entity`, `state`)
- `Exit<S>`: Exit event (`entity`, `state`)
- `Transition<S, S>`: Transition event (`entity`, `from`, `to`)

Access the entity via `trigger.entity` (using Deref).

## How It Works

When a state change is requested:

1. `apply_state_request` observer validates the transition
2. Exit events are triggered
3. Transition event is triggered
4. State component is updated
5. Enter events are triggered

When an FSM component is first added:

1. `on_fsm_added` observer detects the new component
2. Enter events are triggered for the initial state

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

    app.world_mut().commands().trigger(
        StateChangeRequest::<LifeFSM> { entity, next: LifeFSM::Dying },
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
