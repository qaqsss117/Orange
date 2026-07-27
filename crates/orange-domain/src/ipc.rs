use std::fmt;

use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, ZeroizeOnDrop};

use crate::{
    BUSINESS_API_SCHEMA_VERSION, CommandError, ControlPlaneState, DOMAIN_SCHEMA_VERSION,
    DataPlaneState, ErrorCode, LoginRequest, RegisterRequest,
};

pub const GET_PLANE_STATE_COMMAND: &str = "get_plane_state";
pub const GET_RUNTIME_INFO_COMMAND: &str = "get_runtime_info";
pub const INITIALIZE_BUSINESS_COMMAND: &str = "initialize_business";
pub const LOGIN_COMMAND: &str = "login";
pub const REGISTER_COMMAND: &str = "register";
pub const GET_AUTH_SESSION_COMMAND: &str = "get_auth_session";
pub const BASE_COMMANDS: &[&str] = &[GET_PLANE_STATE_COMMAND, GET_RUNTIME_INFO_COMMAND];
pub const DESKTOP_BUSINESS_COMMANDS: &[&str] = &[
    INITIALIZE_BUSINESS_COMMAND,
    LOGIN_COMMAND,
    REGISTER_COMMAND,
    GET_AUTH_SESSION_COMMAND,
];
pub const REGISTERED_COMMANDS: &[&str] = &[
    GET_PLANE_STATE_COMMAND,
    GET_RUNTIME_INFO_COMMAND,
    INITIALIZE_BUSINESS_COMMAND,
    LOGIN_COMMAND,
    REGISTER_COMMAND,
    GET_AUTH_SESSION_COMMAND,
];

pub fn is_registered_command(command: &str) -> bool {
    REGISTERED_COMMANDS.contains(&command)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InitializeBusinessRequest {
    pub schema_version: u16,
}

impl InitializeBusinessRequest {
    pub const fn current() -> Self {
        Self {
            schema_version: DOMAIN_SCHEMA_VERSION,
        }
    }

    pub fn validate(self) -> Result<Self, CommandError> {
        validate_schema_version(self.schema_version)?;
        Ok(self)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AuthSessionRequest {
    pub schema_version: u16,
}

impl AuthSessionRequest {
    pub const fn current() -> Self {
        Self {
            schema_version: DOMAIN_SCHEMA_VERSION,
        }
    }

    pub fn validate(self) -> Result<Self, CommandError> {
        validate_schema_version(self.schema_version)?;
        Ok(self)
    }
}

#[derive(PartialEq, Eq, Serialize, Deserialize, Zeroize, ZeroizeOnDrop)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LoginCommandRequest {
    #[zeroize(skip)]
    pub schema_version: u16,
    pub email: String,
    pub password: String,
}

impl LoginCommandRequest {
    pub fn validate(mut self) -> Result<LoginRequest, CommandError> {
        validate_schema_version(self.schema_version)?;
        Ok(LoginRequest {
            schema_version: BUSINESS_API_SCHEMA_VERSION,
            email: std::mem::take(&mut self.email),
            password: std::mem::take(&mut self.password),
        })
    }
}

impl fmt::Debug for LoginCommandRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("LoginCommandRequest")
            .field("schema_version", &self.schema_version)
            .field("email_bytes", &self.email.len())
            .field("password_bytes", &self.password.len())
            .finish()
    }
}

#[derive(PartialEq, Eq, Serialize, Deserialize, Zeroize, ZeroizeOnDrop)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RegisterCommandRequest {
    #[zeroize(skip)]
    pub schema_version: u16,
    pub email: String,
    pub password: String,
    pub invite_code: Option<String>,
}

impl RegisterCommandRequest {
    pub fn validate(mut self) -> Result<RegisterRequest, CommandError> {
        validate_schema_version(self.schema_version)?;
        Ok(RegisterRequest {
            schema_version: BUSINESS_API_SCHEMA_VERSION,
            email: std::mem::take(&mut self.email),
            password: std::mem::take(&mut self.password),
            invite_code: std::mem::take(&mut self.invite_code),
        })
    }
}

impl fmt::Debug for RegisterCommandRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RegisterCommandRequest")
            .field("schema_version", &self.schema_version)
            .field("email_bytes", &self.email.len())
            .field("password_bytes", &self.password.len())
            .field("has_invite_code", &self.invite_code.is_some())
            .finish()
    }
}

fn validate_schema_version(schema_version: u16) -> Result<(), CommandError> {
    if schema_version != DOMAIN_SCHEMA_VERSION {
        return Err(CommandError::from_code(ErrorCode::Validation));
    }
    Ok(())
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
        validate_schema_version(self.schema_version)?;
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
        validate_schema_version(self.schema_version)?;
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
