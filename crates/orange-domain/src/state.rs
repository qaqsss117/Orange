use std::fmt;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionMode {
    SystemProxy,
    Tun,
}

impl Default for ConnectionMode {
    fn default() -> Self {
        Self::SystemProxy
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn control_plane_happy_path_and_recovery_are_explicit() {
        let mut machine = ControlPlaneStateMachine::default();
        assert_eq!(machine.state(), ControlPlaneState::Cold);
        for state in [
            ControlPlaneState::Decrypting,
            ControlPlaneState::Starting,
            ControlPlaneState::Ready,
            ControlPlaneState::Degraded,
            ControlPlaneState::Ready,
            ControlPlaneState::Stopping,
            ControlPlaneState::Cold,
        ] {
            assert_eq!(machine.transition(state), Ok(TransitionOutcome::Changed));
        }
    }

    #[test]
    fn invalid_transition_does_not_mutate_state() {
        let mut control = ControlPlaneStateMachine::default();
        assert!(control.transition(ControlPlaneState::Ready).is_err());
        assert_eq!(control.state(), ControlPlaneState::Cold);

        let mut data = DataPlaneStateMachine::default();
        assert!(data.transition(DataPlaneState::Online).is_err());
        assert_eq!(data.state(), DataPlaneState::Unconfigured);
    }

    #[test]
    fn repeated_state_transition_is_idempotent() {
        let mut machine = DataPlaneStateMachine::default();
        assert_eq!(
            machine.transition(DataPlaneState::Unconfigured),
            Ok(TransitionOutcome::Unchanged)
        );
    }

    #[test]
    fn state_wire_names_are_stable() {
        assert_eq!(
            serde_json::to_string(&ConnectionMode::SystemProxy).unwrap(),
            "\"system_proxy\""
        );
        assert_eq!(
            serde_json::to_string(&ControlPlaneState::Decrypting).unwrap(),
            "\"decrypting\""
        );
        assert_eq!(
            serde_json::to_string(&DataPlaneState::PermissionRequired).unwrap(),
            "\"permission_required\""
        );
        assert!(serde_json::from_str::<DataPlaneState>("\"future\"").is_err());
    }
}
