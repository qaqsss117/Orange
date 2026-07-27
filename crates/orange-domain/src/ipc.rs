use serde::{Deserialize, Serialize};

use crate::{CommandError, DOMAIN_SCHEMA_VERSION, ErrorCode};

pub const GET_RUNTIME_INFO_COMMAND: &str = "get_runtime_info";
pub const REGISTERED_COMMANDS: &[&str] = &[GET_RUNTIME_INFO_COMMAND];

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
