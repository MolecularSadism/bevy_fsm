//! Observer-driven finite state machine framework for Bevy ECS.
//!
//! This crate provides a lightweight framework for modeling enum-based state machines
//! using Bevy's observer system. State machines are immutable enum components; every
//! state change goes through a validated [`StateChangeRequest`] that fires [`Enter`] and
//! [`Exit`] events, either for one variant or for the whole FSM.
//!
//! # Features
//!
//! - **Enum-based states**: Keep your states as simple enum variants
//! - **Observer-driven**: React to state changes via Bevy observers
//! - **Variant-specific observers**: `On<Enter<life_fsm::Dying>>` runs only on entering `Dying`
//! - **Enforced rules**: The state component is immutable, so `&mut` access does not
//!   compile; transitions go through a validated request
//! - **Flexible validation**: Per-entity and per-type transition rules
//! - **Minimal API**: `FSMPlugin` for automatic setup
//! - **Organized hierarchy**: Observers automatically organized in entity hierarchy
//!
//! # Quick Start
//!
//! ```rust
//! use bevy::prelude::*;
//! use bevy_fsm::{FSMState, FSMTransition, FSMPlugin, StateChangeRequest, Enter, Exit, fsm_observer};
//!
//! fn plugin(app: &mut App) {
//!     // FSMPlugin automatically sets up the observer hierarchy on first use
//!     app.add_plugins(FSMPlugin::<LifeFSM>::default());
//!
//!     // fsm_observer! registers a plain observer and parents it into the
//!     // FSMObservers/LifeFSM/on_enter_dying hierarchy, keeping observers sorted by FSM.
//!     fsm_observer!(app, LifeFSM, on_enter_dying);
//!     fsm_observer!(app, LifeFSM, on_exit_alive);
//!     fsm_observer!(app, LifeFSM, on_exit_dying);
//! }
//!
//! // Zero boilerplate - just derive FSMTransition for "allow all" behavior!
//! #[derive(Component, FSMTransition, FSMState, Reflect, Clone, Copy, Debug, PartialEq, Eq, Hash)]
//! #[component(immutable)]
//! #[reflect(Component)]
//! enum LifeFSM {
//!     Alive,
//!     Dying,
//!     Dead,
//! }
//!
//! // For custom transition rules, skip FSMTransition derive and manually implement:
//! // impl FSMTransition for LifeFSM {
//! //     fn can_transition(from: Self, to: Self) -> bool {
//! //         matches!((from, to),
//! //             (LifeFSM::Alive, LifeFSM::Dying) |
//! //             (LifeFSM::Dying, LifeFSM::Dead)) || from == to
//! //     }
//! // }
//!
//! #[derive(Component)]
//! struct DyingAnimation;
//!
//! fn on_enter_dying(trigger: On<Enter<life_fsm::Dying>>, mut commands: Commands) {
//!     commands.entity(trigger.entity).insert(DyingAnimation);
//! }
//!
//! fn on_exit_alive(trigger: On<Exit<life_fsm::Alive>>) {
//!     println!("Entity {} was unalived.", trigger.entity);
//! }
//!
//! // Exit carries the destination, Enter the origin.
//! fn on_exit_dying(trigger: On<Exit<life_fsm::Dying>>, mut commands: Commands) {
//!     if trigger.to == LifeFSM::Dead {
//!         commands.entity(trigger.entity).despawn();
//!     }
//! }
//!
//! // Request a transition from any system:
//! fn kill(mut commands: Commands, q: Query<Entity, With<LifeFSM>>) {
//!     for entity in &q {
//!         commands.queue(StateChangeRequest { entity, next: LifeFSM::Dying });
//!     }
//! }
//! ```
//!
//! # Context-Aware Transitions
//!
//! For more complex validation logic that requires access to the World (e.g., checking other components),
//! override `can_transition_ctx`:
//!
//! ```rust
//! use bevy::prelude::*;
//! use bevy_fsm::{FSMState, FSMTransition};
//!
//! #[derive(Component)]
//! struct Energy(f32);
//!
//! #[derive(Component, FSMState, Clone, Copy, Debug, Hash, PartialEq, Eq)]
//! #[component(immutable)]
//! enum ActionFSM {
//!     Idle,
//!     Casting,
//!     Exhausted,
//! }
//!
//! impl FSMTransition for ActionFSM {
//!     fn can_transition(from: Self, to: Self) -> bool {
//!         // Basic state-only rules
//!         matches!((from, to),
//!             (ActionFSM::Idle, ActionFSM::Casting) |
//!             (ActionFSM::Casting, ActionFSM::Idle) |
//!             (ActionFSM::Casting, ActionFSM::Exhausted) |
//!             (ActionFSM::Exhausted, ActionFSM::Idle)) || from == to
//!     }
//!
//!     fn can_transition_ctx(world: &World, entity: Entity, from: Self, to: Self) -> bool {
//!         // First check basic rules
//!         if !<Self as FSMTransition>::can_transition(from, to) {
//!             return false;
//!         }
//!
//!         // Additional context-aware validation
//!         if matches!(to, ActionFSM::Casting) {
//!             // Only allow casting if entity has enough energy
//!             if let Some(energy) = world.get::<Energy>(entity) {
//!                 return energy.0 >= 10.0;
//!             }
//!             return false;
//!         }
//!
//!         true
//!     }
//! }
//! ```
//!
//! # Observer Hierarchy
//!
//! The first `FSMPlugin` added to your app automatically creates a hierarchical
//! organization for all FSM observers:
//!
//! ```text
//! FSMObservers (root)
//! ├─ LifeFSM
//! │  ├─ on_fsm_added
//! │  ├─ on_dying
//! │  └─ on_dead
//! ├─ BlockFSM
//! │  ├─ on_fsm_added
//! │  └─ ...
//! └─ ...
//! ```
//!
//! This hierarchy is created automatically when you add your first FSM plugin,
//! with no additional setup required.

// Lets the derive macros' `::bevy_fsm` paths resolve inside this crate's own tests.
extern crate self as bevy_fsm;

use bevy::prelude::*;
use bevy::{
    ecs::{
        component::{ComponentId, Immutable},
        event::{EntityComponentsTrigger, EventPattern},
        world::WorldId,
    },
    platform::collections::{HashMap, HashSet},
    reflect::GetTypeRegistration,
};
// Re-export FSM derives from bevy_fsm_macros.
// Note: FSMState and FSMTransition are both traits (below) and derive macros (from bevy_fsm_macros)
pub use bevy_fsm_macros::{FSMState, FSMTransition};
use core::marker::PhantomData;
use std::any::TypeId;

/// Paths used by the code `#[derive(FSMState)]` generates.
#[doc(hidden)]
pub mod __private {
    pub use bevy::ecs::component::{Component, ComponentId, Immutable, StorageType};
    pub use bevy::ecs::world::World;
}

/// Macro for registering FSM observers sorting them into the per-FSM hierarchy.
///
/// Observers registered with this macro will be organized under:
/// `FSMObservers` -> {`FSMName`} -> observer
///
/// Uses the same naming convention as `global_observer!` for consistency.
///
/// # Example
/// ```
/// # use bevy::prelude::*;
/// # use bevy_fsm::{FSMState, FSMTransition, fsm_observer, Enter};
/// # #[derive(Component, FSMTransition, FSMState, Reflect, Clone, Copy, Debug, PartialEq, Eq, Hash)]
/// # #[component(immutable)]
/// # enum LifeFSM { Alive, Dying }
/// # fn on_dying_observer(_: On<Enter<life_fsm::Dying>>) {}
/// # let mut app = App::new();
/// fsm_observer!(app, LifeFSM, on_dying_observer);
/// ```
#[macro_export]
macro_rules! fsm_observer {
    ($app:expr, $fsm_type:ty, $system:expr) => {{
        let mut world = $app.world_mut();
        let entity = {
            let mut observer = world.add_observer($system);
            observer.insert(bevy::prelude::Name::new(stringify!($system)));
            observer.insert($crate::FSMObserverMarker::<$fsm_type>::default());
            observer.id()
        };
        $crate::attach_observer_to_group::<$fsm_type>(&mut world, entity);
        world.entity_mut(entity)
    }};
}

/// Marker component to tag observers belonging to a specific FSM type.
///
/// This is used internally by the `fsm_observer!` macro but needs to be public
/// for the macro to work across crate boundaries.
#[derive(Component)]
#[doc(hidden)]
pub struct FSMObserverMarker<S: Send + Sync + 'static> {
    _phantom: PhantomData<S>,
}

impl<S: Send + Sync + 'static> Default for FSMObserverMarker<S> {
    fn default() -> Self {
        Self {
            _phantom: PhantomData,
        }
    }
}

/// Marker component for FSM type group entities in the observer hierarchy.
///
/// Each FSM type gets one entity marked with this component, which acts as
/// the parent for all observers of that FSM type.
///
/// Hierarchy: FSMObservers -> FSMObserverGroup<LifeFSM> -> individual observers
#[derive(Component)]
#[doc(hidden)]
pub struct FSMObserverGroup<S: Send + Sync + 'static> {
    _phantom: PhantomData<S>,
}

impl<S: Send + Sync + 'static> Default for FSMObserverGroup<S> {
    fn default() -> Self {
        Self {
            _phantom: PhantomData,
        }
    }
}

/// Anything [`Enter`] and [`Exit`] can be keyed by: an FSM enum (every variant) or one
/// of its variant markers (that variant only).
///
/// Implemented by `#[derive(FSMState)]`; there is no reason to implement it by hand.
pub trait FSMKey: Send + Sync + 'static {
    /// The FSM this key belongs to.
    type Fsm: FSMState;
    /// The component an observer of this key is registered under; `()` for the whole FSM.
    type Components: Bundle;
}

/// Event fired when an entity enters a state.
///
/// Observe it through [`Enter`]: `On<Enter<life_fsm::Dying>>` for one variant,
/// `On<Enter<LifeFSM>>` for all of them.
#[derive(EntityEvent, Debug, Clone, Copy)]
#[entity_event(trigger = EntityComponentsTrigger<'a>)]
pub struct EnterEvent<S: FSMState> {
    /// The entity whose state changed.
    pub entity: Entity,
    /// The state entered.
    pub state: S,
    /// The state left, or `None` when the FSM component was just added.
    pub from: Option<S>,
}

/// Event fired when an entity exits a state.
///
/// Observe it through [`Exit`]: `On<Exit<life_fsm::Alive>>` for one variant,
/// `On<Exit<LifeFSM>>` for all of them.
#[derive(EntityEvent, Debug, Clone, Copy)]
#[entity_event(trigger = EntityComponentsTrigger<'a>)]
pub struct ExitEvent<S: FSMState> {
    /// The entity whose state changes.
    pub entity: Entity,
    /// The state left.
    pub state: S,
    /// The state about to be entered.
    pub to: S,
}

/// Observer pattern for [`EnterEvent`], keyed by an FSM or one of its variant markers.
///
/// ```
/// # use bevy::prelude::*;
/// # use bevy_fsm::{FSMState, FSMTransition, Enter};
/// # #[derive(Component, FSMTransition, FSMState, Clone, Copy, Debug, PartialEq, Eq, Hash)]
/// # #[component(immutable)]
/// # enum LifeFSM { Alive, Dying }
/// fn on_enter_dying(trigger: On<Enter<life_fsm::Dying>>) {
///     assert_eq!(trigger.state, LifeFSM::Dying);
/// }
///
/// fn on_any_enter(trigger: On<Enter<LifeFSM>>) {
///     println!("{} entered {:?}", trigger.entity, trigger.state);
/// }
/// ```
pub struct Enter<K: FSMKey>(PhantomData<fn() -> K>);

impl<K: FSMKey> EventPattern for Enter<K> {
    type Event = EnterEvent<K::Fsm>;
    type Components = K::Components;
}

/// Observer pattern for [`ExitEvent`], keyed by an FSM or one of its variant markers.
///
/// See [`Enter`].
pub struct Exit<K: FSMKey>(PhantomData<fn() -> K>);

impl<K: FSMKey> EventPattern for Exit<K> {
    type Event = ExitEvent<K::Fsm>;
    type Components = K::Components;
}

/// Trait for defining transition logic.
///
/// Implement this trait on your FSM enum to define which transitions are valid.
///
/// # Example
/// ```rust
/// use bevy_fsm::FSMTransition;
///
/// #[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// enum LifeFSM { Alive, Dying, Dead }
///
/// impl FSMTransition for LifeFSM {
///     fn can_transition(from: Self, to: Self) -> bool {
///         matches!((from, to),
///             (LifeFSM::Alive, LifeFSM::Dying) |
///             (LifeFSM::Dying, LifeFSM::Dead)) || from == to
///     }
/// }
/// ```
pub trait FSMTransition {
    /// Returns true if transition from `from` to `to` is allowed.
    fn can_transition(from: Self, to: Self) -> bool
    where
        Self: Sized;

    /// Optional context-aware validation with world access.
    ///
    /// Defaults to calling `can_transition`.
    fn can_transition_ctx(world: &World, entity: Entity, from: Self, to: Self) -> bool
    where
        Self: Sized,
    {
        let _ = (world, entity);
        Self::can_transition(from, to)
    }
}

/// Core FSM trait, implemented by `#[derive(FSMState)]`.
///
/// The state is an immutable component: `&mut` access does not compile. Change it
/// through [`StateChangeRequest`] (validated, fires events) or [`ResetState`]; a raw
/// `insert` of the component still compiles and bypasses both the rules and the events.
///
/// ```compile_fail
/// # use bevy::prelude::*;
/// # use bevy_fsm::{FSMState, FSMTransition};
/// # #[derive(Component, FSMTransition, FSMState, Clone, Copy, Debug, PartialEq, Eq, Hash)]
/// # #[component(immutable)]
/// # enum LifeFSM { Alive, Dying }
/// fn bypass(mut q: Query<&mut LifeFSM>) {
///     for mut state in &mut q {
///         *state = LifeFSM::Dying;
///     }
/// }
/// # let mut app = App::new();
/// # app.add_systems(Update, bypass);
/// ```
pub trait FSMState:
    Component<Mutability = Immutable>
    + FSMKey<Fsm = Self>
    + FSMTransition
    + Copy
    + Eq
    + core::hash::Hash
    + core::fmt::Debug
    + Send
    + Sync
    + 'static
{
    /// Component id of this variant's marker type, the key its observers are registered under.
    #[doc(hidden)]
    fn variant_key(self, world: &mut World) -> ComponentId;
}

/// Why a [`StateChangeRequest`] was not applied.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransitionError {
    /// The entity does not exist or has no FSM component of this type.
    NoState,
    /// The entity is already in the requested state.
    Unchanged,
    /// The transition rules or the entity's [`FSMOverride`] forbid it.
    Denied,
    /// Requested by an [`Exit`] observer while the entity is leaving a state of this
    /// FSM. Request follow-up transitions from an [`Enter`] observer instead.
    Exiting,
}

impl core::fmt::Display for TransitionError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            Self::NoState => "entity has no state of this FSM",
            Self::Unchanged => "entity is already in the requested state",
            Self::Denied => "transition is not allowed",
            Self::Exiting => "transition requested while the entity is exiting a state",
        })
    }
}

impl std::error::Error for TransitionError {}

/// Command requesting a state change for an entity.
///
/// Queue it from any system with `commands.queue(StateChangeRequest { entity, next })`.
/// When applied it validates the transition, fires [`Exit`] for the current state,
/// writes the new state, then fires [`Enter`]. Rejected requests are dropped silently;
/// use [`StateChangeRequest::try_apply`] in exclusive code to get the outcome.
///
/// ```
/// # use bevy::prelude::*;
/// # use bevy_fsm::{FSMState, FSMTransition, StateChangeRequest, TransitionError};
/// # #[derive(Component, FSMState, Clone, Copy, Debug, PartialEq, Eq, Hash)]
/// # #[component(immutable)]
/// # enum LifeFSM { Alive, Dying }
/// # impl FSMTransition for LifeFSM {
/// #     fn can_transition(from: Self, to: Self) -> bool { from == LifeFSM::Alive }
/// # }
/// let mut world = World::new();
/// let entity = world.spawn(LifeFSM::Alive).id();
///
/// let request = StateChangeRequest { entity, next: LifeFSM::Dying };
/// assert_eq!(request.try_apply(&mut world), Ok(LifeFSM::Alive));
///
/// let back = StateChangeRequest { entity, next: LifeFSM::Alive };
/// assert_eq!(back.try_apply(&mut world), Err(TransitionError::Denied));
/// ```
#[derive(Debug, Clone, Copy)]
pub struct StateChangeRequest<S: FSMState> {
    /// The entity whose state should change.
    pub entity: Entity,
    /// The requested state.
    pub next: S,
}

impl<S: FSMState> StateChangeRequest<S> {
    /// Applies the request immediately, returning the state that was left.
    ///
    /// Commands queued by [`Exit`] observers apply before the new state is written,
    /// and everything queued along the way is flushed before this returns. A request
    /// for the same entity and FSM queued by an [`Exit`] observer is rejected with a
    /// warning; [`Enter`] observers may request the next transition.
    ///
    /// # Errors
    ///
    /// Returns a [`TransitionError`] and changes nothing when the entity has no state,
    /// is exiting a state, is already in `next`, or the transition is not allowed.
    pub fn try_apply(self, world: &mut World) -> Result<S, TransitionError> {
        let Self { entity, next } = self;
        let current = *world.get::<S>(entity).ok_or(TransitionError::NoState)?;
        let exit_scope = (world.id(), entity, TypeId::of::<S>());
        if is_exiting(exit_scope) {
            log::warn!(
                "{entity}: rejected {} transition to {next:?} requested while exiting \
                 {current:?}; request it from an Enter observer instead",
                core::any::type_name::<S>()
            );
            return Err(TransitionError::Exiting);
        }
        if current == next {
            return Err(TransitionError::Unchanged);
        }
        if !is_transition_allowed(world, entity, current, next) {
            return Err(TransitionError::Denied);
        }

        let exit_key = current.variant_key(world);
        let exit_guard = ExitGuard::enter(exit_scope);
        world.trigger_with(
            ExitEvent {
                entity,
                state: current,
                to: next,
            },
            keyed_trigger(&[exit_key]),
        );
        world.flush();
        drop(exit_guard);

        // An Exit observer may have despawned the entity or removed its state.
        let Ok(mut entity_mut) = world.get_entity_mut(entity) else {
            return Ok(current);
        };
        if !entity_mut.contains::<S>() {
            return Ok(current);
        }
        entity_mut.insert(next);
        world.flush();

        trigger_enter(world, entity, next, Some(current));
        world.flush();
        Ok(current)
    }
}

impl<S: FSMState> Command for StateChangeRequest<S> {
    type Out = ();

    fn apply(self, world: &mut World) {
        // A rejected request is an expected outcome, not an error.
        let _ = self.try_apply(world);
    }
}

/// Command that overwrites an entity's state without validation and without firing
/// [`Enter`] or [`Exit`].
///
/// For re-initialising pooled or recycled entities, where the old state's history
/// does not apply. Does nothing if the entity has no state of this FSM.
#[derive(Debug, Clone, Copy)]
pub struct ResetState<S: FSMState> {
    /// The entity whose state is overwritten.
    pub entity: Entity,
    /// The new state.
    pub state: S,
}

impl<S: FSMState> Command for ResetState<S> {
    type Out = ();

    fn apply(self, world: &mut World) {
        if let Ok(mut entity_mut) = world.get_entity_mut(self.entity)
            && entity_mut.contains::<S>()
        {
            entity_mut.insert(self.state);
        }
    }
}

/// World, entity and FSM type of a transition whose [`Exit`] observers are running.
type ExitScope = (WorldId, Entity, TypeId);

std::thread_local! {
    // Commands apply on one thread with exclusive world access, so a thread-local
    // list sees every Exit phase a nested request can be issued from.
    static EXITING: core::cell::RefCell<Vec<ExitScope>> =
        const { core::cell::RefCell::new(Vec::new()) };
}

fn is_exiting(scope: ExitScope) -> bool {
    EXITING.with_borrow(|exiting| exiting.contains(&scope))
}

/// Marks an Exit phase as running until dropped, including on early return or panic.
struct ExitGuard(ExitScope);

impl ExitGuard {
    fn enter(scope: ExitScope) -> Self {
        EXITING.with_borrow_mut(|exiting| exiting.push(scope));
        Self(scope)
    }
}

impl Drop for ExitGuard {
    fn drop(&mut self) {
        EXITING.with_borrow_mut(|exiting| {
            if let Some(index) = exiting.iter().rposition(|scope| *scope == self.0) {
                exiting.swap_remove(index);
            }
        });
    }
}

fn keyed_trigger(components: &[ComponentId]) -> EntityComponentsTrigger<'_> {
    EntityComponentsTrigger {
        components,
        old_archetype: None,
        new_archetype: None,
    }
}

fn trigger_enter<S: FSMState>(world: &mut World, entity: Entity, state: S, from: Option<S>) {
    let enter_key = state.variant_key(world);
    world.trigger_with(
        EnterEvent {
            entity,
            state,
            from,
        },
        keyed_trigger(&[enter_key]),
    );
}

/// Decides a transition from the entity's [`FSMOverride`], falling back to [`FSMTransition`].
fn is_transition_allowed<S: FSMState>(world: &World, entity: Entity, from: S, to: S) -> bool {
    let rules = || <S as FSMTransition>::can_transition_ctx(world, entity, from, to);
    let Some(cfg) = world.get::<FSMOverride<S>>(entity) else {
        return rules();
    };
    let listed = cfg.transitions.contains(&(from, to));
    match cfg.mode {
        RuleType::All => !cfg.call_rules || rules(),
        RuleType::None => false,
        RuleType::Whitelist => listed || (cfg.call_rules && rules()),
        RuleType::Blacklist => !listed && (!cfg.call_rules || rules()),
    }
}

/// Configuration mode for FSM transition validation set in the [`FSMOverride`] component.
///
/// The mode determines priority behavior - config wins over `FSMTransition` rules:
/// - **Whitelist**: Transitions ON the list are **immediately accepted** (override rules)
/// - **Blacklist**: Transitions ON the list are **immediately denied** (override rules)
/// - Transitions NOT decided by the config can still use `FSMTransition` rules (if `call_rules: true`)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Reflect)]
pub enum RuleType {
    /// No config restrictions - defer to `FSMTransition` rules.
    ///
    /// The `transitions` set is ignored. If `call_rules: true`, `FSMTransition` validation
    /// applies. If `call_rules: false`, all transitions are allowed.
    All,

    /// Deny all transitions (frozen state).
    ///
    /// The `transitions` set is ignored. All transitions are hard-denied.
    /// `FSMTransition` validation is never reached.
    None,

    /// Whitelist mode - listed transitions are prioritized and accepted.
    ///
    /// **Priority behavior:**
    /// - If transition IS in set → **ACCEPT** (whitelist wins, `FSMTransition` NOT checked)
    /// - If transition is NOT in set:
    ///   - With `call_rules: true` → Check `FSMTransition` (can accept or deny)
    ///   - With `call_rules: false` → **DENY** (default)
    ///
    /// Use this to explicitly allow specific transitions regardless of `FSMTransition` rules.
    Whitelist,

    /// Blacklist mode - listed transitions are prioritized and denied.
    ///
    /// **Priority behavior:**
    /// - If transition IS in set → **DENY** (blacklist wins, `FSMTransition` NOT checked)
    /// - If transition is NOT in set:
    ///   - With `call_rules: true` → Check `FSMTransition` (can accept or deny)
    ///   - With `call_rules: false` → **ACCEPT** (default)
    ///
    /// Use this to explicitly forbid specific transitions regardless of `FSMTransition` rules.
    Blacklist,
}

/// Component for optional per-entity state machine configuration.
///
/// Attach this component alongside your FSM enum to constrain transitions
/// for that specific entity.
///
/// # Priority Model: Config Wins, Rules Fill Gaps
///
/// `FSMOverride` has **priority** over `FSMTransition` rules:
///
/// ```text
/// Transition Request
///       ↓
/// Is it ON whitelist?  → YES → ACCEPT (config wins)
///       ↓ NO
/// call_rules enabled?  → YES → Check FSMTransition
///       ↓ NO
///   DENY (not whitelisted)
/// ```
///
/// ```text
/// Transition Request
///       ↓
/// Is it ON blacklist?  → YES → DENY (config wins)
///       ↓ NO
/// call_rules enabled?  → YES → Check FSMTransition
///       ↓ NO
///   ACCEPT (not blacklisted)
/// ```
///
/// # Validation Flow
///
/// 1. **`FSMOverride` (if present):**
///    - **Whitelist mode:**
///      - ON list: ACCEPT immediately
///      - NOT on list: Check `call_rules` → if true check `FSMTransition`, else DENY
///    - **Blacklist mode:**
///      - ON list: DENY immediately
///      - NOT on list: Check `call_rules` → if true check `FSMTransition`, else ACCEPT
///    - **All mode:** Check `call_rules` → if true check `FSMTransition`, else ACCEPT all
///    - **None mode:** DENY all (immutable)
///
/// 2. **No `FSMOverride`:**
///    - Falls back to `FSMTransition::can_transition_ctx` only
///
/// # Examples
///
/// ```rust
/// use bevy_fsm::{FSMOverride, RuleType};
/// # #[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
/// # enum MyState { A, B, C }
///
/// // Example 1: Whitelist A->C (overrides FSMTransition)
/// let config = FSMOverride::whitelist([
///     (MyState::A, MyState::C),  // Force allow
/// ]);
/// // A->C: ACCEPT (ON whitelist, config wins)
/// // A->B: DENY (NOT on whitelist, call_rules=false so denied)
/// // B->C: DENY (NOT on whitelist)
///
/// // Example 2: Whitelist + FSMTransition for unlisted transitions
/// let config = FSMOverride::whitelist([
///     (MyState::A, MyState::C),  // Force allow (overrides FSMTransition)
/// ]).with_rules();
/// // A->C: ACCEPT (ON whitelist, config wins, FSMTransition NOT checked)
/// // A->B: Check FSMTransition (NOT on whitelist, but call_rules=true)
/// // B->C: Check FSMTransition (NOT on whitelist, but call_rules=true)
///
/// // Example 3: Blacklist A->C (force deny)
/// let config = FSMOverride::blacklist([
///     (MyState::A, MyState::C),  // Force deny
/// ]);
/// // A->C: DENY (ON blacklist, config wins)
/// // A->B: ACCEPT (NOT on blacklist, call_rules=false so allowed)
/// // B->C: ACCEPT (NOT on blacklist)
///
/// // Example 4: Blacklist + FSMTransition for non-blacklisted
/// let config = FSMOverride::blacklist([
///     (MyState::A, MyState::C),  // Force deny
/// ]).with_rules();
/// // A->C: DENY (ON blacklist, config wins, FSMTransition NOT checked)
/// // A->B: Check FSMTransition (NOT on blacklist, but call_rules=true)
/// // B->C: Check FSMTransition (NOT on blacklist, but call_rules=true)
///
/// // Example 5: Allow all (bypass FSMTransition)
/// let config = FSMOverride::<MyState>::allow_all();
/// // All transitions: ACCEPT (no restrictions)
///
/// // Example 6: Allow all but enforce FSMTransition
/// let config = FSMOverride::<MyState>::allow_all().with_rules();
/// // All transitions: Check FSMTransition
///
/// // Example 7: Immutable state
/// let config = FSMOverride::<MyState>::deny_all();
/// // All transitions: DENY
/// ```
///
/// # Use Cases
///
/// - **Force allow specific transitions**: Use `whitelist([...])` to allow transitions
///   that `FSMTransition` would normally forbid - whitelist wins
/// - **Force deny specific transitions**: Use `blacklist([...])` to prevent transitions
///   that `FSMTransition` would normally allow - blacklist wins
/// - **Whitelist + fallback to rules**: Use `whitelist([...]).with_rules()` to allow
///   specific transitions unconditionally while checking `FSMTransition` for others
/// - **Blacklist + fallback to rules**: Use `blacklist([...]).with_rules()` to deny
///   specific transitions unconditionally while checking `FSMTransition` for others
/// - **Immutable states**: Use `deny_all()` for entities that should never change state
#[derive(Component, Reflect, Debug)]
#[reflect(Component)]
pub struct FSMOverride<S: Copy + Eq + core::hash::Hash + Send + Sync + 'static> {
    /// Transition filtering mode.
    pub mode: RuleType,
    /// Transitions set (interpretation depends on mode).
    transitions: HashSet<(S, S)>,
    /// Whether to check `FSMTransition` for transitions NOT decided by the config.
    ///
    /// - **Whitelist mode**: If `true`, transitions NOT on whitelist check `FSMTransition`.
    ///   If `false`, they are denied.
    /// - **Blacklist mode**: If `true`, transitions NOT on blacklist check `FSMTransition`.
    ///   If `false`, they are allowed.
    /// - **All mode**: If `true`, check `FSMTransition`. If `false`, allow everything.
    /// - **None mode**: Ignored (all transitions denied).
    ///
    /// **Note**: Transitions ON whitelist/blacklist are decided by config and do NOT
    /// check `FSMTransition` regardless of this flag (config has priority).
    pub call_rules: bool,
}

impl<S> Default for FSMOverride<S>
where
    S: Copy + Eq + core::hash::Hash + Send + Sync + 'static,
{
    fn default() -> Self {
        Self {
            mode: RuleType::All,
            transitions: HashSet::new(),
            call_rules: false,
        }
    }
}

impl<S> FSMOverride<S>
where
    S: Copy + Eq + core::hash::Hash + Send + Sync + 'static,
{
    /// Allow all transitions (validate only via `FSMTransition` trait).
    #[must_use]
    pub fn allow_all() -> Self {
        Self {
            mode: RuleType::All,
            transitions: HashSet::new(),
            call_rules: false,
        }
    }

    /// Deny all transitions (frozen state).
    #[must_use]
    pub fn deny_all() -> Self {
        Self {
            mode: RuleType::None,
            transitions: HashSet::new(),
            call_rules: false,
        }
    }

    /// Allow only whitelisted transitions.
    pub fn whitelist<I>(edges: I) -> Self
    where
        I: IntoIterator<Item = (S, S)>,
    {
        Self {
            mode: RuleType::Whitelist,
            transitions: edges.into_iter().collect(),
            call_rules: false,
        }
    }

    /// Allow all except blacklisted transitions.
    pub fn blacklist<I>(edges: I) -> Self
    where
        I: IntoIterator<Item = (S, S)>,
    {
        Self {
            mode: RuleType::Blacklist,
            transitions: edges.into_iter().collect(),
            call_rules: false,
        }
    }

    /// Enable `FSMTransition` validation for transitions NOT decided by config.
    ///
    /// By default, `FSMOverride` is the sole authority - whitelisted/blacklisted transitions
    /// are decided by config alone, others have default behavior. Calling `with_rules()`
    /// applies `FSMTransition` validation to the "gap" transitions not explicitly listed.
    ///
    /// **Behavior by mode:**
    /// - **Whitelist**: Listed transitions still ACCEPT (config priority). Unlisted
    ///   transitions now check `FSMTransition` instead of auto-denying.
    /// - **Blacklist**: Listed transitions still DENY (config priority). Unlisted
    ///   transitions now check `FSMTransition` instead of auto-allowing.
    /// - **All**: No whitelist/blacklist, so all transitions check `FSMTransition`.
    /// - **None**: No effect (all transitions denied).
    ///
    /// # Examples
    /// ```rust
    /// # use bevy_fsm::FSMOverride;
    /// # #[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
    /// # enum MyState { A, B, C }
    ///
    /// // Whitelist without with_rules: whitelist is sole authority
    /// let config = FSMOverride::whitelist([(MyState::A, MyState::C)]);
    /// // A->C: ACCEPT (whitelisted)
    /// // A->B: DENY (not whitelisted, no rules check)
    ///
    /// // Whitelist with with_rules: whitelist wins, rules fill gaps
    /// let config = FSMOverride::whitelist([(MyState::A, MyState::C)]).with_rules();
    /// // A->C: ACCEPT (whitelisted, config priority)
    /// // A->B: Check FSMTransition (not whitelisted, rules enabled)
    ///
    /// // Blacklist without with_rules: blacklist is sole authority
    /// let config = FSMOverride::blacklist([(MyState::A, MyState::C)]);
    /// // A->C: DENY (blacklisted)
    /// // A->B: ACCEPT (not blacklisted, no rules check)
    ///
    /// // Blacklist with with_rules: blacklist wins, rules fill gaps
    /// let config = FSMOverride::blacklist([(MyState::A, MyState::C)]).with_rules();
    /// // A->C: DENY (blacklisted, config priority)
    /// // A->B: Check FSMTransition (not blacklisted, rules enabled)
    /// ```
    #[must_use]
    pub fn with_rules(mut self) -> Self {
        self.call_rules = true;
        self
    }

    /// Add transitions to the set.
    ///
    /// For whitelist mode: adds allowed transitions.
    /// For blacklist mode: adds denied transitions.
    /// For All/None modes: has no effect.
    #[must_use]
    pub fn and_allow<I>(mut self, edges: I) -> Self
    where
        I: IntoIterator<Item = (S, S)>,
    {
        self.transitions.extend(edges);
        self
    }

    /// Add denied transitions (for blacklist mode).
    ///
    /// Alias for `and_allow()` when using blacklist mode for semantic clarity.
    #[must_use]
    pub fn and_deny<I>(mut self, edges: I) -> Self
    where
        I: IntoIterator<Item = (S, S)>,
    {
        self.transitions.extend(edges);
        self
    }

    /// Check if a transition is allowed by this config.
    pub fn is_transition_allowed(&self, from: S, to: S) -> bool {
        match self.mode {
            RuleType::All => true,
            RuleType::None => false,
            RuleType::Whitelist => self.transitions.contains(&(from, to)),
            RuleType::Blacklist => !self.transitions.contains(&(from, to)),
        }
    }
}

/// Observer that fires [`Enter`] for the initial state when an FSM component is first added.
///
/// The event's `from` is `None`. Registered by [`FSMPlugin`] unless
/// [`FSMPlugin::ignore_fsm_addition`] is set.
///
/// # Important Timing Considerations
///
/// **WARNING**: Enter events fire **in the same frame** as entity spawn when the FSM
/// component is added during `spawn()`. This means:
///
/// - The entity is **not yet fully initialized** - other components may not exist yet
/// - Child entities may not be spawned
///
/// **Best practices**:
/// - Avoid accessing other components in initial Enter event handlers
/// - For initialization logic, consider using a separate system that runs after spawn
/// - Do not despawn entities in Enter events that may fire on spawn
/// - Do not insert components in Enter events that may fire on spawn
///
/// # Example
/// ```
/// # use bevy::prelude::*;
/// # use bevy_fsm::{FSMState, FSMTransition, Enter};
/// # #[derive(Component)]
/// # struct HealthBar;
/// # #[derive(Component, FSMTransition, FSMState, Clone, Copy, Debug, PartialEq, Eq, Hash)]
/// # #[component(immutable)]
/// # enum LifeFSM { Alive }
/// fn on_alive_enter(
///     trigger: On<Enter<life_fsm::Alive>>,
///     mut commands: Commands,
/// ) {
///     // SAFE: Queue command for later execution
///     commands.entity(trigger.entity).insert(HealthBar);
///
///     // UNSAFE: Don't query other components - they may not exist yet!
///     // let health = query.get(trigger.entity).unwrap(); // May panic!
/// }
/// ```
///
/// For manual registration:
/// ```
/// # use bevy::prelude::*;
/// # use bevy_fsm::{FSMState, FSMTransition, on_fsm_added};
/// # #[derive(Component, FSMTransition, FSMState, Reflect, Clone, Copy, Debug, PartialEq, Eq, Hash)]
/// # #[component(immutable)]
/// # enum YourFSM { StateA }
/// # let mut app = App::new();
/// app.world_mut().add_observer(on_fsm_added::<YourFSM>);
/// ```
pub fn on_fsm_added<S: FSMState>(trigger: On<Add<S>>, mut commands: Commands, q_state: Query<&S>) {
    let entity = trigger.entity;
    let Ok(&state) = q_state.get(entity) else {
        return;
    };
    commands.queue(move |world: &mut World| trigger_enter(world, entity, state, None));
}

/// Generic plugin for FSM types that automatically sets up core observers.
///
/// This plugin registers the state type for reflection, creates the FSM's observer
/// group, and adds [`on_fsm_added`], which fires Enter events when the FSM component
/// is first added. Transitions need no observer: [`StateChangeRequest`] is a command.
///
/// # Timing Warning
///
/// See [`on_fsm_added`] documentation for timing considerations and best practices.
///
/// # Example
/// ```
/// # use bevy::prelude::*;
/// # use bevy_fsm::{FSMState, FSMTransition, FSMPlugin, fsm_observer, Enter};
/// # #[derive(Component, FSMTransition, FSMState, Reflect, Clone, Copy, Debug, PartialEq, Eq, Hash)]
/// # #[component(immutable)]
/// # enum LifeFSM { Alive, Dying }
/// # fn on_dying_observer(_: On<Enter<life_fsm::Dying>>) {}
/// # let mut app = App::new();
/// app.add_plugins(FSMPlugin::<LifeFSM>::default());
///
/// // Register additional observers using fsm_observer! macro:
/// fsm_observer!(app, LifeFSM, on_dying_observer);
/// ```
pub struct FSMPlugin<S: FSMState> {
    /// If true, skip registering the `on_fsm_added` observer
    ignore_fsm_addition: bool,
    _phantom: PhantomData<S>,
}

impl<S: FSMState> Default for FSMPlugin<S> {
    fn default() -> Self {
        Self {
            ignore_fsm_addition: false,
            _phantom: PhantomData,
        }
    }
}

impl<S: FSMState> FSMPlugin<S> {
    /// Create a new `FSMPlugin` with default settings.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Skip registering the `on_fsm_added` observer.
    ///
    /// Use this if you don't want automatic Enter events when the FSM component is added.
    #[must_use]
    pub fn ignore_fsm_addition(mut self) -> Self {
        self.ignore_fsm_addition = true;
        self
    }
}

impl<S: FSMState + Reflect + GetTypeRegistration> Plugin for FSMPlugin<S> {
    fn build(&self, app: &mut App) {
        app.register_type::<S>();
        let world = app.world_mut();
        let group_entity = ensure_fsm_group::<S>(world);
        if !self.ignore_fsm_addition {
            let added_entity = {
                let mut observer = world.add_observer(on_fsm_added::<S>);
                observer.insert(Name::new("on_fsm_added"));
                observer.insert(FSMObserverMarker::<S>::default());
                observer.id()
            };
            world.entity_mut(group_entity).add_child(added_entity);
        }
    }
}

/// Tracks the root observer entity and per-type observer groups.
#[derive(Resource)]
struct FSMObserverHierarchy {
    root: Entity,
    groups: HashMap<TypeId, Entity>,
}

/// Marker component for the root `FSMObservers` entity.
#[derive(Component)]
struct FSMObserversRoot;

/// Ensures the root `FSMObservers` entity exists and returns its [`Entity`] id.
fn ensure_fsm_hierarchy(world: &mut World) -> Entity {
    if let Some(hierarchy) = world.get_resource::<FSMObserverHierarchy>() {
        return hierarchy.root;
    }

    let root = world
        .spawn((Name::new("FSMObservers"), FSMObserversRoot))
        .id();

    world.insert_resource(FSMObserverHierarchy {
        root,
        groups: HashMap::default(),
    });

    root
}

/// Ensures an observer group exists for the FSM type and returns its entity id.
fn ensure_fsm_group<S>(world: &mut World) -> Entity
where
    S: Send + Sync + 'static,
{
    let root = ensure_fsm_hierarchy(world);
    let type_id = TypeId::of::<S>();

    if let Some(group) = {
        let hierarchy = world.resource::<FSMObserverHierarchy>();
        hierarchy.groups.get(&type_id).copied()
    } {
        return group;
    }

    let type_name = std::any::type_name::<S>()
        .split("::")
        .last()
        .unwrap_or("UnknownFSM")
        .to_string();

    let group = world
        .spawn((Name::new(type_name), FSMObserverGroup::<S>::default()))
        .id();

    world.entity_mut(root).add_child(group);

    world
        .resource_mut::<FSMObserverHierarchy>()
        .groups
        .insert(type_id, group);

    group
}

/// Attaches an observer entity to the hierarchy for the FSM type `S`.
pub fn attach_observer_to_group<S>(world: &mut World, observer: Entity)
where
    S: Send + Sync + 'static,
{
    let group_entity = ensure_fsm_group::<S>(world);
    world.entity_mut(group_entity).add_child(observer);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Component, FSMState, Reflect, Clone, Copy, Debug, Hash, PartialEq, Eq)]
    #[component(immutable)]
    #[reflect(Component)]
    enum TestState {
        A,
        B,
        C,
    }

    impl FSMTransition for TestState {
        fn can_transition(from: Self, to: Self) -> bool {
            !(matches!(from, TestState::A) && matches!(to, TestState::C))
        }
    }

    #[derive(Resource, Default)]
    struct EventLog {
        enters: Vec<(TestState, Option<TestState>)>,
        exits: Vec<(TestState, TestState)>,
        variant_enters_b: usize,
        variant_exits_a: usize,
    }

    fn on_enter(trigger: On<Enter<TestState>>, mut log: ResMut<EventLog>) {
        log.enters.push((trigger.state, trigger.from));
    }

    fn on_exit(trigger: On<Exit<TestState>>, mut log: ResMut<EventLog>) {
        log.exits.push((trigger.state, trigger.to));
    }

    fn on_enter_b(trigger: On<Enter<test_state::B>>, mut log: ResMut<EventLog>) {
        assert_eq!(trigger.state, TestState::B);
        log.variant_enters_b += 1;
    }

    fn on_exit_a(trigger: On<Exit<test_state::A>>, mut log: ResMut<EventLog>) {
        assert_eq!(trigger.state, TestState::A);
        log.variant_exits_a += 1;
    }

    fn test_app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.init_resource::<EventLog>();
        app
    }

    fn request(app: &mut App, entity: Entity, next: TestState) {
        app.world_mut()
            .commands()
            .queue(StateChangeRequest { entity, next });
        app.update();
    }

    fn reset(app: &mut App, entity: Entity, state: TestState) {
        app.world_mut()
            .commands()
            .queue(ResetState { entity, state });
        app.update();
    }

    fn state(app: &App, entity: Entity) -> TestState {
        *app.world().get::<TestState>(entity).unwrap()
    }

    #[test]
    fn transitions_apply_and_fire_events() {
        let mut app = test_app();
        app.add_observer(on_enter);
        app.add_observer(on_exit);

        let e = app.world_mut().spawn(TestState::A).id();
        request(&mut app, e, TestState::B);

        assert_eq!(state(&app, e), TestState::B);
        let log = app.world().resource::<EventLog>();
        assert_eq!(log.exits, vec![(TestState::A, TestState::B)]);
        assert_eq!(log.enters, vec![(TestState::B, Some(TestState::A))]);
    }

    #[test]
    fn variant_observers_fire_only_for_their_variant() {
        let mut app = test_app();
        app.add_observer(on_enter_b);
        app.add_observer(on_exit_a);

        let e = app.world_mut().spawn(TestState::A).id();
        request(&mut app, e, TestState::B);
        request(&mut app, e, TestState::C);
        request(&mut app, e, TestState::B);

        let log = app.world().resource::<EventLog>();
        assert_eq!(log.variant_enters_b, 2);
        assert_eq!(log.variant_exits_a, 1);
    }

    #[test]
    fn variant_markers_of_different_fsms_do_not_cross_fire() {
        let mut app = test_app();
        app.add_observer(on_enter_b);

        let e = app.world_mut().spawn((TestState::A, SecondFSM::X)).id();
        app.world_mut().commands().queue(StateChangeRequest {
            entity: e,
            next: SecondFSM::Y,
        });
        app.update();

        assert_eq!(app.world().resource::<EventLog>().variant_enters_b, 0);
    }

    #[test]
    fn exit_sees_old_state_and_enter_sees_new_state() {
        #[derive(Resource, Default)]
        struct Seen(Vec<TestState>);

        let mut app = test_app();
        app.init_resource::<Seen>();
        app.add_observer(
            |trigger: On<Exit<test_state::A>>, q: Query<&TestState>, mut seen: ResMut<Seen>| {
                seen.0.push(*q.get(trigger.entity).unwrap());
            },
        );
        app.add_observer(
            |trigger: On<Enter<test_state::B>>, q: Query<&TestState>, mut seen: ResMut<Seen>| {
                seen.0.push(*q.get(trigger.entity).unwrap());
            },
        );

        let e = app.world_mut().spawn(TestState::A).id();
        request(&mut app, e, TestState::B);

        assert_eq!(
            app.world().resource::<Seen>().0,
            vec![TestState::A, TestState::B]
        );
    }

    #[test]
    fn exit_observer_commands_apply_before_enter() {
        #[derive(Component)]
        struct LeftA;

        #[derive(Resource, Default)]
        struct SawLeftA(bool);

        let mut app = test_app();
        app.init_resource::<SawLeftA>();
        app.add_observer(|trigger: On<Exit<test_state::A>>, mut commands: Commands| {
            commands.entity(trigger.entity).insert(LeftA);
        });
        app.add_observer(
            |trigger: On<Enter<test_state::B>>,
             q: Query<(), With<LeftA>>,
             mut saw: ResMut<SawLeftA>| {
                saw.0 = q.contains(trigger.entity);
            },
        );

        let e = app.world_mut().spawn(TestState::A).id();
        request(&mut app, e, TestState::B);

        assert!(app.world().resource::<SawLeftA>().0);
    }

    #[test]
    fn exit_observer_removing_state_stops_the_transition() {
        let mut app = test_app();
        app.add_observer(on_enter);
        app.add_observer(|trigger: On<Exit<test_state::A>>, mut commands: Commands| {
            commands.entity(trigger.entity).remove::<TestState>();
        });

        let e = app.world_mut().spawn(TestState::A).id();
        request(&mut app, e, TestState::B);

        assert!(app.world().get::<TestState>(e).is_none());
        assert!(app.world().resource::<EventLog>().enters.is_empty());
    }

    #[test]
    fn exit_observer_requests_are_rejected() {
        #[derive(Resource, Default)]
        struct Outcome(Option<Result<TestState, TransitionError>>);

        let mut app = test_app();
        app.init_resource::<Outcome>();
        app.add_observer(on_enter);
        app.add_observer(|trigger: On<Exit<test_state::A>>, mut commands: Commands| {
            let entity = trigger.entity;
            commands.queue(move |world: &mut World| {
                let outcome = StateChangeRequest {
                    entity,
                    next: TestState::C,
                }
                .try_apply(world);
                world.resource_mut::<Outcome>().0 = Some(outcome);
            });
        });

        let e = app.world_mut().spawn(TestState::A).id();
        request(&mut app, e, TestState::B);

        assert_eq!(state(&app, e), TestState::B);
        assert_eq!(
            app.world().resource::<Outcome>().0,
            Some(Err(TransitionError::Exiting))
        );
        assert_eq!(
            app.world().resource::<EventLog>().enters,
            vec![(TestState::B, Some(TestState::A))]
        );
    }

    #[test]
    fn exit_observer_may_request_other_entities_and_fsms() {
        let mut app = test_app();
        let other = app.world_mut().spawn(TestState::A).id();
        app.add_observer(
            move |trigger: On<Exit<test_state::B>>, mut commands: Commands| {
                commands.queue(StateChangeRequest {
                    entity: other,
                    next: TestState::B,
                });
                commands.queue(StateChangeRequest {
                    entity: trigger.entity,
                    next: SecondFSM::Y,
                });
            },
        );

        let e = app.world_mut().spawn((TestState::B, SecondFSM::X)).id();
        request(&mut app, e, TestState::C);

        assert_eq!(state(&app, e), TestState::C);
        assert_eq!(state(&app, other), TestState::B);
        assert_eq!(*app.world().get::<SecondFSM>(e).unwrap(), SecondFSM::Y);
    }

    #[test]
    fn enter_observers_chain_transitions() {
        let mut app = test_app();
        app.add_observer(on_fsm_added::<TestState>);
        app.add_observer(on_enter);
        app.add_observer(
            |trigger: On<Enter<test_state::A>>, mut commands: Commands| {
                commands.queue(StateChangeRequest {
                    entity: trigger.entity,
                    next: TestState::B,
                });
            },
        );
        app.add_observer(
            |trigger: On<Enter<test_state::B>>, mut commands: Commands| {
                commands.queue(StateChangeRequest {
                    entity: trigger.entity,
                    next: TestState::C,
                });
            },
        );

        let e = app.world_mut().spawn(TestState::A).id();
        app.update();

        assert_eq!(state(&app, e), TestState::C);
        assert_eq!(
            app.world().resource::<EventLog>().enters,
            vec![
                (TestState::A, None),
                (TestState::B, Some(TestState::A)),
                (TestState::C, Some(TestState::B)),
            ]
        );
    }

    #[test]
    fn guard_blocks_invalid_transitions() {
        let mut app = test_app();
        app.add_observer(on_exit);

        let e = app.world_mut().spawn(TestState::A).id();
        request(&mut app, e, TestState::C);

        assert_eq!(state(&app, e), TestState::A);
        assert!(app.world().resource::<EventLog>().exits.is_empty());
    }

    #[test]
    fn try_apply_reports_the_outcome() {
        let mut world = World::new();
        let e = world.spawn(TestState::A).id();
        let missing = world.spawn_empty().id();

        assert_eq!(
            StateChangeRequest {
                entity: e,
                next: TestState::A
            }
            .try_apply(&mut world),
            Err(TransitionError::Unchanged)
        );
        assert_eq!(
            StateChangeRequest {
                entity: e,
                next: TestState::C
            }
            .try_apply(&mut world),
            Err(TransitionError::Denied)
        );
        assert_eq!(
            StateChangeRequest {
                entity: missing,
                next: TestState::B
            }
            .try_apply(&mut world),
            Err(TransitionError::NoState)
        );
        assert_eq!(
            StateChangeRequest {
                entity: e,
                next: TestState::B
            }
            .try_apply(&mut world),
            Ok(TestState::A)
        );
        assert_eq!(*world.get::<TestState>(e).unwrap(), TestState::B);
    }

    #[test]
    fn reset_state_skips_rules_and_events() {
        let mut app = test_app();
        app.add_observer(on_enter);
        app.add_observer(on_exit);

        let e = app
            .world_mut()
            .spawn((TestState::A, FSMOverride::<TestState>::deny_all()))
            .id();
        reset(&mut app, e, TestState::C);

        assert_eq!(state(&app, e), TestState::C);
        let log = app.world().resource::<EventLog>();
        assert!(log.enters.is_empty());
        assert!(log.exits.is_empty());
    }

    #[test]
    fn reset_state_ignores_entities_without_state() {
        let mut app = test_app();
        let e = app.world_mut().spawn_empty().id();
        reset(&mut app, e, TestState::B);
        assert!(app.world().get::<TestState>(e).is_none());
    }

    #[test]
    fn on_fsm_added_fires_initial_enter_events() {
        let mut app = test_app();
        app.add_observer(on_fsm_added::<TestState>);
        app.add_observer(on_enter);
        app.add_observer(
            |trigger: On<Enter<test_state::A>>, mut log: ResMut<EventLog>| {
                assert_eq!(trigger.from, None);
                log.variant_exits_a += 1;
            },
        );

        app.world_mut().spawn(TestState::A);
        app.update();

        let log = app.world().resource::<EventLog>();
        assert_eq!(log.enters, vec![(TestState::A, None)]);
        assert_eq!(
            log.variant_exits_a, 1,
            "variant observer sees the initial enter"
        );
    }

    #[test]
    fn fsm_observer_macro_registers_and_organizes() {
        let mut app = test_app();

        let observer_entity = fsm_observer!(app, TestState, on_enter).id();

        let world = app.world();
        assert_eq!(
            world.get::<Name>(observer_entity).map(Name::as_str),
            Some("on_enter"),
            "observer should be named after the system"
        );
        assert!(
            world
                .get::<FSMObserverMarker<TestState>>(observer_entity)
                .is_some(),
            "observer should carry the FSM marker"
        );

        let e = app.world_mut().spawn(TestState::A).id();
        request(&mut app, e, TestState::B);
        assert_eq!(
            app.world().resource::<EventLog>().enters,
            vec![(TestState::B, Some(TestState::A))]
        );

        let root = {
            let mut query = app
                .world_mut()
                .query_filtered::<Entity, With<FSMObserversRoot>>();
            query
                .single(app.world())
                .expect("FSMObservers root entity should be created")
        };
        let group = {
            let mut query = app
                .world_mut()
                .query_filtered::<Entity, With<FSMObserverGroup<TestState>>>();
            query
                .single(app.world())
                .expect("TestState group should exist")
        };
        assert_eq!(
            app.world().get::<ChildOf>(group).map(ChildOf::parent),
            Some(root),
            "TestState group should be child of FSMObservers"
        );
        assert_eq!(
            app.world()
                .get::<ChildOf>(observer_entity)
                .map(ChildOf::parent),
            Some(group),
            "observer should be parented under the TestState group"
        );
    }

    #[test]
    fn fsm_config_whitelist_mode() {
        let mut app = test_app();
        let e = app
            .world_mut()
            .spawn((
                TestState::A,
                FSMOverride::whitelist([
                    (TestState::A, TestState::B),
                    (TestState::A, TestState::C), // Override FSMTransition
                ]),
            ))
            .id();

        request(&mut app, e, TestState::B);
        assert_eq!(state(&app, e), TestState::B);

        reset(&mut app, e, TestState::A);
        request(&mut app, e, TestState::C);
        assert_eq!(
            state(&app, e),
            TestState::C,
            "Whitelist should override FSMTransition when call_rules is false"
        );
    }

    #[test]
    fn fsm_config_whitelist_with_rules() {
        let mut app = test_app();
        let e = app
            .world_mut()
            .spawn((
                TestState::A,
                FSMOverride::whitelist([
                    (TestState::A, TestState::C), // Force allow A->C
                ])
                .with_rules(),
            ))
            .id();

        request(&mut app, e, TestState::C);
        assert_eq!(
            state(&app, e),
            TestState::C,
            "Whitelisted transition should accept regardless of FSMTransition"
        );

        reset(&mut app, e, TestState::A);
        request(&mut app, e, TestState::B);
        assert_eq!(
            state(&app, e),
            TestState::B,
            "Non-whitelisted but FSMTransition-valid transition should succeed with with_rules"
        );

        request(&mut app, e, TestState::C);
        assert_eq!(
            state(&app, e),
            TestState::C,
            "Non-whitelisted transition should defer to FSMTransition when with_rules enabled"
        );
    }

    #[test]
    fn fsm_config_blacklist_mode() {
        let mut app = test_app();
        let e = app
            .world_mut()
            .spawn((
                TestState::A,
                FSMOverride::blacklist([(TestState::A, TestState::C)]),
            ))
            .id();

        // Non-blacklisted transition should succeed (even if FSMTransition would block it)
        request(&mut app, e, TestState::B);
        assert_eq!(state(&app, e), TestState::B);

        reset(&mut app, e, TestState::A);
        request(&mut app, e, TestState::C);
        assert_eq!(
            state(&app, e),
            TestState::A,
            "Blacklisted transition should be denied"
        );
    }

    #[test]
    fn fsm_config_blacklist_with_rules() {
        let mut app = test_app();
        // Blacklist B->C, but A->C is also invalid per FSMTransition
        let e = app
            .world_mut()
            .spawn((
                TestState::A,
                FSMOverride::blacklist([(TestState::B, TestState::C)]).with_rules(),
            ))
            .id();

        request(&mut app, e, TestState::C);
        assert_eq!(
            state(&app, e),
            TestState::A,
            "FSMTransition should block A->C even though not blacklisted"
        );

        request(&mut app, e, TestState::B);
        assert_eq!(state(&app, e), TestState::B);

        request(&mut app, e, TestState::C);
        assert_eq!(state(&app, e), TestState::B, "Blacklist should block B->C");
    }

    #[test]
    fn fsm_config_deny_all_mode() {
        let mut app = test_app();
        let e = app
            .world_mut()
            .spawn((TestState::A, FSMOverride::<TestState>::deny_all()))
            .id();

        request(&mut app, e, TestState::B);
        assert_eq!(state(&app, e), TestState::A);
    }

    #[test]
    fn fsm_config_allow_all_mode() {
        let mut app = test_app();
        let e = app
            .world_mut()
            .spawn((TestState::A, FSMOverride::<TestState>::allow_all()))
            .id();

        request(&mut app, e, TestState::B);
        assert_eq!(state(&app, e), TestState::B);

        reset(&mut app, e, TestState::A);
        // Even invalid transition (A->C) is allowed because FSMTransition is not checked
        request(&mut app, e, TestState::C);
        assert_eq!(
            state(&app, e),
            TestState::C,
            "allow_all without call_rules should bypass FSMTransition"
        );
    }

    #[test]
    fn fsm_config_allow_all_with_rules() {
        let mut app = test_app();
        let e = app
            .world_mut()
            .spawn((
                TestState::A,
                FSMOverride::<TestState>::allow_all().with_rules(),
            ))
            .id();

        request(&mut app, e, TestState::B);
        assert_eq!(state(&app, e), TestState::B);

        reset(&mut app, e, TestState::A);
        request(&mut app, e, TestState::C);
        assert_eq!(
            state(&app, e),
            TestState::A,
            "allow_all with call_rules should enforce FSMTransition"
        );
    }

    // Test with FSMPlugin using a real FSMState enum
    #[derive(Component, FSMState, Reflect, Clone, Copy, Debug, PartialEq, Eq, Hash)]
    #[component(immutable)]
    #[reflect(Component)]
    enum PluginTestState {
        Initial,
        Active,
        Done,
    }

    impl FSMTransition for PluginTestState {
        fn can_transition(from: Self, to: Self) -> bool {
            matches!(
                (from, to),
                (PluginTestState::Initial, PluginTestState::Active)
                    | (PluginTestState::Active, PluginTestState::Done)
            ) || from == to
        }
    }

    #[derive(Resource, Default)]
    struct PluginEventLog {
        enters: Vec<PluginTestState>,
    }

    fn on_plugin_enter(trigger: On<Enter<PluginTestState>>, mut log: ResMut<PluginEventLog>) {
        log.enters.push(trigger.event().state);
    }

    #[test]
    fn fsm_plugin_fires_initial_enter_event() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.init_resource::<PluginEventLog>();
        app.add_plugins(FSMPlugin::<PluginTestState>::default());
        app.world_mut().add_observer(on_plugin_enter);

        let entity = app.world_mut().spawn(PluginTestState::Initial).id();
        app.update();

        assert_eq!(
            app.world().resource::<PluginEventLog>().enters,
            vec![PluginTestState::Initial],
            "FSMPlugin should fire Enter event for initial state when entity is spawned"
        );

        app.world_mut().commands().queue(StateChangeRequest {
            entity,
            next: PluginTestState::Active,
        });
        app.update();

        assert_eq!(
            *app.world().get::<PluginTestState>(entity).unwrap(),
            PluginTestState::Active
        );
        assert_eq!(
            app.world().resource::<PluginEventLog>().enters,
            vec![PluginTestState::Initial, PluginTestState::Active],
            "FSMPlugin should fire Enter events for both initial state and transitions"
        );
    }

    #[test]
    fn fsm_plugin_ignore_fsm_addition_skips_initial_enter() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.init_resource::<PluginEventLog>();
        app.add_plugins(FSMPlugin::<PluginTestState>::new().ignore_fsm_addition());
        app.world_mut().add_observer(on_plugin_enter);

        app.world_mut().spawn(PluginTestState::Initial);
        app.update();

        assert!(app.world().resource::<PluginEventLog>().enters.is_empty());
    }

    /// A second reflectable FSM used to verify multiple `FSMPlugin`s share one
    /// hierarchy root.
    #[derive(
        Component, FSMState, FSMTransition, Reflect, Clone, Copy, Debug, PartialEq, Eq, Hash,
    )]
    #[component(immutable)]
    #[reflect(Component)]
    enum SecondFSM {
        X,
        Y,
    }

    /// `On<Add<S>>` must be *add-only*: a transition re-inserts the state component
    /// (an Insert/Discard, not an Add), so the initial Enter must not repeat.
    #[test]
    fn on_fsm_added_is_add_only_not_on_reinsert() {
        let mut app = test_app();
        app.add_observer(on_fsm_added::<TestState>);
        app.add_observer(on_enter);

        let e = app.world_mut().spawn(TestState::A).id();
        app.update();
        assert_eq!(
            app.world().resource::<EventLog>().enters,
            vec![(TestState::A, None)],
            "on_fsm_added should fire exactly one initial Enter on spawn"
        );

        reset(&mut app, e, TestState::B);
        assert_eq!(
            app.world().resource::<EventLog>().enters,
            vec![(TestState::A, None)],
            "On<Add> must not re-fire when the state component is re-inserted"
        );
    }

    #[test]
    fn request_ignores_despawned_entity() {
        let mut app = test_app();
        let e = app.world_mut().spawn(TestState::A).id();
        app.world_mut().entity_mut(e).despawn();

        request(&mut app, e, TestState::B); // must not panic

        assert!(app.world().get_entity(e).is_err());
    }

    #[test]
    fn request_ignores_removed_component() {
        let mut app = test_app();
        let e = app.world_mut().spawn(TestState::A).id();
        app.world_mut().entity_mut(e).remove::<TestState>();

        request(&mut app, e, TestState::B); // must not panic

        assert!(
            app.world().get::<TestState>(e).is_none(),
            "request for an entity without the FSM component should be ignored"
        );
    }

    #[test]
    fn fsm_plugin_registers_type_for_reflection() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(FSMPlugin::<PluginTestState>::default());

        let registry = app.world().resource::<AppTypeRegistry>().read();
        assert!(
            registry.get(TypeId::of::<PluginTestState>()).is_some(),
            "FSMPlugin should register its state type in the reflection registry"
        );
    }

    #[test]
    fn multiple_fsm_plugins_share_single_root() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(FSMPlugin::<PluginTestState>::default());
        app.add_plugins(FSMPlugin::<SecondFSM>::default());

        let mut roots = app
            .world_mut()
            .query_filtered::<Entity, With<FSMObserversRoot>>();
        assert_eq!(
            roots.iter(app.world()).count(),
            1,
            "all FSM plugins should share exactly one FSMObservers root"
        );

        let mut g1 = app
            .world_mut()
            .query_filtered::<Entity, With<FSMObserverGroup<PluginTestState>>>();
        let mut g2 = app
            .world_mut()
            .query_filtered::<Entity, With<FSMObserverGroup<SecondFSM>>>();
        assert_eq!(g1.iter(app.world()).count(), 1);
        assert_eq!(g2.iter(app.world()).count(), 1);
    }

    mod visibility {
        use super::*;

        // Private enum: markers must be visible to this module without leaking it.
        #[derive(Component, FSMState, FSMTransition, Clone, Copy, Debug, PartialEq, Eq, Hash)]
        #[component(immutable)]
        enum PrivateFSM {
            On,
            Off,
        }

        #[derive(Component, FSMState, FSMTransition, Clone, Copy, Debug, PartialEq, Eq, Hash)]
        #[component(immutable)]
        #[allow(dead_code)]
        pub(crate) enum CrateFSM {
            On,
            Off,
        }

        mod nested {
            use super::*;

            #[derive(
                Component, FSMState, FSMTransition, Clone, Copy, Debug, PartialEq, Eq, Hash,
            )]
            #[component(immutable)]
            #[allow(dead_code)]
            pub(super) enum ParentFSM {
                On,
                Off,
            }
        }

        // Markers resolve wherever their enum is visible.
        type _Markers = (
            crate_fsm::On,
            nested::parent_fsm::Off,
            Enter<crate_fsm::Off>,
            Exit<nested::parent_fsm::On>,
        );

        #[test]
        fn markers_of_private_enum_are_usable_next_to_it() {
            #[derive(Resource, Default)]
            struct Count(u32);

            let mut world = World::new();
            world.init_resource::<Count>();
            world.add_observer(|_: On<Enter<private_fsm::Off>>, mut count: ResMut<Count>| {
                count.0 += 1;
            });
            let e = world.spawn(PrivateFSM::On).id();
            StateChangeRequest {
                entity: e,
                next: PrivateFSM::Off,
            }
            .try_apply(&mut world)
            .unwrap();
            assert_eq!(world.resource::<Count>().0, 1);
        }
    }
}
