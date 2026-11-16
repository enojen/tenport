use utoipa::openapi::security::{HttpAuthScheme, HttpBuilder, SecurityScheme};
use utoipa::{Modify, OpenApi};

const BEARER_SCHEME: &str = "bearer";

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
