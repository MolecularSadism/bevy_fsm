//! Minimal example showing the simplest possible FSM setup.
//!
//! This demonstrates:
//! - Zero boilerplate FSM with default "allow all transitions" behavior
//! - Just three derives: #[derive(EnumEvent, `FSMTransition`, `FSMState`)]
//! - No manual `FSMTransition` implementation needed!
//! - Using `fsm_observer`! macro to register observers in the FSM hierarchy
//!
//! Run with: cargo run --example simple

use bevy::prelude::*;
use bevy_fsm::{
    fsm_observer, Enter, EnumEvent, Exit, FSMPlugin, FSMState, FSMTransition, StateChangeRequest,
};

fn main() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_plugins(FSMPlugin::<GameState>::default());

    // Use fsm_observer! macro to register observers in the FSM hierarchy
    fsm_observer!(app, GameState, on_enter_playing);
    fsm_observer!(app, GameState, on_exit_playing);

    app.add_systems(Startup, setup)
        .add_systems(Update, cycle_states)
        .run();
}

/// Define a simple game state FSM.
///
/// The `FSMTransition` derive gives us "allow all transitions" by default.
#[derive(
    Component, EnumEvent, FSMTransition, FSMState, Reflect, Clone, Copy, Debug, PartialEq, Eq, Hash,
)]
#[reflect(Component)]
enum GameState {
    MainMenu,
    Playing,
    Paused,
    GameOver,
}

// No manual impl needed - FSMTransition derive allows all transitions.

/// Setup the game
fn setup(mut commands: Commands) {
    println!("=== Simple FSM Example ===");
    println!("This FSM allows ALL transitions by default");
    println!("Observers registered using fsm_observer! macro\n");

    commands.spawn((GameState::MainMenu, Name::new("Game")));
}

/// Cycle through states to demonstrate the FSM
#[allow(
    clippy::needless_pass_by_value,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
fn cycle_states(
    mut commands: Commands,
    query: Query<(Entity, &GameState, &Name)>,
    time: Res<Time>,
    mut elapsed: Local<f32>,
    mut last_transition: Local<u32>,
) {
    *elapsed += time.delta_secs();
    let current_step = (*elapsed * 2.0) as u32;

    // Only trigger transitions when we reach a new step
    if current_step != *last_transition {
        *last_transition = current_step;

        for (entity, &state, name) in query.iter() {
            let next_state = match current_step {
                2 | 6 => Some(GameState::Playing),
                4 => Some(GameState::Paused),
                8 => Some(GameState::GameOver),
                10 => Some(GameState::MainMenu), // Can go back to menu from game over!
                12 => {
                    println!("\n=== Example complete! ===");
                    std::process::exit(0);
                }
                _ => None,
            };

            if let Some(next) = next_state {
                println!("\n{name} transitioning: {state:?} -> {next:?}");
                commands.trigger(StateChangeRequest { entity, next });
            }
        }
    }
}

/// Observer: fires when entering Playing state
fn on_enter_playing(_trigger: On<Enter<game_state::Playing>>) {
    println!("  [ENTER Playing] Game started!");
}

/// Observer: fires when exiting Playing state
fn on_exit_playing(_trigger: On<Exit<game_state::Playing>>) {
    println!("  [EXIT Playing] Game stopped!");
}
