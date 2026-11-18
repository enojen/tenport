use serde_json::Value;
use utoipa::openapi::path::{Operation, PathItem};
use utoipa::openapi::security::{HttpAuthScheme, HttpBuilder, SecurityScheme};
use utoipa::openapi::{Ref, RefOr};
use utoipa::{Modify, OpenApi, ToResponse};

use super::error::{
    NotMember, NotProjectAdmin, RunNotFound, TooManyRequests, Unauthorized, Unavailable,
};
use crate::runs::command_rules;

const BEARER_SCHEME: &str = "bearer";
const INJECTOR_SCHEME: &str = "injector";
const RUN_COMMAND_POINTER: &str = "/components/schemas/NewRun/properties/command";
const COMMANDS_EXTENSION: &str = "x-tenbox-commands";

#[derive(OpenApi)]
#[openapi(
    info(
        title = "Tenport",
        description = "The backend of Tenyard. Limits: 10 requests per second for each user, 20 at once (429, code too-many-requests); 5 followed logs open for each user (429, code too-many-log-streams); 30 seconds for a request to answer (503, code timeout); 2 MB for a body (413). A 429 has Retry-After: ask again after that many seconds. A list gives 50 items, up to 100 with limit, and a Link header with rel=\"next\" for the next page.",
        license(name = "FSL-1.1-ALv2")
    ),
    components(responses(
        Unauthorized,
        NotMember,
        NotProjectAdmin,
        RunNotFound,
        TooManyRequests,
        Unavailable
    )),
    modifiers(&SecuritySchemes)
)]
pub struct ApiDoc;

struct SecuritySchemes;

impl Modify for SecuritySchemes {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        let components = openapi.components.get_or_insert_with(Default::default);
        components.add_security_scheme(
            BEARER_SCHEME,
            SecurityScheme::Http(HttpBuilder::new().scheme(HttpAuthScheme::Bearer).build()),
        );
        components.add_security_scheme(
            INJECTOR_SCHEME,
            SecurityScheme::Http(
                HttpBuilder::new()
                    .scheme(HttpAuthScheme::Bearer)
                    .description(Some(
                        "The injector token: only the injector of the egress proxy has it",
                    ))
                    .build(),
            ),
        );
    }
}

pub fn document(mut api: utoipa::openapi::OpenApi) -> Value {
    for item in api.paths.paths.values_mut() {
        operations(item)
            .filter(|operation| has_security(operation))
            .for_each(add_shared_responses);
    }
    let mut document = serde_json::to_value(api).unwrap_or(Value::Null);
    if let Some(Value::Object(command)) = document.pointer_mut(RUN_COMMAND_POINTER) {
        command.insert(COMMANDS_EXTENSION.to_owned(), command_rules());
    }
    document
}

fn operations(item: &mut PathItem) -> impl Iterator<Item = &mut Operation> {
    [
        &mut item.get,
        &mut item.put,
        &mut item.post,
        &mut item.delete,
        &mut item.patch,
    ]
    .into_iter()
    .flatten()
}

fn has_security(operation: &Operation) -> bool {
    operation
        .security
        .as_ref()
        .is_some_and(|security| !security.is_empty())
}

fn add_shared_responses(operation: &mut Operation) {
    let shared = [
        ("401", Unauthorized::response().0),
        ("429", TooManyRequests::response().0),
        ("503", Unavailable::response().0),
    ];
    for (status, name) in shared {
        operation
            .responses
            .responses
            .entry(status.to_owned())
            .or_insert_with(|| RefOr::Ref(Ref::from_response_name(name)));
    }
}
