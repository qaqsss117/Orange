use serde::{Deserialize, Serialize};

use crate::{CommandError, ControlPlaneState, DOMAIN_SCHEMA_VERSION, DataPlaneState, ErrorCode};

pub const GET_PLANE_STATE_COMMAND: &str = "get_plane_state";
pub const GET_RUNTIME_INFO_COMMAND: &str = "get_runtime_info";
pub const REGISTERED_COMMANDS: &[&str] = &[GET_PLANE_STATE_COMMAND, GET_RUNTIME_INFO_COMMAND];

pub fn is_registered_command(command: &str) -> bool {
    REGISTERED_COMMANDS.contains(&command)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeInfoRequest {
    pub schema_version: u16,
}

impl RuntimeInfoRequest {
    pub const fn current() -> Self {
        Self {
            schema_version: DOMAIN_SCHEMA_VERSION,
        }
    }

    pub fn validate(self) -> Result<Self, CommandError> {
        if self.schema_version != DOMAIN_SCHEMA_VERSION {
            return Err(CommandError::from_code(ErrorCode::Validation));
        }

        Ok(self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeInfoResponse {
    pub schema_version: u16,
    pub product_name: String,
    pub product_version: String,
}

impl RuntimeInfoResponse {
    pub fn new(product_version: impl Into<String>) -> Self {
        Self {
            schema_version: DOMAIN_SCHEMA_VERSION,
            product_name: "Orange".to_owned(),
            product_version: product_version.into(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PlaneStateRequest {
    pub schema_version: u16,
}

impl PlaneStateRequest {
    pub const fn current() -> Self {
        Self {
            schema_version: DOMAIN_SCHEMA_VERSION,
        }
    }

    pub fn validate(self) -> Result<Self, CommandError> {
        if self.schema_version != DOMAIN_SCHEMA_VERSION {
            return Err(CommandError::from_code(ErrorCode::Validation));
        }
        Ok(self)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaneStateResponse {
    pub schema_version: u16,
    pub control_plane: ControlPlaneState,
    pub data_plane: DataPlaneState,
}

impl PlaneStateResponse {
    pub const fn new(control_plane: ControlPlaneState, data_plane: DataPlaneState) -> Self {
        Self {
            schema_version: DOMAIN_SCHEMA_VERSION,
            control_plane,
            data_plane,
        }
    }
}
