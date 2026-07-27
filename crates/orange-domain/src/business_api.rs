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

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use serde::Deserialize;
    use serde_json::{Value, json};
    use zeroize::Zeroize;

    use super::*;

    const SCHEMA: &str =
        include_str!("../../../contracts/business-api/business-api.schema.v1.json");
    const WIRE_SUCCESS: &str =
        include_str!("../../../contracts/business-api/fixtures/wire-success.v1.json");
    const PUBLIC_SUCCESS: &str =
        include_str!("../../../contracts/business-api/fixtures/public-success.v1.json");
    const FAILURES: &str =
        include_str!("../../../contracts/business-api/fixtures/failures.v1.json");
    const FIELD_MAPPING: &str =
        include_str!("../../../contracts/business-api/field-mapping.v1.json");

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    struct WireFixture {
        schema_version: u16,
        environment: String,
        responses: WireResponses,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    struct WireResponses {
        config: ConfigWireResponse,
        login: AuthWireResponse,
        register: AuthWireResponse,
        account: AccountResponse,
        subscription: SubscriptionWireResponse,
        plans: PlansResponse,
        orders: OrderResponse,
        payment: PaymentWireResponse,
        invite: InviteResponse,
        tickets: TicketsResponse,
        update: UpdateResponse,
    }

    #[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    struct PublicFixture {
        schema_version: u16,
        environment: String,
        responses: PublicResponses,
    }

    #[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    struct PublicResponses {
        config: ConfigResponse,
        login: AuthPublicResponse,
        register: AuthPublicResponse,
        account: AccountResponse,
        subscription: SubscriptionPublicResponse,
        plans: PlansResponse,
        orders: OrderResponse,
        payment: PaymentPublicResponse,
        invite: InviteResponse,
        tickets: TicketsResponse,
        update: UpdateResponse,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    struct FailureDocument {
        schema_version: u16,
        cases: Vec<FailureCase>,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    struct FailureCase {
        name: String,
        source: FailureSource,
        expected: FailureResult,
    }

    #[derive(Deserialize)]
    #[serde(
        tag = "kind",
        rename_all = "snake_case",
        rename_all_fields = "camelCase",
        deny_unknown_fields
    )]
    enum FailureSource {
        Http {
            status_code: u16,
            content_type: Option<String>,
            body_class: BodyClass,
        },
        Transport {
            code: TransportFailure,
        },
    }

    #[derive(Debug, Deserialize, PartialEq, Eq)]
    #[serde(rename_all = "snake_case")]
    enum BodyClass {
        Empty,
        ApiError,
        NonJson,
        SchemaDrift,
    }

    #[derive(Debug, Deserialize, PartialEq, Eq)]
    #[serde(rename_all = "snake_case")]
    enum TransportFailure {
        Timeout,
    }

    #[derive(Debug, Deserialize, PartialEq, Eq)]
    #[serde(rename_all = "snake_case")]
    enum FailureResult {
        EmptySuccess,
        RequestRejected,
        ServiceUnavailable,
        InvalidResponse,
        Timeout,
        SchemaDrift,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    struct MappingDocument {
        schema_version: u16,
        mappings: Vec<FieldMapping>,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    struct FieldMapping {
        operation: String,
        wire_field: String,
        public_field: Option<String>,
        policy: String,
    }

    #[test]
    fn wire_fixture_covers_every_operation_and_redacts_debug_output() {
        let mut fixture: WireFixture = serde_json::from_str(WIRE_SUCCESS).unwrap();
        assert_eq!(fixture.schema_version, BUSINESS_API_SCHEMA_VERSION);
        assert_eq!(fixture.environment, "development");
        assert_eq!(fixture.responses.config.schema_version, 1);
        assert_eq!(fixture.responses.login.schema_version, 1);
        assert_eq!(fixture.responses.register.schema_version, 1);
        assert_eq!(fixture.responses.account.schema_version, 1);
        assert_eq!(fixture.responses.subscription.schema_version, 1);
        assert_eq!(fixture.responses.plans.schema_version, 1);
        assert_eq!(fixture.responses.orders.schema_version, 1);
        assert_eq!(fixture.responses.payment.schema_version, 1);
        assert_eq!(fixture.responses.invite.schema_version, 1);
        assert_eq!(fixture.responses.tickets.schema_version, 1);
        assert_eq!(fixture.responses.update.schema_version, 1);

        let debug = format!(
            "{:?} {:?} {:?} {:?}",
            fixture.responses.config,
            fixture.responses.login,
            fixture.responses.subscription,
            fixture.responses.payment
        );
        for secret in [
            "<redacted:access-token>",
            "<redacted:refresh-token>",
            "<redacted:subscription-credential>",
            "<redacted:payment-url>",
            "<redacted:api-base-url>",
            "<redacted:payment-base-url>",
            "<redacted:support-url>",
            "<redacted:banner-url>",
            "member@example.invalid",
        ] {
            assert!(!debug.contains(secret));
        }

        fixture.responses.login.credentials.zeroize();
        fixture.responses.subscription.zeroize();
        fixture.responses.payment.zeroize();
        assert!(fixture.responses.login.credentials.access_token.is_empty());
        assert!(fixture.responses.login.credentials.refresh_token.is_empty());
        assert!(
            fixture
                .responses
                .subscription
                .subscription_credential
                .is_empty()
        );
        assert!(fixture.responses.payment.payment_url.is_none());
    }

    #[test]
    fn public_fixture_round_trips_strictly_without_secret_fields() {
        let fixture: PublicFixture = serde_json::from_str(PUBLIC_SUCCESS).unwrap();
        assert_eq!(fixture.schema_version, BUSINESS_API_SCHEMA_VERSION);
        assert_eq!(fixture.environment, "development");
        assert!(fixture.responses.login.authenticated);
        assert_eq!(
            fixture.responses.subscription.status,
            SubscriptionStatus::Active
        );
        assert_eq!(
            fixture.responses.payment.target_host.as_deref(),
            Some("pay.orange.invalid")
        );
        assert_eq!(
            serde_json::to_value(&fixture).unwrap(),
            serde_json::from_str::<Value>(PUBLIC_SUCCESS).unwrap()
        );

        let mut drift: Value = serde_json::from_str(PUBLIC_SUCCESS).unwrap();
        drift["responses"]["account"]["unexpected"] = json!(true);
        assert!(serde_json::from_value::<PublicFixture>(drift).is_err());

        let mut version_drift: Value = serde_json::from_str(PUBLIC_SUCCESS).unwrap();
        version_drift["responses"]["account"]["schemaVersion"] = json!(2);
        assert!(serde_json::from_value::<PublicFixture>(version_drift).is_err());
    }

    #[test]
    fn units_nullability_and_unknown_status_policy_are_enforced() {
        assert!(SafeInteger::new(MAX_JAVASCRIPT_SAFE_INTEGER).is_some());
        assert!(SafeInteger::new(MAX_JAVASCRIPT_SAFE_INTEGER + 1).is_none());
        assert!(serde_json::from_str::<SafeInteger>("9007199254740992").is_err());
        assert!(serde_json::from_str::<CurrencyCode>("\"CNY\"").is_ok());
        assert!(serde_json::from_str::<CurrencyCode>("\"cny\"").is_err());
        assert_eq!(
            serde_json::from_str::<OrderStatus>("\"future_server_value\"").unwrap(),
            OrderStatus::Unknown
        );

        let fixture: PublicFixture = serde_json::from_str(PUBLIC_SUCCESS).unwrap();
        assert!(fixture.responses.config.notice.is_none());
        assert!(fixture.responses.orders.order.paid_at_unix_ms.is_none());
        assert!(
            fixture.responses.tickets.tickets[0]
                .closed_at_unix_ms
                .is_none()
        );
    }

    #[test]
    fn schema_declares_every_operation_and_closed_object_policy() {
        let schema: Value = serde_json::from_str(SCHEMA).unwrap();
        assert_eq!(schema["schemaVersion"], BUSINESS_API_SCHEMA_VERSION);
        assert_eq!(schema["environment"], "development");
        assert_eq!(schema["releaseAllowed"], false);
        assert_eq!(schema["units"]["timestamp"], "unix_milliseconds");
        assert_eq!(schema["units"]["money"], "integer_minor_units");
        assert_eq!(schema["unknownFieldPolicy"]["objects"], "reject");
        assert_eq!(
            schema["unknownFieldPolicy"]["statusValues"],
            "map_to_unknown"
        );

        let operations = schema["x-orange-operations"]
            .as_array()
            .unwrap()
            .iter()
            .map(|entry| entry["name"].as_str().unwrap())
            .collect::<HashSet<_>>();
        assert_eq!(
            operations,
            HashSet::from([
                "config",
                "login",
                "register",
                "account",
                "subscription",
                "plans",
                "orders",
                "payment",
                "invite",
                "tickets",
                "update",
            ])
        );

        for definition in schema["$defs"].as_object().unwrap().values() {
            if definition["type"] == "object" {
                assert_eq!(definition["additionalProperties"], false);
            }
        }
    }

    #[test]
    fn failure_fixture_covers_every_required_failure_class() {
        let document: FailureDocument = serde_json::from_str(FAILURES).unwrap();
        assert_eq!(document.schema_version, BUSINESS_API_SCHEMA_VERSION);
        assert_eq!(document.cases.len(), 6);
        assert_eq!(
            document
                .cases
                .iter()
                .map(|case| case.name.as_str())
                .collect::<HashSet<_>>(),
            HashSet::from([
                "empty-2xx",
                "http-4xx",
                "http-5xx",
                "non-json",
                "timeout",
                "schema-drift",
            ])
        );

        for case in document.cases {
            match (case.name.as_str(), case.source, case.expected) {
                (
                    "empty-2xx",
                    FailureSource::Http {
                        status_code: 204,
                        content_type: None,
                        body_class: BodyClass::Empty,
                    },
                    FailureResult::EmptySuccess,
                )
                | (
                    "http-4xx",
                    FailureSource::Http {
                        status_code: 400,
                        content_type: Some(_),
                        body_class: BodyClass::ApiError,
                    },
                    FailureResult::RequestRejected,
                )
                | (
                    "http-5xx",
                    FailureSource::Http {
                        status_code: 503,
                        content_type: Some(_),
                        body_class: BodyClass::ApiError,
                    },
                    FailureResult::ServiceUnavailable,
                )
                | (
                    "non-json",
                    FailureSource::Http {
                        status_code: 200,
                        content_type: Some(_),
                        body_class: BodyClass::NonJson,
                    },
                    FailureResult::InvalidResponse,
                )
                | (
                    "timeout",
                    FailureSource::Transport {
                        code: TransportFailure::Timeout,
                    },
                    FailureResult::Timeout,
                )
                | (
                    "schema-drift",
                    FailureSource::Http {
                        status_code: 200,
                        content_type: Some(_),
                        body_class: BodyClass::SchemaDrift,
                    },
                    FailureResult::SchemaDrift,
                ) => {}
                _ => panic!("failure fixture mapping drifted"),
            }
        }
    }

    #[test]
    fn field_mapping_keeps_sensitive_values_out_of_public_dtos() {
        let document: MappingDocument = serde_json::from_str(FIELD_MAPPING).unwrap();
        assert_eq!(document.schema_version, BUSINESS_API_SCHEMA_VERSION);
        let mappings = document
            .mappings
            .into_iter()
            .map(|mapping| {
                (
                    (mapping.operation, mapping.wire_field),
                    (mapping.public_field, mapping.policy),
                )
            })
            .collect::<std::collections::HashMap<_, _>>();

        for (key, public_field, policy) in [
            (
                ("login", "credentials.accessToken"),
                None,
                "rust_secure_store",
            ),
            (
                ("login", "credentials.refreshToken"),
                None,
                "rust_secure_store",
            ),
            (
                ("subscription", "subscriptionCredential"),
                None,
                "data_plane_only",
            ),
            (
                ("payment", "paymentUrl"),
                Some("targetHost"),
                "validate_and_project_host",
            ),
        ] {
            let mapped = mappings.get(&(key.0.to_owned(), key.1.to_owned())).unwrap();
            assert_eq!(mapped.0.as_deref(), public_field);
            assert_eq!(mapped.1, policy);
        }
    }
}
