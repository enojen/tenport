use serde_json::Value;
use utoipa::openapi::security::{HttpAuthScheme, HttpBuilder, SecurityScheme};
use utoipa::{Modify, OpenApi};

use crate::runs::command_rules;

const BEARER_SCHEME: &str = "bearer";
const RUN_COMMAND_POINTER: &str = "/components/schemas/NewRun/properties/command";
const COMMANDS_EXTENSION: &str = "x-tenbox-commands";

#[derive(OpenApi)]
#[openapi(
    info(
        title = "Tenport",
        description = "The backend of Tenyard",
        license(name = "FSL-1.1-ALv2")
    ),
    modifiers(&BearerToken)
)]
pub struct ApiDoc;

struct BearerToken;

impl Modify for BearerToken {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        let components = openapi.components.get_or_insert_with(Default::default);
        components.add_security_scheme(
            BEARER_SCHEME,
            SecurityScheme::Http(HttpBuilder::new().scheme(HttpAuthScheme::Bearer).build()),
        );
    }
}

pub fn document(api: &utoipa::openapi::OpenApi) -> Value {
    let mut document = serde_json::to_value(api).unwrap_or(Value::Null);
    if let Some(Value::Object(command)) = document.pointer_mut(RUN_COMMAND_POINTER) {
        command.insert(COMMANDS_EXTENSION.to_owned(), command_rules());
    }
    document
}
