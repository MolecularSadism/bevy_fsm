//! Observer-driven finite state machine framework for Bevy ECS.
//!
//! This crate provides a lightweight framework for modeling enum-based state machines
//! using Bevy's observer system. State machines are defined as enum components with
//! automatic variant-specific event generation.
//!
//! # Features
//!
//! - **Enum-based states**: Keep your states as simple enum variants
//! - **Observer-driven**: React to state changes via Bevy observers
//! - **Variant-specific events**: No runtime state checks needed in observers
//! - **Flexible validation**: Per-entity and per-type transition rules
//! - **Minimal API**: `FSMPlugin` for automatic setup
//! - **Organized hierarchy**: Observers automatically organized in entity hierarchy
//!
//! # Quick Start
//!
//! ```rust
//! use bevy::prelude::*;
//! use bevy_fsm::{FSMState, FSMTransition, FSMPlugin, StateChangeRequest, Enter, Exit, Transition, fsm_observer};
//! use bevy_enum_event::EnumEvent;
//!
//! fn plugin(app: &mut App) {
//!     // FSMPlugin automatically sets up the observer hierarchy on first use
//!     app.add_plugins(FSMPlugin::<LifeFSM>::default());
//!
//!     // Use fsm_observer! macro for variant-specific observers
//!     // This is functionally identical to a typed global observer but gets automatically parented
//!     // into a custom FSMObservers/LifeFSM/on_enter_dying hierarchy that keeps observers nicely
//!     // sorted by their respective FSM.
//!     fsm_observer!(app, LifeFSM, on_enter_dying);
//!     fsm_observer!(app, LifeFSM, on_exit_alive);
//!     fsm_observer!(app, LifeFSM, on_transition_dying_dead);
//! }
//!
//! // Zero boilerplate - just derive FSMTransition for "allow all" behavior!
//! #[derive(Component, EnumEvent, FSMTransition, FSMState, Reflect, Clone, Copy, Debug, PartialEq, Eq, Hash)]
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
//! fn on_transition_dying_dead(
//!     trigger: On<Transition<life_fsm::Dying, life_fsm::Dead>>,
//!     mut commands: Commands
//! ) {
//!     commands.entity(trigger.entity).despawn()
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
//! #[derive(Component, Clone, Copy, Debug, Hash, PartialEq, Eq)]
//! enum ActionFSM {
//!     Idle,
//!     Casting,
//!     Exhausted,
//! }
//!
//! impl FSMState for ActionFSM {}
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
//! │  ├─ apply_state_request
//! │  ├─ on_fsm_added
//! │  ├─ on_dying
//! │  └─ on_dead
//! ├─ BlockFSM
//! │  ├─ apply_state_request
//! │  └─ ...
//! └─ ...
//! ```
//!
//! This hierarchy is created automatically when you add your first FSM plugin,
//! with no additional setup required.

use bevy::prelude::*;
use bevy::{
    ecs::event::EntityEvent,
    platform::collections::{HashMap, HashSet},
    reflect::GetTypeRegistration,
};
// Re-export EnumEvent from bevy_enum_event and FSM derives from bevy_fsm_macros
// Note: FSMState and FSMTransition are both traits (below) and derive macros (from bevy_fsm_macros)
pub use bevy_enum_event::EnumEvent;
pub use bevy_fsm_macros::{FSMState, FSMTransition};
use std::any::TypeId;

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
/// # use bevy_enum_event::EnumEvent;
/// # #[derive(Component, EnumEvent, FSMTransition, FSMState, Reflect, Clone, Copy, Debug, PartialEq, Eq, Hash)]
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
    _phantom: std::marker::PhantomData<S>,
}

impl<S: Send + Sync + 'static> Default for FSMObserverMarker<S> {
    fn default() -> Self {
        Self {
            _phantom: std::marker::PhantomData,
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
    _phantom: std::marker::PhantomData<S>,
}

impl<S: Send + Sync + 'static> Default for FSMObserverGroup<S> {
    fn default() -> Self {
        Self {
            _phantom: std::marker::PhantomData,
        }
    }
}

/// Event requesting a state change for an entity.
#[derive(Event, Debug, Clone, Copy)]
pub struct StateChangeRequest<S: Copy + Send + Sync + 'static> {
    pub entity: Entity,
    pub next: S,
}

impl<S: Copy + Send + Sync + 'static> EntityEvent for StateChangeRequest<S> {
    fn event_target(&self) -> Entity {
        self.entity
    }
}

/// Event fired when an entity exits a state.
#[derive(Event, Debug, Clone, Copy)]
pub struct Exit<S: Copy + Send + Sync + 'static> {
    pub entity: Entity,
    pub state: S,
}

impl<S: Copy + Send + Sync + 'static> EntityEvent for Exit<S> {
    fn event_target(&self) -> Entity {
        self.entity
    }
}

/// Event fired when an entity enters a state.
#[derive(Event, Debug, Clone, Copy)]
pub struct Enter<S: Copy + Send + Sync + 'static> {
    pub entity: Entity,
    pub state: S,
}

impl<S: Copy + Send + Sync + 'static> EntityEvent for Enter<S> {
    fn event_target(&self) -> Entity {
        self.entity
    }
}

/// Event fired for state transitions.
#[derive(Event, Debug, Clone, Copy)]
pub struct Transition<F, T>
where
    F: Copy + Send + Sync + 'static,
    T: Copy + Send + Sync + 'static,
{
    pub entity: Entity,
    pub from: F,
    pub to: T,
}

impl<F, T> EntityEvent for Transition<F, T>
where
    F: Copy + Send + Sync + 'static,
    T: Copy + Send + Sync + 'static,
{
    fn event_target(&self) -> Entity {
        self.entity
    }
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

/// Core FSM trait implemented automatically by `#[derive(FSMState)]`.
///
/// This trait provides the infrastructure for variant-specific event generation
/// and state transition management.
pub trait FSMState: Component + Copy + Eq + Send + Sync + 'static + FSMTransition {
    /// Validate transition (delegated to `FSMTransition` impl).
    fn can_transition(from: Self, to: Self) -> bool {
        <Self as FSMTransition>::can_transition(from, to)
    }

    /// Context-aware validation with world access.
    fn can_transition_ctx(world: &World, entity: Entity, from: Self, to: Self) -> bool {
        <Self as FSMTransition>::can_transition_ctx(world, entity, from, to)
    }

    /// Fire variant-specific enter event (generated by derive macro).
    #[inline]
    fn trigger_enter_variant(_commands: &mut Commands, _entity: Entity, _state: Self) {}

    /// Fire variant-specific exit event (generated by derive macro).
    #[inline]
    fn trigger_exit_variant(_commands: &mut Commands, _entity: Entity, _state: Self) {}

    /// Fire variant-specific transition event (generated by derive macro).
    #[inline]
    fn trigger_transition_variant(
        _commands: &mut Commands,
        _entity: Entity,
        _from: Self,
        _to: Self,
    ) {
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

    /// Deny all transitions (immutable state).
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

    /// Deny all transitions (immutable state).
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

/// Observer that triggers enter events when an FSM component is first added.
///
/// **Note**: This is automatically registered when using `FSMPlugin` (recommended).
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
/// # use bevy_fsm::{FSMState, FSMTransition, on_fsm_added, Enter};
/// # #[derive(Component)]
/// # struct HealthBar;
/// # impl Default for HealthBar { fn default() -> Self { Self } }
/// # #[derive(Component, Clone, Copy, Debug, PartialEq, Eq, Hash)]
/// # enum LifeFSM { Alive }
/// # impl FSMState for LifeFSM {}
/// # impl FSMTransition for LifeFSM {
/// #     fn can_transition(_: Self, _: Self) -> bool { true }
/// # }
/// fn on_alive_enter(
///     trigger: On<Enter<LifeFSM>>,
///     mut commands: Commands,
/// ) {
///     // SAFE: Queue command for later execution
///     commands.entity(trigger.entity).insert(HealthBar::default());
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
/// # use bevy_enum_event::EnumEvent;
/// # #[derive(Component, EnumEvent, FSMTransition, FSMState, Reflect, Clone, Copy, Debug, PartialEq, Eq, Hash)]
/// # enum YourFSM { StateA }
/// # let mut app = App::new();
/// app.world_mut().add_observer(on_fsm_added::<YourFSM>);
/// ```
#[allow(clippy::needless_pass_by_value)]
pub fn on_fsm_added<S: FSMState>(trigger: On<Add, S>, mut commands: Commands, q_state: Query<&S>) {
    let entity = trigger.entity;

    let Ok(&state) = q_state.get(entity) else {
        return;
    };

    // Fire enter events for initial state
    commands.trigger(Enter::<S> { entity, state });
    S::trigger_enter_variant(&mut commands, entity, state);
}

/// Observer that applies state change requests.
///
/// For manual registration:
/// ```
/// # use bevy::prelude::*;
/// # use bevy_fsm::{FSMState, FSMTransition, apply_state_request};
/// # use bevy_enum_event::EnumEvent;
/// # #[derive(Component, EnumEvent, FSMTransition, FSMState, Reflect, Clone, Copy, Debug, PartialEq, Eq, Hash)]
/// # enum YourFSM { StateA }
/// # let mut app = App::new();
/// app.world_mut().add_observer(apply_state_request::<YourFSM>);
/// ```
///
/// Gracefully handles entities that may have been despawned or had their FSM
/// component removed by using a query to check component existence.
#[allow(clippy::needless_pass_by_value)]
pub fn apply_state_request<S: FSMState + core::hash::Hash>(
    trigger: On<StateChangeRequest<S>>,
    mut commands: Commands,
    world: &World,
    q_state: Query<&S>,
) {
    let entity = trigger.event().entity;

    // Query fails gracefully if entity was despawned or component removed
    let current = q_state.get(entity).ok().copied();

    if let Some(cur) = current {
        let next = trigger.event().next;
        if cur == next {
            return;
        }

        // Validation flow with priority model:
        // FSMOverride (if present) has priority - it can force accept or force deny
        // FSMTransition rules only apply to transitions NOT decided by FSMOverride
        if let Some(cfg) = world.get::<FSMOverride<S>>(entity) {
            let in_set = cfg.transitions.contains(&(cur, next));

            match cfg.mode {
                RuleType::All => {
                    // All mode: no config restrictions, optionally check rules
                    if cfg.call_rules
                        && !<S as FSMState>::can_transition_ctx(world, entity, cur, next)
                    {
                        return;
                    }
                }
                RuleType::None => {
                    // None mode: deny everything
                    return;
                }
                RuleType::Whitelist => {
                    if in_set {
                        // ON whitelist: ACCEPT immediately (whitelist wins)
                        // Don't check FSMTransition - whitelist has priority
                    } else {
                        // NOT on whitelist: check rules if enabled, otherwise deny
                        if cfg.call_rules {
                            if !<S as FSMState>::can_transition_ctx(world, entity, cur, next) {
                                return;
                            }
                        } else {
                            // Not on whitelist and no rules checking: deny
                            return;
                        }
                    }
                }
                RuleType::Blacklist => {
                    if in_set {
                        // ON blacklist: DENY immediately (blacklist wins)
                        return;
                    }
                    // NOT on blacklist: check rules if enabled
                    if cfg.call_rules
                        && !<S as FSMState>::can_transition_ctx(world, entity, cur, next)
                    {
                        return;
                    }
                }
            }
        } else {
            // No FSMOverride - fall back to type-level FSMTransition validation
            if !<S as FSMState>::can_transition_ctx(world, entity, cur, next) {
                return;
            }
        }

        // Fire exit
        commands.trigger(Exit::<S> { entity, state: cur });
        S::trigger_exit_variant(&mut commands, entity, cur);

        // Fire transition
        commands.trigger(Transition::<S, S> {
            entity,
            from: cur,
            to: next,
        });
        S::trigger_transition_variant(&mut commands, entity, cur, next);

        // Apply new state
        commands.entity(entity).insert(next);

        // Fire enter
        commands.trigger(Enter::<S> {
            entity,
            state: next,
        });
        S::trigger_enter_variant(&mut commands, entity, next);
    }
}

/// Generic plugin for FSM types that automatically sets up core observers.
///
/// This plugin automatically registers:
/// - `apply_state_request` - Handles state transition requests
/// - `on_fsm_added` - Fires Enter events when FSM component is first added
///
/// # Timing Warning
///
/// See [`on_fsm_added`] documentation for timing considerations and best practices.
///
/// # Example
/// ```
/// # use bevy::prelude::*;
/// # use bevy_fsm::{FSMState, FSMTransition, FSMPlugin, fsm_observer, Enter};
/// # use bevy_enum_event::{EnumEvent};
/// # #[derive(Component, EnumEvent, FSMTransition, FSMState, Reflect, Clone, Copy, Debug, PartialEq, Eq, Hash)]
/// # enum LifeFSM { Alive, Dying }
/// # fn on_dying_observer(_: On<Enter<life_fsm::Dying>>) {}
/// # let mut app = App::new();
/// app.add_plugins(FSMPlugin::<LifeFSM>::default());
///
/// // Register additional observers using fsm_observer! macro:
/// fsm_observer!(app, LifeFSM, on_dying_observer);
/// ```
pub struct FSMPlugin<S: FSMState + core::hash::Hash + Component> {
    /// If true, skip registering the `on_fsm_added` observer
    ignore_fsm_addition: bool,
    _phantom: std::marker::PhantomData<S>,
}

impl<S: FSMState + core::hash::Hash + Component> Default for FSMPlugin<S> {
    fn default() -> Self {
        Self {
            ignore_fsm_addition: false,
            _phantom: std::marker::PhantomData,
        }
    }
}

impl<S: FSMState + core::hash::Hash + Component> FSMPlugin<S> {
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

impl<S: FSMState + core::hash::Hash + Component + Reflect + GetTypeRegistration> Plugin
    for FSMPlugin<S>
{
    fn build(&self, app: &mut App) {
        // Register the FSM type for reflection
        app.register_type::<S>();
        {
            let world = app.world_mut();
            let group_entity = ensure_fsm_group::<S>(world);

            // Register core observers under the group entity
            let apply_entity = {
                let mut observer = world.add_observer(apply_state_request::<S>);
                observer.insert(Name::new("apply_state_request"));
                observer.insert(FSMObserverMarker::<S>::default());
                observer.id()
            };
            world.entity_mut(group_entity).add_child(apply_entity);

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

    #[derive(Component, Clone, Copy, Debug, Hash, PartialEq, Eq)]
    enum TestState {
        A,
        B,
        C,
    }

    impl FSMState for TestState {}

    impl FSMTransition for TestState {
        fn can_transition(from: Self, to: Self) -> bool {
            !(matches!(from, TestState::A) && matches!(to, TestState::C))
        }
    }

    #[derive(Resource, Default)]
    struct EventLog {
        enters: Vec<TestState>,
        exits: Vec<TestState>,
        transitions: Vec<(TestState, TestState)>,
    }

    #[allow(clippy::needless_pass_by_value)]
    fn on_enter(trigger: On<Enter<TestState>>, mut log: ResMut<EventLog>) {
        log.enters.push(trigger.event().state);
    }

    #[allow(clippy::needless_pass_by_value)]
    fn on_exit(trigger: On<Exit<TestState>>, mut log: ResMut<EventLog>) {
        log.exits.push(trigger.event().state);
    }

    #[allow(clippy::needless_pass_by_value)]
    fn on_transition(trigger: On<Transition<TestState, TestState>>, mut log: ResMut<EventLog>) {
        let event = trigger.event();
        log.transitions.push((event.from, event.to));
    }

    #[test]
    fn transitions_apply_and_fire_events() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.init_resource::<EventLog>();
        app.world_mut()
            .add_observer(apply_state_request::<TestState>);
        app.world_mut().add_observer(on_enter);
        app.world_mut().add_observer(on_exit);

        let e = app.world_mut().spawn(TestState::A).id();

        app.world_mut()
            .commands()
            .trigger(StateChangeRequest::<TestState> {
                entity: e,
                next: TestState::B,
            });

        app.update();

        assert_eq!(*app.world().get::<TestState>(e).unwrap(), TestState::B);
        let log = app.world().resource::<EventLog>();
        assert_eq!(log.exits, vec![TestState::A]);
        assert_eq!(log.enters, vec![TestState::B]);
    }

    #[test]
    fn guard_blocks_invalid_transitions() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.init_resource::<EventLog>();
        app.world_mut()
            .add_observer(apply_state_request::<TestState>);

        let e = app.world_mut().spawn(TestState::A).id();

        app.world_mut()
            .commands()
            .trigger(StateChangeRequest::<TestState> {
                entity: e,
                next: TestState::C,
            });

        app.update();

        assert_eq!(*app.world().get::<TestState>(e).unwrap(), TestState::A);
    }

    #[test]
    fn generic_transition_events_fire() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.init_resource::<EventLog>();
        app.world_mut()
            .add_observer(apply_state_request::<TestState>);
        app.world_mut().add_observer(on_transition);

        let e = app.world_mut().spawn(TestState::A).id();

        // Transition A -> B
        app.world_mut()
            .commands()
            .trigger(StateChangeRequest::<TestState> {
                entity: e,
                next: TestState::B,
            });
        app.update();

        let log = app.world().resource::<EventLog>();
        assert_eq!(log.transitions, vec![(TestState::A, TestState::B)]);
    }

    #[test]
    fn on_fsm_added_fires_initial_enter_events() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.init_resource::<EventLog>();
        app.world_mut()
            .add_observer(apply_state_request::<TestState>);
        app.world_mut().add_observer(on_fsm_added::<TestState>);
        app.world_mut().add_observer(on_enter);

        let _e = app.world_mut().spawn(TestState::A).id();
        app.update();

        let log = app.world().resource::<EventLog>();
        assert_eq!(log.enters, vec![TestState::A]);
    }

    // Additional tests without variant-specific events (to avoid proc macro issues in test modules)
    #[test]
    fn fsm_plugin_registers_core_observers_with_teststate() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.init_resource::<EventLog>();
        // Note: TestState doesn't have FSMState derive, so can't use FSMPlugin
        // This tests the manual approach still works
        app.world_mut()
            .add_observer(apply_state_request::<TestState>);
        app.world_mut().add_observer(on_fsm_added::<TestState>);
        app.world_mut().add_observer(on_enter);

        let e = app.world_mut().spawn(TestState::A).id();
        app.update();

        // Verify on_fsm_added was registered
        let log = app.world().resource::<EventLog>();
        assert_eq!(log.enters, vec![TestState::A]);

        // Verify apply_state_request was registered
        app.world_mut()
            .commands()
            .trigger(StateChangeRequest::<TestState> {
                entity: e,
                next: TestState::B,
            });
        app.update();

        assert_eq!(*app.world().get::<TestState>(e).unwrap(), TestState::B);
    }

    #[allow(clippy::too_many_lines, clippy::uninlined_format_args)]
    #[test]
    fn fsm_observer_macro_registers_and_organizes() {
        println!("\n=== TEST START: fsm_observer_macro_registers_and_organizes ===");

        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.init_resource::<EventLog>();
        app.world_mut()
            .add_observer(apply_state_request::<TestState>);
        println!("✓ App initialized with MinimalPlugins and apply_state_request observer");

        // Use the macro to register observer (should parent itself automatically)
        let observer_entity = fsm_observer!(app, TestState, on_enter).id();
        println!("✓ fsm_observer! macro called for TestState::on_enter");
        println!("  Observer entity created: {:?}", observer_entity);

        // Verify the observer entity was created with the correct components
        {
            let world = app.world();
            if let Some(name) = world.get::<Name>(observer_entity) {
                println!("  Observer entity has Name: '{}'", name.as_str());
            } else {
                println!("  WARNING: Observer entity has NO Name component!");
            }
            if world
                .get::<FSMObserverMarker<TestState>>(observer_entity)
                .is_some()
            {
                println!("  Observer entity has FSMObserverMarker<TestState>");
            } else {
                println!("  WARNING: Observer entity has NO FSMObserverMarker!");
            }

            // Check if the observer has ChildOf component (Bevy 0.17 hierarchy)
            if let Some(child_of) = world.get::<ChildOf>(observer_entity) {
                println!(
                    "  Observer entity has ChildOf component, parent: {:?}",
                    child_of.parent()
                );
            } else {
                println!("  WARNING: Observer entity has NO ChildOf component!");
                println!("  This indicates add_child() might not be working as expected!");
            }
        }

        println!("\n--- Checking if observer entity still exists BEFORE spawning test entity ---");
        {
            let world = app.world();
            if world.get_entity(observer_entity).is_ok() {
                println!("  ✓ Observer entity {:?} still exists", observer_entity);
            } else {
                println!(
                    "  ERROR: Observer entity {:?} already despawned!",
                    observer_entity
                );
            }
        }

        let e = app.world_mut().spawn(TestState::A).id();
        println!("✓ Entity spawned with TestState::A, entity id: {:?}", e);

        app.world_mut()
            .commands()
            .trigger(StateChangeRequest::<TestState> {
                entity: e,
                next: TestState::B,
            });
        println!("✓ StateChangeRequest triggered: A -> B");

        app.update();
        println!("✓ App updated (commands flushed)");

        println!("\n--- Checking if observer entity still exists AFTER app.update() ---");
        {
            let world = app.world();
            if world.get_entity(observer_entity).is_ok() {
                println!("  ✓ Observer entity {:?} still exists", observer_entity);
                if let Some(name) = world.get::<Name>(observer_entity) {
                    println!("    Name: '{}'", name.as_str());
                } else {
                    println!("    WARNING: Name component removed!");
                }
            } else {
                println!(
                    "  ERROR: Observer entity {:?} was despawned during app.update()!",
                    observer_entity
                );
            }
        }

        let log = app.world().resource::<EventLog>();
        println!(
            "✓ EventLog retrieved: enters={:?}, exits={:?}",
            log.enters, log.exits
        );
        assert_eq!(log.enters, vec![TestState::B]);

        // Check that FSMObservers root was created and hierarchy is correct
        println!("\n--- Checking FSMObservers root ---");
        let root = {
            let mut query = app
                .world_mut()
                .query_filtered::<(Entity, &Name), With<FSMObserversRoot>>();
            let roots: Vec<_> = query.iter(app.world()).collect();
            println!("Found {} FSMObserversRoot entities:", roots.len());
            for (entity, name) in &roots {
                println!("  - Entity {:?}, Name: '{}'", entity, name.as_str());
            }
            query
                .iter(app.world())
                .map(|(entity, _)| entity)
                .next()
                .expect("FSMObservers root entity should be created")
        };
        println!("✓ Root entity: {:?}", root);

        println!("\n--- Checking TestState group ---");
        let group = {
            let mut query = app
                .world_mut()
                .query_filtered::<(Entity, &Name), With<FSMObserverGroup<TestState>>>();
            let groups: Vec<_> = query.iter(app.world()).collect();
            println!(
                "Found {} FSMObserverGroup<TestState> entities:",
                groups.len()
            );
            for (entity, name) in &groups {
                println!("  - Entity {:?}, Name: '{}'", entity, name.as_str());
            }
            query
                .iter(app.world())
                .map(|(entity, _)| entity)
                .next()
                .expect("TestState group should exist")
        };
        println!("✓ Group entity: {:?}", group);

        println!("\n--- Checking ALL entities with Name component ---");
        {
            let mut query = app.world_mut().query::<(Entity, &Name)>();
            let all_named: Vec<_> = query.iter(app.world()).collect();
            println!("Found {} entities with Name component:", all_named.len());
            for (entity, name) in &all_named {
                println!("  - Entity {:?}, Name: '{}'", entity, name.as_str());
            }
        }

        println!("\n--- Searching specifically for 'on_enter' observer ---");
        {
            let mut query = app.world_mut().query::<(Entity, &Name)>();
            let on_enter_entities: Vec<_> = query
                .iter(app.world())
                .filter(|(_, name)| name.as_str() == "on_enter")
                .collect();
            println!(
                "Found {} entities named 'on_enter':",
                on_enter_entities.len()
            );
            for (entity, name) in &on_enter_entities {
                println!("  - Entity {:?}, Name: '{}'", entity, name.as_str());
            }
        }

        println!("\n--- Checking hierarchy relationships (ChildOf) ---");
        let mut group_is_child = false;
        let mut observer_is_child = false;
        {
            println!("Checking all entities with ChildOf component:");
            let mut child_count = 0;
            let mut query = app.world_mut().query::<(Entity, &ChildOf, Option<&Name>)>();
            for (entity_id, child_of, name) in query.iter(app.world()) {
                child_count += 1;
                let name_str = name.map_or("<no name>", Name::as_str);
                println!(
                    "  - Entity {:?}, Name: '{}', Parent: {:?}",
                    entity_id,
                    name_str,
                    child_of.parent()
                );
                if child_of.parent() == root && name_str == "TestState" {
                    println!("    ✓ MATCH: This is the TestState group as child of root");
                    group_is_child = true;
                }
                if child_of.parent() == group && name_str == "on_enter" {
                    println!("    ✓ MATCH: This is the on_enter observer as child of group");
                    observer_is_child = true;
                }
            }
            println!("Found {} entities with ChildOf component", child_count);
        }

        println!("\n--- ALL ENTITIES IN WORLD ---");
        {
            let mut count = 0;
            let mut query = app.world_mut().query::<(Entity, Option<&Name>)>();
            for (entity_id, name) in query.iter(app.world()) {
                count += 1;
                print!("  - Entity {:?}", entity_id);
                if let Some(name) = name {
                    print!(", Name: '{}'", name.as_str());
                }
                println!();
            }
            println!("  Total entities: {}", count);
        }

        println!("\n--- Final assertions ---");
        println!("group_is_child: {}", group_is_child);
        println!("observer_is_child: {}", observer_is_child);

        assert!(
            group_is_child,
            "TestState group should be child of FSMObservers"
        );
        assert!(
            observer_is_child,
            "Observer should be parented under the TestState group"
        );

        println!("\n=== TEST END ===\n");
    }

    #[test]
    fn fsm_config_whitelist_mode() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.world_mut()
            .add_observer(apply_state_request::<TestState>);

        // Whitelist A->C (which FSMTransition normally forbids)
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

        // Whitelisted transition should succeed
        app.world_mut()
            .commands()
            .trigger(StateChangeRequest::<TestState> {
                entity: e,
                next: TestState::B,
            });
        app.update();
        assert_eq!(*app.world().get::<TestState>(e).unwrap(), TestState::B);

        // Reset to A
        app.world_mut().entity_mut(e).insert(TestState::A);

        // A->C is whitelisted, should succeed even though FSMTransition would block it
        app.world_mut()
            .commands()
            .trigger(StateChangeRequest::<TestState> {
                entity: e,
                next: TestState::C,
            });
        app.update();
        assert_eq!(
            *app.world().get::<TestState>(e).unwrap(),
            TestState::C,
            "Whitelist should override FSMTransition when call_rules is false"
        );
    }

    #[test]
    fn fsm_config_whitelist_with_rules() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.world_mut()
            .add_observer(apply_state_request::<TestState>);

        // Whitelist A->C (overrides FSMTransition), with_rules checks others
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

        // A->C: ON whitelist, should ACCEPT (config wins, FSMTransition NOT checked)
        app.world_mut()
            .commands()
            .trigger(StateChangeRequest::<TestState> {
                entity: e,
                next: TestState::C,
            });
        app.update();
        assert_eq!(
            *app.world().get::<TestState>(e).unwrap(),
            TestState::C,
            "Whitelisted transition should accept regardless of FSMTransition"
        );

        // Reset to A
        app.world_mut().entity_mut(e).insert(TestState::A);

        // A->B: NOT on whitelist, check FSMTransition (allows it)
        app.world_mut()
            .commands()
            .trigger(StateChangeRequest::<TestState> {
                entity: e,
                next: TestState::B,
            });
        app.update();
        assert_eq!(
            *app.world().get::<TestState>(e).unwrap(),
            TestState::B,
            "Non-whitelisted but FSMTransition-valid transition should succeed with with_rules"
        );

        // B->C: NOT on whitelist, check FSMTransition (allows it)
        app.world_mut()
            .commands()
            .trigger(StateChangeRequest::<TestState> {
                entity: e,
                next: TestState::C,
            });
        app.update();
        assert_eq!(
            *app.world().get::<TestState>(e).unwrap(),
            TestState::C,
            "Non-whitelisted transition should defer to FSMTransition when with_rules enabled"
        );
    }

    #[test]
    fn fsm_config_blacklist_mode() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.world_mut()
            .add_observer(apply_state_request::<TestState>);

        let e = app
            .world_mut()
            .spawn((
                TestState::A,
                FSMOverride::blacklist([(TestState::A, TestState::C)]),
            ))
            .id();

        // Non-blacklisted transition should succeed (even if FSMTransition would block it)
        app.world_mut()
            .commands()
            .trigger(StateChangeRequest::<TestState> {
                entity: e,
                next: TestState::B,
            });
        app.update();
        assert_eq!(*app.world().get::<TestState>(e).unwrap(), TestState::B);

        // Reset to A
        app.world_mut().entity_mut(e).insert(TestState::A);

        // Blacklisted transition should fail
        app.world_mut()
            .commands()
            .trigger(StateChangeRequest::<TestState> {
                entity: e,
                next: TestState::C,
            });
        app.update();
        assert_eq!(
            *app.world().get::<TestState>(e).unwrap(),
            TestState::A,
            "Blacklisted transition should be denied"
        );
    }

    #[test]
    fn fsm_config_blacklist_with_rules() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.world_mut()
            .add_observer(apply_state_request::<TestState>);

        // Blacklist B->C, but A->C is also invalid per FSMTransition
        let e = app
            .world_mut()
            .spawn((
                TestState::A,
                FSMOverride::blacklist([(TestState::B, TestState::C)]).with_rules(),
            ))
            .id();

        // A->C: not blacklisted, but FSMTransition blocks it
        app.world_mut()
            .commands()
            .trigger(StateChangeRequest::<TestState> {
                entity: e,
                next: TestState::C,
            });
        app.update();
        assert_eq!(
            *app.world().get::<TestState>(e).unwrap(),
            TestState::A,
            "FSMTransition should block A->C even though not blacklisted"
        );

        // A->B: allowed by both blacklist and FSMTransition
        app.world_mut()
            .commands()
            .trigger(StateChangeRequest::<TestState> {
                entity: e,
                next: TestState::B,
            });
        app.update();
        assert_eq!(*app.world().get::<TestState>(e).unwrap(), TestState::B);

        // B->C: blacklisted, should fail even though FSMTransition would allow it
        app.world_mut()
            .commands()
            .trigger(StateChangeRequest::<TestState> {
                entity: e,
                next: TestState::C,
            });
        app.update();
        assert_eq!(
            *app.world().get::<TestState>(e).unwrap(),
            TestState::B,
            "Blacklist should block B->C"
        );
    }

    #[test]
    fn fsm_config_deny_all_mode() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.world_mut()
            .add_observer(apply_state_request::<TestState>);

        let e = app
            .world_mut()
            .spawn((TestState::A, FSMOverride::<TestState>::deny_all()))
            .id();

        // All transitions should be denied
        app.world_mut()
            .commands()
            .trigger(StateChangeRequest::<TestState> {
                entity: e,
                next: TestState::B,
            });
        app.update();
        assert_eq!(*app.world().get::<TestState>(e).unwrap(), TestState::A);
    }

    #[test]
    fn fsm_config_allow_all_mode() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.world_mut()
            .add_observer(apply_state_request::<TestState>);

        let e = app
            .world_mut()
            .spawn((TestState::A, FSMOverride::<TestState>::allow_all()))
            .id();

        // Without call_rules, FSMTransition is bypassed - ALL transitions allowed
        app.world_mut()
            .commands()
            .trigger(StateChangeRequest::<TestState> {
                entity: e,
                next: TestState::B,
            });
        app.update();
        assert_eq!(*app.world().get::<TestState>(e).unwrap(), TestState::B);

        // Reset to A
        app.world_mut().entity_mut(e).insert(TestState::A);

        // Even invalid transition (A->C) is allowed because FSMTransition is not checked
        app.world_mut()
            .commands()
            .trigger(StateChangeRequest::<TestState> {
                entity: e,
                next: TestState::C,
            });
        app.update();
        assert_eq!(
            *app.world().get::<TestState>(e).unwrap(),
            TestState::C,
            "allow_all without call_rules should bypass FSMTransition"
        );
    }

    #[test]
    fn fsm_config_allow_all_with_rules() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.world_mut()
            .add_observer(apply_state_request::<TestState>);

        let e = app
            .world_mut()
            .spawn((
                TestState::A,
                FSMOverride::<TestState>::allow_all().with_rules(),
            ))
            .id();

        // Valid transition should succeed
        app.world_mut()
            .commands()
            .trigger(StateChangeRequest::<TestState> {
                entity: e,
                next: TestState::B,
            });
        app.update();
        assert_eq!(*app.world().get::<TestState>(e).unwrap(), TestState::B);

        // Reset to A
        app.world_mut().entity_mut(e).insert(TestState::A);

        // Invalid transition (A->C) should be blocked by FSMTransition
        app.world_mut()
            .commands()
            .trigger(StateChangeRequest::<TestState> {
                entity: e,
                next: TestState::C,
            });
        app.update();
        assert_eq!(
            *app.world().get::<TestState>(e).unwrap(),
            TestState::A,
            "allow_all with call_rules should enforce FSMTransition"
        );
    }

    // Test with FSMPlugin using a real FSMState enum
    #[derive(Component, Reflect, Clone, Copy, Debug, PartialEq, Eq, Hash)]
    #[reflect(Component)]
    enum PluginTestState {
        Initial,
        Active,
        Done,
    }

    impl FSMState for PluginTestState {}

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

    #[allow(clippy::needless_pass_by_value)]
    fn on_plugin_enter(trigger: On<Enter<PluginTestState>>, mut log: ResMut<PluginEventLog>) {
        log.enters.push(trigger.event().state);
    }

    #[test]
    fn fsm_plugin_fires_initial_enter_event() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.init_resource::<PluginEventLog>();

        // Use FSMPlugin to register observers automatically
        app.add_plugins(FSMPlugin::<PluginTestState>::default());

        // Register observer to track Enter events
        app.world_mut().add_observer(on_plugin_enter);

        // Spawn entity with initial state
        let entity = app.world_mut().spawn(PluginTestState::Initial).id();

        // Update to process the on_fsm_added observer
        app.update();

        // Verify that Enter event was fired for initial state
        let log = app.world().resource::<PluginEventLog>();
        assert_eq!(
            log.enters,
            vec![PluginTestState::Initial],
            "FSMPlugin should fire Enter event for initial state when entity is spawned"
        );

        // Verify that state transitions still work
        app.world_mut()
            .commands()
            .trigger(StateChangeRequest::<PluginTestState> {
                entity,
                next: PluginTestState::Active,
            });
        app.update();

        assert_eq!(
            *app.world().get::<PluginTestState>(entity).unwrap(),
            PluginTestState::Active
        );

        let log = app.world().resource::<PluginEventLog>();
        assert_eq!(
            log.enters,
            vec![PluginTestState::Initial, PluginTestState::Active],
            "FSMPlugin should fire Enter events for both initial state and transitions"
        );
    }
}
