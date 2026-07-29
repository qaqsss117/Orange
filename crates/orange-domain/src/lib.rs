#![forbid(unsafe_code)]

mod business_api;
mod error;
mod ipc;
mod state;

pub use business_api::{
    AccountResponse, AccountStatus, AuthPublicResponse, AuthSessionResponse, AuthSessionStatus,
    AuthWireResponse, BUSINESS_API_SCHEMA_VERSION, BusinessInitializationResponse, ConfigResponse,
    ConfigWireResponse, CreateOrderRequest, CreatePaymentRequest, CreateTicketRequest,
    CredentialBundle, CurrencyCode, InviteResponse, LoginRequest, Money, Order, OrderResponse,
    OrderStatus, PaymentPublicResponse, PaymentStatus, PaymentWireResponse, Plan, PlansResponse,
    RegisterRequest, SafeInteger, SubscriptionPublicResponse, SubscriptionStatus,
    SubscriptionWireResponse, Ticket, TicketStatus, TicketsResponse, UnixMillis, UpdateResponse,
    UserProfile,
};
pub use error::{CommandError, ErrorCode};
pub use ipc::{
    AccountRefreshRequest, AuthSessionRequest, BASE_COMMANDS, CONTROL_DATA_PLANE_COMMAND,
    ConnectionModeRequest, ConnectionModeResponse, DESKTOP_BUSINESS_COMMANDS,
    DESKTOP_DATA_PLANE_COMMANDS, DESKTOP_OBSERVABILITY_COMMANDS, DataPlaneControlAction,
    DataPlaneControlRequest, DataPlaneControlResponse, DataPlaneEventSnapshotRequest,
    GET_AUTH_SESSION_COMMAND, GET_CONNECTION_MODE_COMMAND, GET_DATA_PLANE_EVENT_SNAPSHOT_COMMAND,
    GET_PLANE_STATE_COMMAND, GET_RUNTIME_INFO_COMMAND, INITIALIZE_BUSINESS_COMMAND,
    InitializeBusinessRequest, LOGIN_COMMAND, LOGOUT_COMMAND, LoginCommandRequest, LogoutRequest,
    PlaneStateRequest, PlaneStateResponse, REFRESH_ACCOUNT_COMMAND, REFRESH_SUBSCRIPTION_COMMAND,
    REGISTER_COMMAND, REGISTERED_COMMANDS, RegisterCommandRequest, RuntimeInfoRequest,
    RuntimeInfoResponse, SET_CONNECTION_MODE_COMMAND, SetConnectionModeRequest,
    SubscriptionRefreshRequest, is_registered_command,
};
pub use state::{
    ConnectionMode, ControlPlaneState, ControlPlaneStateMachine, DataPlaneState,
    DataPlaneStateMachine, StateTransitionError, TransitionOutcome,
};

pub const DOMAIN_SCHEMA_VERSION: u16 = 2;

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::{
        AccountRefreshRequest, CONTROL_DATA_PLANE_COMMAND, CommandError, ControlPlaneState,
        DOMAIN_SCHEMA_VERSION, DataPlaneControlAction, DataPlaneControlRequest,
        DataPlaneControlResponse, DataPlaneEventSnapshotRequest, DataPlaneState, ErrorCode,
        GET_AUTH_SESSION_COMMAND, GET_CONNECTION_MODE_COMMAND,
        GET_DATA_PLANE_EVENT_SNAPSHOT_COMMAND, GET_PLANE_STATE_COMMAND, GET_RUNTIME_INFO_COMMAND,
        INITIALIZE_BUSINESS_COMMAND, LOGIN_COMMAND, LOGOUT_COMMAND, LoginCommandRequest,
        LogoutRequest, PlaneStateRequest, PlaneStateResponse, REFRESH_ACCOUNT_COMMAND,
        REFRESH_SUBSCRIPTION_COMMAND, REGISTER_COMMAND, REGISTERED_COMMANDS, RuntimeInfoRequest,
        RuntimeInfoResponse, SET_CONNECTION_MODE_COMMAND, SubscriptionRefreshRequest,
        is_registered_command,
    };

    const SCHEMA: &str = include_str!("../../../contracts/orange-ipc.schema.json");
    const REQUEST_FIXTURE: &str =
        include_str!("../../../contracts/fixtures/runtime-info.request.v2.json");
    const RESPONSE_FIXTURE: &str =
        include_str!("../../../contracts/fixtures/runtime-info.response.v2.json");
    const PLANE_REQUEST_FIXTURE: &str =
        include_str!("../../../contracts/fixtures/plane-state.request.v2.json");
    const PLANE_RESPONSE_FIXTURE: &str =
        include_str!("../../../contracts/fixtures/plane-state.response.v2.json");
    const DATA_PLANE_CONTROL_REQUEST_FIXTURE: &str =
        include_str!("../../../contracts/fixtures/data-plane-control.request.v2.json");
    const DATA_PLANE_CONTROL_RESPONSE_FIXTURE: &str =
        include_str!("../../../contracts/fixtures/data-plane-control.response.v2.json");
    const ERROR_FIXTURE: &str = include_str!("../../../contracts/fixtures/command-error.v2.json");

    #[test]
    fn schema_version_is_two() {
        assert_eq!(DOMAIN_SCHEMA_VERSION, 2);
    }

    #[test]
    fn request_fixture_round_trips_and_rejects_unknown_fields() {
        let request: RuntimeInfoRequest = serde_json::from_str(REQUEST_FIXTURE).unwrap();
        assert_eq!(request, RuntimeInfoRequest::current());
        assert_eq!(
            serde_json::to_value(request).unwrap(),
            json!({ "schemaVersion": 2 })
        );

        let error = serde_json::from_value::<RuntimeInfoRequest>(json!({
            "schemaVersion": 2,
            "unexpected": true
        }))
        .unwrap_err();
        assert!(error.to_string().contains("unknown field"));
    }

    #[test]
    fn response_fixture_round_trips_and_accepts_unknown_fields() {
        let response: RuntimeInfoResponse = serde_json::from_str(RESPONSE_FIXTURE).unwrap();
        assert_eq!(response.schema_version, DOMAIN_SCHEMA_VERSION);
        assert_eq!(response.product_name, "Orange");

        let mut value = serde_json::to_value(response).unwrap();
        value["futureField"] = json!("ignored");
        let compatible: RuntimeInfoResponse = serde_json::from_value(value).unwrap();
        assert_eq!(compatible.product_name, "Orange");
    }

    #[test]
    fn plane_state_fixtures_round_trip_with_strict_request_and_compatible_response() {
        let request: PlaneStateRequest = serde_json::from_str(PLANE_REQUEST_FIXTURE).unwrap();
        assert_eq!(request, PlaneStateRequest::current());
        assert!(
            serde_json::from_value::<PlaneStateRequest>(json!({
                "schemaVersion": 2,
                "path": "/tmp/private"
            }))
            .is_err()
        );

        let response: PlaneStateResponse = serde_json::from_str(PLANE_RESPONSE_FIXTURE).unwrap();
        assert_eq!(response.control_plane, ControlPlaneState::Cold);
        assert_eq!(response.data_plane, DataPlaneState::Unconfigured);
        let mut value = serde_json::to_value(response).unwrap();
        value["futureField"] = json!(true);
        assert!(serde_json::from_value::<PlaneStateResponse>(value).is_ok());
    }

    #[test]
    fn invalid_schema_version_returns_sanitized_validation_error() {
        let error = RuntimeInfoRequest { schema_version: 1 }
            .validate()
            .unwrap_err();
        assert_eq!(error, CommandError::from_code(ErrorCode::Validation));
        assert!(!serde_json::to_string(&error).unwrap().contains('1'));
    }

    #[test]
    fn error_fixture_round_trips_and_unknown_codes_are_rejected() {
        let error: CommandError = serde_json::from_str(ERROR_FIXTURE).unwrap();
        assert_eq!(error, CommandError::from_code(ErrorCode::Network));

        let unknown = json!({
            "schemaVersion": 2,
            "code": "future_error",
            "message": "A safe public message.",
            "retryable": false
        });
        assert!(serde_json::from_value::<CommandError>(unknown).is_err());

        let unsafe_message = json!({
            "schemaVersion": 2,
            "code": "network",
            "message": "secret diagnostic detail",
            "retryable": true
        });
        assert!(serde_json::from_value::<CommandError>(unsafe_message).is_err());
    }

    #[test]
    fn error_code_table_covers_every_public_category_without_secrets() {
        for code in ErrorCode::ALL {
            let error = CommandError::from_code(code);
            let serialized = serde_json::to_string(&error).unwrap();
            assert!(!serialized.contains("secret"));
            assert!(!serialized.contains("token"));
            assert!(!serialized.contains("detail"));
        }
    }

    #[test]
    fn command_registry_denies_unknown_commands() {
        assert_eq!(
            REGISTERED_COMMANDS,
            &[
                GET_PLANE_STATE_COMMAND,
                GET_RUNTIME_INFO_COMMAND,
                GET_DATA_PLANE_EVENT_SNAPSHOT_COMMAND,
                CONTROL_DATA_PLANE_COMMAND,
                GET_CONNECTION_MODE_COMMAND,
                SET_CONNECTION_MODE_COMMAND,
                INITIALIZE_BUSINESS_COMMAND,
                LOGIN_COMMAND,
                REGISTER_COMMAND,
                GET_AUTH_SESSION_COMMAND,
                LOGOUT_COMMAND,
                REFRESH_ACCOUNT_COMMAND,
                REFRESH_SUBSCRIPTION_COMMAND,
            ]
        );
        assert!(is_registered_command(GET_PLANE_STATE_COMMAND));
        assert!(is_registered_command(GET_RUNTIME_INFO_COMMAND));
        assert!(is_registered_command(GET_DATA_PLANE_EVENT_SNAPSHOT_COMMAND));
        assert!(is_registered_command(CONTROL_DATA_PLANE_COMMAND));
        assert!(is_registered_command(GET_CONNECTION_MODE_COMMAND));
        assert!(is_registered_command(SET_CONNECTION_MODE_COMMAND));
        assert!(is_registered_command(INITIALIZE_BUSINESS_COMMAND));
        assert!(is_registered_command(LOGIN_COMMAND));
        assert!(is_registered_command(REGISTER_COMMAND));
        assert!(is_registered_command(GET_AUTH_SESSION_COMMAND));
        assert!(is_registered_command(LOGOUT_COMMAND));
        assert!(is_registered_command(REFRESH_ACCOUNT_COMMAND));
        assert!(is_registered_command(REFRESH_SUBSCRIPTION_COMMAND));
        assert!(!is_registered_command("open_file"));
        assert!(!is_registered_command("run_shell"));
    }

    #[test]
    fn login_ipc_request_returns_canonical_validation_errors_without_secret_debug() {
        let request = LoginCommandRequest {
            schema_version: 1,
            email: "member@example.invalid".to_owned(),
            password: "do-not-print-this-password".to_owned(),
        };
        let debug = format!("{request:?}");
        assert!(!debug.contains("member@example.invalid"));
        assert!(!debug.contains("do-not-print-this-password"));
        assert_eq!(
            request.validate().unwrap_err(),
            CommandError::from_code(ErrorCode::Validation)
        );
    }

    #[test]
    fn account_and_subscription_refresh_requests_reject_all_injected_fields() {
        assert_eq!(
            AccountRefreshRequest::current().validate().unwrap(),
            AccountRefreshRequest::current()
        );
        assert_eq!(
            SubscriptionRefreshRequest::current().validate().unwrap(),
            SubscriptionRefreshRequest::current()
        );
        for injected in [
            json!({ "schemaVersion": 2, "url": "https://evil.invalid" }),
            json!({ "schemaVersion": 2, "token": "not-allowed" }),
            json!({ "schemaVersion": 2, "subscriptionCredential": "not-allowed" }),
            json!({ "schemaVersion": 2, "extra": true }),
        ] {
            assert!(serde_json::from_value::<AccountRefreshRequest>(injected.clone()).is_err());
            assert!(
                serde_json::from_value::<SubscriptionRefreshRequest>(injected.clone()).is_err()
            );
            assert!(serde_json::from_value::<LogoutRequest>(injected).is_err());
        }
        assert_eq!(
            LogoutRequest::current().validate().unwrap(),
            LogoutRequest::current()
        );
    }

    #[test]
    fn event_snapshot_request_is_closed_and_versioned() {
        assert_eq!(
            DataPlaneEventSnapshotRequest::current().validate().unwrap(),
            DataPlaneEventSnapshotRequest::current()
        );
        assert!(
            serde_json::from_value::<DataPlaneEventSnapshotRequest>(json!({
                "schemaVersion": 2,
                "path": "C:/private"
            }))
            .is_err()
        );
        assert_eq!(
            DataPlaneEventSnapshotRequest { schema_version: 1 }
                .validate()
                .unwrap_err(),
            CommandError::from_code(ErrorCode::Validation)
        );
    }

    #[test]
    fn data_plane_control_fixtures_are_closed_and_forward_compatible() {
        let request: DataPlaneControlRequest =
            serde_json::from_str(DATA_PLANE_CONTROL_REQUEST_FIXTURE).unwrap();
        assert_eq!(
            request,
            DataPlaneControlRequest::current(DataPlaneControlAction::Status)
        );
        for injected in [
            json!({ "schemaVersion": 2, "action": "start", "revision": 7 }),
            json!({ "schemaVersion": 2, "action": "start", "config": {} }),
            json!({ "schemaVersion": 2, "action": "start", "url": "https://evil.invalid" }),
        ] {
            assert!(serde_json::from_value::<DataPlaneControlRequest>(injected).is_err());
        }
        assert!(
            serde_json::from_value::<DataPlaneControlRequest>(json!({
                "schemaVersion": 2,
                "action": "restart"
            }))
            .is_err()
        );

        let response: DataPlaneControlResponse =
            serde_json::from_str(DATA_PLANE_CONTROL_RESPONSE_FIXTURE).unwrap();
        assert_eq!(response.control_plane, ControlPlaneState::Ready);
        assert_eq!(response.data_plane, DataPlaneState::Unconfigured);
        assert!(!response.can_start);
        assert!(!response.can_stop);
        let mut value = serde_json::to_value(response).unwrap();
        value["futureField"] = json!(true);
        assert!(serde_json::from_value::<DataPlaneControlResponse>(value).is_ok());
    }

    #[test]
    fn rust_contract_matches_the_canonical_schema() {
        let schema: Value = serde_json::from_str(SCHEMA).unwrap();
        let schema_commands = schema["x-orange-commands"]
            .as_array()
            .unwrap()
            .iter()
            .map(|command| command["name"].as_str().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(schema_commands, REGISTERED_COMMANDS);

        let schema_codes = schema["$defs"]["ErrorCode"]["enum"]
            .as_array()
            .unwrap()
            .iter()
            .map(|code| code.as_str().unwrap())
            .collect::<Vec<_>>();
        let rust_codes = ErrorCode::ALL
            .iter()
            .map(|code| code.as_str())
            .collect::<Vec<_>>();
        assert_eq!(schema_codes, rust_codes);

        let schema_definitions = schema["x-orange-error-definitions"].as_array().unwrap();
        for (definition, code) in schema_definitions.iter().zip(ErrorCode::ALL) {
            assert_eq!(definition["code"], code.as_str());
            assert_eq!(definition["message"], code.public_message());
            assert_eq!(definition["retryable"], code.retryable());
        }
    }
}
