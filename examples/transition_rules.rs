//! Transition rules example using the same `GameState` enum as `fully_connected.rs`.
//!
//! This demonstrates:
//! - Custom transition rules via manual `FSMTransition` implementation
//! - Blocking specific transitions in an otherwise connected graph
//! - Logging which transitions succeed or get rejected
//! - Using `fsm_observer!` to register enter observers
//!
//! Blocked transitions in this example:
//!   `MainMenu` -> Paused
//!   Playing -> `MainMenu`
//!   `GameOver` -> Playing
//!   `GameOver` -> Paused
//!
//! Run with: `cargo run --example transition_rules`

use bevy::prelude::*;
use bevy_fsm::{
    fsm_observer, Enter, EnumEvent, FSMPlugin, FSMState, FSMTransition, StateChangeRequest,
};

fn main() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_plugins(FSMPlugin::<GameState>::default());

    // Register observers for state entry events
    fsm_observer!(app, GameState, on_enter_main_menu);
    fsm_observer!(app, GameState, on_enter_playing);
    fsm_observer!(app, GameState, on_enter_paused);
    fsm_observer!(app, GameState, on_enter_game_over);

    app.add_systems(Startup, setup)
        .add_systems(Update, drive_state_transitions)
        .run();
}

/// Game states shared with the `fully_connected` example.
#[derive(Component, EnumEvent, FSMState, Reflect, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[reflect(Component)]
enum GameState {
    MainMenu,
    Playing,
    Paused,
    GameOver,
}

/// Custom transition rules that block selected transitions.
///
/// All transitions are allowed except the four blocked pairs described above.
impl FSMTransition for GameState {
    fn can_transition(from: Self, to: Self) -> bool {
        if matches!(
            (from, to),
            (GameState::MainMenu | GameState::GameOver, GameState::Paused)
                | (GameState::Playing, GameState::MainMenu)
                | (GameState::GameOver, GameState::Playing)
        ) {
            return false;
        }

        true
    }
}

/// Spawn an entity in the starting state.
fn setup(mut commands: Commands) {
    println!("=== Transition Rules Example ===");
    println!("Blocked transitions: MainMenu -> Paused, Playing -> MainMenu, GameOver -> Playing, GameOver -> Paused\n");

    commands.spawn((GameState::MainMenu, Name::new("Game")));
}

/// Drive a scripted sequence of transition attempts to demonstrate the rules.
#[allow(clippy::match_same_arms)]
fn drive_state_transitions(
    mut commands: Commands,
    query: Query<(Entity, &GameState, &Name)>,
    mut frame: Local<u32>,
) {
    *frame += 1;

    for (entity, &state, name) in &query {
        let frame = *frame;
        let name = name.as_str();
        println!("Frame {frame:02}: {name} currently in {state:?}");

        let next = match frame {
            1 => Some(GameState::Paused),   // Blocked (MainMenu -> Paused)
            2 => Some(GameState::Playing),  // Allowed (MainMenu -> Playing)
            3 => Some(GameState::MainMenu), // Blocked (Playing -> MainMenu)
            4 => Some(GameState::Paused),   // Allowed (Playing -> Paused)
            5 => Some(GameState::GameOver), // Allowed (Paused -> GameOver)
            6 => Some(GameState::Playing),  // Blocked (GameOver -> Playing)
            7 => Some(GameState::Paused),   // Blocked (GameOver -> Paused)
            8 => Some(GameState::MainMenu), // Allowed (GameOver -> MainMenu)
            9 => {
                println!("=== Example complete! ===");
                std::process::exit(0);
            }
            _ => None,
        };

        if let Some(target) = next {
            println!("  Attempting transition: {state:?} -> {target:?}");
            let allowed = <GameState as FSMTransition>::can_transition(state, target);
            println!(
                "  {}xpecting transition",
                if allowed { "E" } else { "NOT e" }
            );
            commands.trigger(StateChangeRequest {
                entity,
                next: target,
            });
        }
    }
}

fn on_enter_main_menu(_trigger: On<Enter<game_state::MainMenu>>) {
    println!("  [ENTER MainMenu] Showing title screen");
}

fn on_enter_playing(_trigger: On<Enter<game_state::Playing>>) {
    println!("  [ENTER Playing] Gameplay started");
}

fn on_enter_paused(_trigger: On<Enter<game_state::Paused>>) {
    println!("  [ENTER Paused] Game paused");
}

fn on_enter_game_over(_trigger: On<Enter<game_state::GameOver>>) {
    println!("  [ENTER GameOver] Game over screen displayed");
}
