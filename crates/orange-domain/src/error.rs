use serde::{Deserialize, Deserializer, Serialize, de::Error as _};

use crate::DOMAIN_SCHEMA_VERSION;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    Validation,
    ProxyPortInUse,
    Permission,
    Network,
    Bootstrap,
    Subscription,
    Service,
    Timeout,
    Cancelled,
    Internal,
}

impl ErrorCode {
    pub const ALL: [Self; 10] = [
        Self::Validation,
        Self::ProxyPortInUse,
        Self::Permission,
        Self::Network,
        Self::Bootstrap,
        Self::Subscription,
        Self::Service,
        Self::Timeout,
        Self::Cancelled,
        Self::Internal,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Validation => "validation",
            Self::ProxyPortInUse => "proxy_port_in_use",
            Self::Permission => "permission",
            Self::Network => "network",
            Self::Bootstrap => "bootstrap",
            Self::Subscription => "subscription",
            Self::Service => "service",
            Self::Timeout => "timeout",
            Self::Cancelled => "cancelled",
            Self::Internal => "internal",
        }
    }

    pub const fn public_message(self) -> &'static str {
        match self {
            Self::Validation => "请求参数无效。",
            Self::ProxyPortInUse => "该代理端口已被占用，请更换端口。",
            Self::Permission => "当前操作未获授权。",
            Self::Network => "网络请求失败，请稍后重试。",
            Self::Bootstrap => "安全连接初始化失败。",
            Self::Subscription => "订阅数据不可用。",
            Self::Service => "系统服务暂不可用。",
            Self::Timeout => "操作超时，请重试。",
            Self::Cancelled => "操作已取消。",
            Self::Internal => "发生内部错误。",
        }
    }

    pub const fn retryable(self) -> bool {
        matches!(
            self,
            Self::Network | Self::Bootstrap | Self::Service | Self::Timeout
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandError {
    schema_version: u16,
    code: ErrorCode,
    message: String,
    retryable: bool,
}

impl CommandError {
    pub fn from_code(code: ErrorCode) -> Self {
        Self {
            schema_version: DOMAIN_SCHEMA_VERSION,
            code,
            message: code.public_message().to_owned(),
            retryable: code.retryable(),
        }
    }

    pub const fn schema_version(&self) -> u16 {
        self.schema_version
    }

    pub const fn code(&self) -> ErrorCode {
        self.code
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    pub const fn retryable(&self) -> bool {
        self.retryable
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CommandErrorWire {
    schema_version: u16,
    code: ErrorCode,
    message: String,
    retryable: bool,
}

impl<'de> Deserialize<'de> for CommandError {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = CommandErrorWire::deserialize(deserializer)?;
        let canonical = Self::from_code(wire.code);

        if wire.schema_version != canonical.schema_version
            || wire.message != canonical.message
            || wire.retryable != canonical.retryable
        {
            return Err(D::Error::custom("invalid command error contract"));
        }

        Ok(canonical)
    }
}
