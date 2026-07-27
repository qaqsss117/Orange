#![forbid(unsafe_code)]

mod business_api;
mod error;
mod ipc;
mod state;

pub use business_api::{
    AccountResponse, AccountStatus, AuthPublicResponse, AuthWireResponse,
    BUSINESS_API_SCHEMA_VERSION, ConfigResponse, CreateOrderRequest, CreatePaymentRequest,
    CreateTicketRequest, CredentialBundle, CurrencyCode, InviteResponse, LoginRequest, Money,
    Order, OrderResponse, OrderStatus, PaymentPublicResponse, PaymentStatus, PaymentWireResponse,
    Plan, PlansResponse, RegisterRequest, SafeInteger, SubscriptionPublicResponse,
    SubscriptionStatus, SubscriptionWireResponse, Ticket, TicketStatus, TicketsResponse,
    UnixMillis, UpdateResponse, UserProfile,
};
pub use error::{CommandError, ErrorCode};
pub use ipc::{
    GET_PLANE_STATE_COMMAND, GET_RUNTIME_INFO_COMMAND, PlaneStateRequest, PlaneStateResponse,
    REGISTERED_COMMANDS, RuntimeInfoRequest, RuntimeInfoResponse, is_registered_command,
};
pub use state::{
    ControlPlaneState, ControlPlaneStateMachine, DataPlaneState, DataPlaneStateMachine,
    StateTransitionError, TransitionOutcome,
};

pub const DOMAIN_SCHEMA_VERSION: u16 = 1;

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::{
        CommandError, ControlPlaneState, DOMAIN_SCHEMA_VERSION, DataPlaneState, ErrorCode,
        GET_PLANE_STATE_COMMAND, GET_RUNTIME_INFO_COMMAND, PlaneStateRequest, PlaneStateResponse,
        REGISTERED_COMMANDS, RuntimeInfoRequest, RuntimeInfoResponse, is_registered_command,
    };

    const SCHEMA: &str = include_str!("../../../contracts/orange-ipc.schema.json");
    const REQUEST_FIXTURE: &str =
        include_str!("../../../contracts/fixtures/runtime-info.request.v1.json");
    const RESPONSE_FIXTURE: &str =
        include_str!("../../../contracts/fixtures/runtime-info.response.v1.json");
    const PLANE_REQUEST_FIXTURE: &str =
        include_str!("../../../contracts/fixtures/plane-state.request.v1.json");
    const PLANE_RESPONSE_FIXTURE: &str =
        include_str!("../../../contracts/fixtures/plane-state.response.v1.json");
    const ERROR_FIXTURE: &str = include_str!("../../../contracts/fixtures/command-error.v1.json");

    #[test]
    fn schema_version_starts_at_one() {
        assert_eq!(DOMAIN_SCHEMA_VERSION, 1);
    }

    #[test]
    fn request_fixture_round_trips_and_rejects_unknown_fields() {
        let request: RuntimeInfoRequest = serde_json::from_str(REQUEST_FIXTURE).unwrap();
        assert_eq!(request, RuntimeInfoRequest::current());
        assert_eq!(
            serde_json::to_value(request).unwrap(),
            json!({ "schemaVersion": 1 })
        );

        let error = serde_json::from_value::<RuntimeInfoRequest>(json!({
            "schemaVersion": 1,
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
                "schemaVersion": 1,
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
        let error = RuntimeInfoRequest { schema_version: 2 }
            .validate()
            .unwrap_err();
        assert_eq!(error, CommandError::from_code(ErrorCode::Validation));
        assert!(!serde_json::to_string(&error).unwrap().contains('2'));
    }

    #[test]
    fn error_fixture_round_trips_and_unknown_codes_are_rejected() {
        let error: CommandError = serde_json::from_str(ERROR_FIXTURE).unwrap();
        assert_eq!(error, CommandError::from_code(ErrorCode::Network));

        let unknown = json!({
            "schemaVersion": 1,
            "code": "future_error",
            "message": "A safe public message.",
            "retryable": false
        });
        assert!(serde_json::from_value::<CommandError>(unknown).is_err());

        let unsafe_message = json!({
            "schemaVersion": 1,
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
            &[GET_PLANE_STATE_COMMAND, GET_RUNTIME_INFO_COMMAND]
        );
        assert!(is_registered_command(GET_PLANE_STATE_COMMAND));
        assert!(is_registered_command(GET_RUNTIME_INFO_COMMAND));
        assert!(!is_registered_command("open_file"));
        assert!(!is_registered_command("run_shell"));
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
