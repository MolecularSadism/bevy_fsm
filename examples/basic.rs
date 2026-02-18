//! Basic example demonstrating advanced `bevy_fsm` features.
//!
//! This example shows:
//! - Custom transition rules via manual `FSMTransition` implementation
//! - Multiple rapid transitions in a single frame
//! - Component management during state changes (adding/removing components)
//! - Using `fsm_observer`! macro to register observers in the FSM hierarchy
//! - Handling Enter, Exit, and Transition events
//!
//! Unlike simple.rs which uses zero boilerplate, this demonstrates custom transition logic
//! and more complex state management patterns.
//!
//! Run with: cargo run --example basic

use bevy::prelude::*;
use bevy_fsm::{
    fsm_observer, Enter, EnumEvent, Exit, FSMPlugin, FSMState, FSMTransition, StateChangeRequest,
    Transition,
};

fn main() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_plugins(FSMPlugin::<LifeFSM>::default());

    // Use fsm_observer! macro to register observers in the FSM hierarchy
    fsm_observer!(app, LifeFSM, on_enter_dying);
    fsm_observer!(app, LifeFSM, on_exit_alive);
    fsm_observer!(app, LifeFSM, on_transition_dying_dead);
    fsm_observer!(app, LifeFSM, on_transition_dying_alive);

    app.add_systems(Startup, setup)
        .add_systems(Update, trigger_transitions)
        .run();
}

/// Define the Life FSM with three states
#[derive(Component, EnumEvent, FSMState, Reflect, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[reflect(Component)]
enum LifeFSM {
    Alive,
    Dying,
    Dead,
}

/// Define transition rules for the Life FSM
impl FSMTransition for LifeFSM {
    fn can_transition(from: Self, to: Self) -> bool {
        matches!(
            (from, to),
            (LifeFSM::Alive, LifeFSM::Dying) | (LifeFSM::Dying, LifeFSM::Alive | LifeFSM::Dead)
        ) || from == to
    }
}

/// Component to track when an entity is dying
#[derive(Component, Debug)]
struct DyingAnimation {
    #[allow(dead_code)]
    timer: f32,
}

/// Component to identify our test entity
#[derive(Component)]
struct TestEntity;

/// Setup the example world with one entity
fn setup(mut commands: Commands) {
    println!("=== Setting up Life FSM Example ===");
    println!("Observers registered using fsm_observer! macro");

    // Spawn an entity with initial Alive state
    let entity = commands
        .spawn((TestEntity, LifeFSM::Alive, Name::new("Hero")))
        .id();

    println!("Spawned entity {entity:?} in Alive state");
}

/// System to trigger state transitions for demonstration
#[allow(clippy::too_many_arguments, clippy::needless_pass_by_value)]
fn trigger_transitions(
    mut commands: Commands,
    query: Query<(Entity, &LifeFSM, &Name), With<TestEntity>>,
    time: Res<Time>,
    mut elapsed: Local<f32>,
    mut triggered_10: Local<bool>,
    mut triggered_20: Local<bool>,
    mut triggered_30: Local<bool>,
    mut triggered_40: Local<bool>,
    mut triggered_50: Local<bool>,
) {
    *elapsed += time.delta_secs();

    for (entity, &state, name) in query.iter() {
        // At ~1 second: Alive -> Dying
        if *elapsed >= 1.0 && !*triggered_10 {
            *triggered_10 = true;
            println!("\n--- Triggering transition: {name} Alive -> Dying ---");
            commands.trigger(StateChangeRequest {
                entity,
                next: LifeFSM::Dying,
            });
        }

        // At ~2 seconds: Dying -> Alive (resurrection)
        if *elapsed >= 2.0 && !*triggered_20 && state == LifeFSM::Dying {
            *triggered_20 = true;
            println!("\n--- Triggering transition: {name} Dying -> Alive (Resurrection!) ---");
            commands.trigger(StateChangeRequest {
                entity,
                next: LifeFSM::Alive,
            });
        }

        // At ~3 seconds: Alive -> Dying again
        if *elapsed >= 3.0 && !*triggered_30 && state == LifeFSM::Alive {
            *triggered_30 = true;
            println!("\n--- Triggering transition: {name} Alive -> Dying ---");
            commands.trigger(StateChangeRequest {
                entity,
                next: LifeFSM::Dying,
            });
        }

        // At ~4 seconds: Dying -> Dead
        if *elapsed >= 4.0 && !*triggered_40 && state == LifeFSM::Dying {
            *triggered_40 = true;
            println!("\n--- Triggering transition: {name} Dying -> Dead ---");
            commands.trigger(StateChangeRequest {
                entity,
                next: LifeFSM::Dead,
            });
        }

        // At ~5 seconds: Exit
        if *elapsed >= 5.0 && !*triggered_50 {
            *triggered_50 = true;
            println!("\n=== Example complete! ===");
            std::process::exit(0);
        }
    }
}

/// Observer: Fires when entering the Dying state
#[allow(clippy::needless_pass_by_value)]
fn on_enter_dying(trigger: On<Enter<life_fsm::Dying>>, mut commands: Commands) {
    let entity = trigger.entity;
    println!("  [ENTER Dying] Entity {entity:?} is now dying!");

    // Add a DyingAnimation component when entering Dying state
    commands
        .entity(entity)
        .insert(DyingAnimation { timer: 3.0 });
}

/// Observer: Fires when exiting the Alive state
#[allow(clippy::needless_pass_by_value)]
fn on_exit_alive(trigger: On<Exit<life_fsm::Alive>>, query: Query<&Name>) {
    let entity = trigger.entity;
    let name = query.get(entity).map(Name::as_str).unwrap_or("Unknown");
    println!("  [EXIT Alive] Entity {name} ({entity:?}) is no longer alive!");
}

/// Observer: Fires on Dying -> Dead transition
#[allow(clippy::needless_pass_by_value)]
fn on_transition_dying_dead(
    trigger: On<Transition<life_fsm::Dying, life_fsm::Dead>>,
    mut commands: Commands,
    query: Query<&Name>,
) {
    let entity = trigger.entity;
    let name = query.get(entity).map(Name::as_str).unwrap_or("Unknown");
    println!("  [TRANSITION Dying -> Dead] {name} ({entity:?}) has died. Removing DyingAnimation...");

    // Remove the DyingAnimation component
    commands.entity(entity).remove::<DyingAnimation>();
}

/// Observer: Fires on Dying -> Alive transition (resurrection)
#[allow(clippy::needless_pass_by_value)]
fn on_transition_dying_alive(
    trigger: On<Transition<life_fsm::Dying, life_fsm::Alive>>,
    mut commands: Commands,
    query: Query<&Name>,
) {
    let entity = trigger.entity;
    let name = query.get(entity).map(Name::as_str).unwrap_or("Unknown");
    println!("  [TRANSITION Dying -> Alive] {name} ({entity:?}) has been resurrected!");

    // Remove the DyingAnimation component since they're no longer dying
    commands.entity(entity).remove::<DyingAnimation>();
}
