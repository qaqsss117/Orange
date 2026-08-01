use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, de::Error as _};
use zeroize::{Zeroize, ZeroizeOnDrop};

pub const BUSINESS_API_SCHEMA_VERSION: u16 = 1;
const MAX_JAVASCRIPT_SAFE_INTEGER: u64 = 9_007_199_254_740_991;

fn deserialize_schema_version<'de, D>(deserializer: D) -> Result<u16, D::Error>
where
    D: Deserializer<'de>,
{
    let value = u16::deserialize(deserializer)?;
    if value != BUSINESS_API_SCHEMA_VERSION {
        return Err(D::Error::custom("unsupported business API schema version"));
    }
    Ok(value)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(transparent)]
pub struct SafeInteger(u64);

impl SafeInteger {
    pub const fn new(value: u64) -> Option<Self> {
        if value <= MAX_JAVASCRIPT_SAFE_INTEGER {
            Some(Self(value))
        } else {
            None
        }
    }

    pub const fn get(self) -> u64 {
        self.0
    }

    pub const fn checked_add(self, other: Self) -> Option<Self> {
        match self.0.checked_add(other.0) {
            Some(value) => Self::new(value),
            None => None,
        }
    }

    pub const fn saturating_sub(self, other: Self) -> Self {
        Self(self.0.saturating_sub(other.0))
    }
}

impl<'de> Deserialize<'de> for SafeInteger {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = u64::deserialize(deserializer)?;
        Self::new(value).ok_or_else(|| D::Error::custom("integer exceeds JavaScript-safe range"))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(transparent)]
pub struct UnixMillis(SafeInteger);

impl UnixMillis {
    pub const fn new(value: u64) -> Option<Self> {
        match SafeInteger::new(value) {
            Some(value) => Some(Self(value)),
            None => None,
        }
    }

    pub const fn get(self) -> u64 {
        self.0.get()
    }
}

impl<'de> Deserialize<'de> for UnixMillis {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Ok(Self(SafeInteger::deserialize(deserializer)?))
    }
}

#[derive(Clone, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct CurrencyCode(String);

impl CurrencyCode {
    pub fn new(value: impl Into<String>) -> Option<Self> {
        let value = value.into();
        (value.len() == 3 && value.bytes().all(|byte| byte.is_ascii_uppercase()))
            .then_some(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for CurrencyCode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for CurrencyCode {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        if value.len() != 3 || !value.bytes().all(|byte| byte.is_ascii_uppercase()) {
            return Err(D::Error::custom(
                "currency must be three uppercase ASCII letters",
            ));
        }
        Ok(Self(value))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Money {
    pub minor_units: SafeInteger,
    pub currency: CurrencyCode,
}

macro_rules! status_enum {
    ($name:ident { $($variant:ident),+ $(,)? }) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
        #[serde(rename_all = "snake_case")]
        pub enum $name {
            $($variant,)+
            #[serde(other)]
            Unknown,
        }
    };
}

status_enum!(AccountStatus { Active, Disabled });
status_enum!(SubscriptionStatus {
    None,
    Trial,
    Active,
    Expired,
    Exhausted,
});
status_enum!(OrderStatus {
    Pending,
    Paid,
    Cancelled,
    Closed,
    Refunded,
});
status_enum!(PaymentStatus {
    Unavailable,
    Pending,
    Ready,
    Expired,
});
status_enum!(TicketStatus {
    Open,
    Answered,
    Closed,
});

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UserProfile {
    pub user_id: String,
    pub email: String,
    pub status: AccountStatus,
    pub balance: Money,
}

#[derive(PartialEq, Eq, Serialize, Deserialize, Zeroize, ZeroizeOnDrop)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LoginRequest {
    #[zeroize(skip)]
    #[serde(deserialize_with = "deserialize_schema_version")]
    pub schema_version: u16,
    pub email: String,
    pub password: String,
}

impl fmt::Debug for LoginRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("LoginRequest")
            .field("schema_version", &self.schema_version)
            .field("email_bytes", &self.email.len())
            .field("password_bytes", &self.password.len())
            .finish()
    }
}

#[derive(PartialEq, Eq, Serialize, Deserialize, Zeroize, ZeroizeOnDrop)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RegisterRequest {
    #[zeroize(skip)]
    #[serde(deserialize_with = "deserialize_schema_version")]
    pub schema_version: u16,
    pub email: String,
    pub password: String,
    pub invite_code: Option<String>,
}

impl fmt::Debug for RegisterRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RegisterRequest")
            .field("schema_version", &self.schema_version)
            .field("email_bytes", &self.email.len())
            .field("password_bytes", &self.password.len())
            .field("has_invite_code", &self.invite_code.is_some())
            .finish()
    }
}

#[derive(PartialEq, Eq, Serialize, Deserialize, Zeroize, ZeroizeOnDrop)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CredentialBundle {
    pub access_token: String,
    pub refresh_token: String,
    #[zeroize(skip)]
    pub expires_at_unix_ms: UnixMillis,
}

impl CredentialBundle {
    pub fn with_access_token<R>(&self, consume: impl FnOnce(&str) -> R) -> R {
        consume(&self.access_token)
    }

    pub fn with_refresh_token<R>(&self, consume: impl FnOnce(&str) -> R) -> R {
        consume(&self.refresh_token)
    }
}

impl fmt::Debug for CredentialBundle {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CredentialBundle")
            .field("access_token", &"<redacted>")
            .field("refresh_token", &"<redacted>")
            .field("expires_at_unix_ms", &self.expires_at_unix_ms)
            .finish()
    }
}

#[derive(PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AuthWireResponse {
    #[serde(deserialize_with = "deserialize_schema_version")]
    pub schema_version: u16,
    pub credentials: CredentialBundle,
    pub user: UserProfile,
}

impl fmt::Debug for AuthWireResponse {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AuthWireResponse")
            .field("schema_version", &self.schema_version)
            .field("credentials", &self.credentials)
            .field("user_status", &self.user.status)
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AuthPublicResponse {
    #[serde(deserialize_with = "deserialize_schema_version")]
    pub schema_version: u16,
    pub authenticated: bool,
    pub user: UserProfile,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthSessionStatus {
    SignedOut,
    Authenticated,
    Unverified,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthSessionResponse {
    #[serde(deserialize_with = "deserialize_schema_version")]
    pub schema_version: u16,
    pub status: AuthSessionStatus,
    pub user: Option<UserProfile>,
}

impl AuthSessionResponse {
    pub const fn signed_out() -> Self {
        Self {
            schema_version: BUSINESS_API_SCHEMA_VERSION,
            status: AuthSessionStatus::SignedOut,
            user: None,
        }
    }

    pub fn authenticated(user: UserProfile) -> Self {
        Self {
            schema_version: BUSINESS_API_SCHEMA_VERSION,
            status: AuthSessionStatus::Authenticated,
            user: Some(user),
        }
    }

    pub fn unverified(user: Option<UserProfile>) -> Self {
        Self {
            schema_version: BUSINESS_API_SCHEMA_VERSION,
            status: AuthSessionStatus::Unverified,
            user,
        }
    }
}

#[derive(PartialEq, Eq, Serialize, Deserialize, Zeroize, ZeroizeOnDrop)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConfigWireResponse {
    #[zeroize(skip)]
    #[serde(deserialize_with = "deserialize_schema_version")]
    pub schema_version: u16,
    pub minimum_supported_version: String,
    #[zeroize(skip)]
    pub maintenance: bool,
    pub notice: Option<String>,
    #[zeroize(skip)]
    pub registration_requires_invite: bool,
    pub api_base_url: String,
    pub payment_base_url: String,
    pub support_url: String,
    pub banner_url: Option<String>,
}

impl fmt::Debug for ConfigWireResponse {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ConfigWireResponse")
            .field("schema_version", &self.schema_version)
            .field("minimum_supported_version", &self.minimum_supported_version)
            .field("maintenance", &self.maintenance)
            .field("has_notice", &self.notice.is_some())
            .field(
                "registration_requires_invite",
                &self.registration_requires_invite,
            )
            .field("has_api_base_url", &!self.api_base_url.is_empty())
            .field("has_payment_base_url", &!self.payment_base_url.is_empty())
            .field("has_support_url", &!self.support_url.is_empty())
            .field("has_banner_url", &self.banner_url.is_some())
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConfigResponse {
    #[serde(deserialize_with = "deserialize_schema_version")]
    pub schema_version: u16,
    pub minimum_supported_version: String,
    pub maintenance: bool,
    pub notice: Option<String>,
    pub registration_requires_invite: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BusinessInitializationResponse {
    #[serde(deserialize_with = "deserialize_schema_version")]
    pub schema_version: u16,
    pub config: ConfigResponse,
    pub session: AuthSessionResponse,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AccountResponse {
    #[serde(deserialize_with = "deserialize_schema_version")]
    pub schema_version: u16,
    pub user: UserProfile,
}

#[derive(PartialEq, Eq, Serialize, Deserialize, Zeroize, ZeroizeOnDrop)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SubscriptionWireResponse {
    #[zeroize(skip)]
    #[serde(deserialize_with = "deserialize_schema_version")]
    pub schema_version: u16,
    #[zeroize(skip)]
    pub status: SubscriptionStatus,
    pub plan_id: Option<String>,
    #[zeroize(skip)]
    pub expires_at_unix_ms: Option<UnixMillis>,
    #[zeroize(skip)]
    pub used_bytes: SafeInteger,
    #[zeroize(skip)]
    pub total_bytes: Option<SafeInteger>,
    pub subscription_credential: String,
}

impl SubscriptionWireResponse {
    pub fn with_credential<R>(&self, consume: impl FnOnce(&str) -> R) -> R {
        consume(&self.subscription_credential)
    }
}

impl fmt::Debug for SubscriptionWireResponse {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SubscriptionWireResponse")
            .field("schema_version", &self.schema_version)
            .field("status", &self.status)
            .field("has_plan", &self.plan_id.is_some())
            .field("expires_at_unix_ms", &self.expires_at_unix_ms)
            .field("used_bytes", &self.used_bytes)
            .field("total_bytes", &self.total_bytes)
            .field("subscription_credential", &"<redacted>")
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SubscriptionPublicResponse {
    #[serde(deserialize_with = "deserialize_schema_version")]
    pub schema_version: u16,
    pub status: SubscriptionStatus,
    pub plan_id: Option<String>,
    pub expires_at_unix_ms: Option<UnixMillis>,
    pub used_bytes: SafeInteger,
    pub total_bytes: Option<SafeInteger>,
}

impl SubscriptionPublicResponse {
    pub fn remaining_bytes(&self) -> Option<SafeInteger> {
        self.total_bytes
            .map(|total| total.saturating_sub(self.used_bytes))
    }

    pub fn effective_status(&self, now_unix_ms: u64) -> SubscriptionStatus {
        if !matches!(
            self.status,
            SubscriptionStatus::Trial | SubscriptionStatus::Active
        ) {
            return self.status;
        }
        if self
            .expires_at_unix_ms
            .is_some_and(|expires_at| expires_at.get() <= now_unix_ms)
        {
            return SubscriptionStatus::Expired;
        }
        if self
            .total_bytes
            .is_some_and(|total| total.get() == 0 || self.used_bytes >= total)
        {
            return SubscriptionStatus::Exhausted;
        }
        self.status
    }

    pub fn allows_new_data_plane_start(&self, now_unix_ms: u64) -> bool {
        matches!(
            self.effective_status(now_unix_ms),
            SubscriptionStatus::Trial | SubscriptionStatus::Active
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Plan {
    pub plan_id: String,
    pub name: String,
    pub price: Money,
    pub billing_period_days: SafeInteger,
    pub traffic_bytes: Option<SafeInteger>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PlansResponse {
    #[serde(deserialize_with = "deserialize_schema_version")]
    pub schema_version: u16,
    pub plans: Vec<Plan>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateOrderRequest {
    #[serde(deserialize_with = "deserialize_schema_version")]
    pub schema_version: u16,
    pub plan_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateOrderResponse {
    #[serde(deserialize_with = "deserialize_schema_version")]
    pub schema_version: u16,
    pub order_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Order {
    pub order_id: String,
    pub plan_id: String,
    pub status: OrderStatus,
    pub amount: Money,
    pub created_at_unix_ms: UnixMillis,
    pub paid_at_unix_ms: Option<UnixMillis>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OrderResponse {
    #[serde(deserialize_with = "deserialize_schema_version")]
    pub schema_version: u16,
    pub order: Order,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OrderSummary {
    pub order_id: String,
    pub plan_id: String,
    pub plan_name: String,
    pub billing_period_days: Option<SafeInteger>,
    pub status: OrderStatus,
    pub amount: Money,
    pub created_at_unix_ms: UnixMillis,
    pub paid_at_unix_ms: Option<UnixMillis>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OrdersResponse {
    #[serde(deserialize_with = "deserialize_schema_version")]
    pub schema_version: u16,
    pub orders: Vec<OrderSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreatePaymentRequest {
    #[serde(deserialize_with = "deserialize_schema_version")]
    pub schema_version: u16,
    pub order_id: String,
    pub payment_method: String,
}

#[derive(PartialEq, Eq, Serialize, Deserialize, Zeroize, ZeroizeOnDrop)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PaymentWireResponse {
    #[zeroize(skip)]
    #[serde(deserialize_with = "deserialize_schema_version")]
    pub schema_version: u16,
    pub order_id: String,
    #[zeroize(skip)]
    pub status: PaymentStatus,
    pub payment_url: Option<String>,
    #[zeroize(skip)]
    pub expires_at_unix_ms: Option<UnixMillis>,
}

impl PaymentWireResponse {
    pub fn with_payment_url<R>(&self, consume: impl FnOnce(Option<&str>) -> R) -> R {
        consume(self.payment_url.as_deref())
    }
}

impl fmt::Debug for PaymentWireResponse {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PaymentWireResponse")
            .field("schema_version", &self.schema_version)
            .field("order_id_bytes", &self.order_id.len())
            .field("status", &self.status)
            .field("has_payment_url", &self.payment_url.is_some())
            .field("expires_at_unix_ms", &self.expires_at_unix_ms)
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PaymentPublicResponse {
    #[serde(deserialize_with = "deserialize_schema_version")]
    pub schema_version: u16,
    pub order_id: String,
    pub status: PaymentStatus,
    pub available: bool,
    pub target_host: Option<String>,
    pub expires_at_unix_ms: Option<UnixMillis>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InviteResponse {
    #[serde(deserialize_with = "deserialize_schema_version")]
    pub schema_version: u16,
    pub invite_code: String,
    pub invited_users: SafeInteger,
    pub commission: Money,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateTicketRequest {
    #[serde(deserialize_with = "deserialize_schema_version")]
    pub schema_version: u16,
    pub subject: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Ticket {
    pub ticket_id: String,
    pub status: TicketStatus,
    pub subject: String,
    pub last_message_at_unix_ms: UnixMillis,
    pub closed_at_unix_ms: Option<UnixMillis>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TicketsResponse {
    #[serde(deserialize_with = "deserialize_schema_version")]
    pub schema_version: u16,
    pub tickets: Vec<Ticket>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpdateResponse {
    #[serde(deserialize_with = "deserialize_schema_version")]
    pub schema_version: u16,
    pub latest_version: String,
    pub mandatory: bool,
    pub release_notes: Option<String>,
}
