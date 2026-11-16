use utoipa::OpenApi;

#[derive(OpenApi)]
#[openapi(info(
    title = "Tenport",
    description = "The backend of Tenyard",
    license(name = "FSL-1.1-ALv2")
))]
pub struct ApiDoc;
