use std::fmt;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionMode {
    #[default]
    SystemProxy,
    Tun,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoutingMode {
    #[default]
    Smart,
    Global,
    Direct,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ControlPlaneState {
    Cold,
    Decrypting,
    Starting,
    Ready,
    Degraded,
    Failed,
    Stopping,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DataPlaneState {
    Unconfigured,
    Validating,
    PermissionRequired,
    Starting,
    Online,
    Stopping,
    Failed,
    Rollback,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransitionOutcome {
    Changed,
    Unchanged,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StateTransitionError {
    Control {
        from: ControlPlaneState,
        to: ControlPlaneState,
    },
    Data {
        from: DataPlaneState,
        to: DataPlaneState,
    },
}

impl fmt::Display for StateTransitionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Control { from, to } => {
                write!(
                    formatter,
                    "invalid Control Plane transition: {from:?} -> {to:?}"
                )
            }
            Self::Data { from, to } => {
                write!(
                    formatter,
                    "invalid Data Plane transition: {from:?} -> {to:?}"
                )
            }
        }
    }
}

impl std::error::Error for StateTransitionError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ControlPlaneStateMachine {
    state: ControlPlaneState,
}

impl Default for ControlPlaneStateMachine {
    fn default() -> Self {
        Self {
            state: ControlPlaneState::Cold,
        }
    }
}

impl ControlPlaneStateMachine {
    pub const fn state(&self) -> ControlPlaneState {
        self.state
    }

    pub fn transition(
        &mut self,
        next: ControlPlaneState,
    ) -> Result<TransitionOutcome, StateTransitionError> {
        if self.state == next {
            return Ok(TransitionOutcome::Unchanged);
        }
        if !control_transition_allowed(self.state, next) {
            return Err(StateTransitionError::Control {
                from: self.state,
                to: next,
            });
        }
        self.state = next;
        Ok(TransitionOutcome::Changed)
    }

    pub fn restore_authoritative(&mut self, state: ControlPlaneState) {
        self.state = state;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DataPlaneStateMachine {
    state: DataPlaneState,
}

impl Default for DataPlaneStateMachine {
    fn default() -> Self {
        Self {
            state: DataPlaneState::Unconfigured,
        }
    }
}

impl DataPlaneStateMachine {
    pub const fn state(&self) -> DataPlaneState {
        self.state
    }

    pub fn transition(
        &mut self,
        next: DataPlaneState,
    ) -> Result<TransitionOutcome, StateTransitionError> {
        if self.state == next {
            return Ok(TransitionOutcome::Unchanged);
        }
        if !data_transition_allowed(self.state, next) {
            return Err(StateTransitionError::Data {
                from: self.state,
                to: next,
            });
        }
        self.state = next;
        Ok(TransitionOutcome::Changed)
    }

    pub fn restore_authoritative(&mut self, state: DataPlaneState) {
        self.state = state;
    }
}

const fn control_transition_allowed(from: ControlPlaneState, to: ControlPlaneState) -> bool {
    use ControlPlaneState::{Cold, Decrypting, Degraded, Failed, Ready, Starting, Stopping};

    matches!(
        (from, to),
        (Cold, Decrypting)
            | (Decrypting, Starting | Failed | Stopping)
            | (Starting, Ready | Degraded | Failed | Stopping)
            | (Ready, Degraded | Failed | Stopping)
            | (Degraded, Ready | Failed | Stopping)
            | (Failed, Cold | Decrypting | Stopping)
            | (Stopping, Cold | Failed)
    )
}

const fn data_transition_allowed(from: DataPlaneState, to: DataPlaneState) -> bool {
    use DataPlaneState::{
        Failed, Online, PermissionRequired, Rollback, Starting, Stopping, Unconfigured, Validating,
    };

    matches!(
        (from, to),
        (Unconfigured, Validating)
            | (
                Validating,
                PermissionRequired | Starting | Online | Stopping | Failed | Unconfigured
            )
            | (
                PermissionRequired,
                Validating | Stopping | Rollback | Unconfigured
            )
            | (Starting, Online | Stopping | Failed | Rollback)
            | (Online, Validating | Stopping | Failed | Rollback)
            | (Stopping, PermissionRequired | Unconfigured | Failed)
            | (Failed, Validating | Rollback | Stopping | Unconfigured)
            | (
                Rollback,
                PermissionRequired | Starting | Online | Stopping | Failed | Unconfigured
            )
    )
}
