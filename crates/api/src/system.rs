use serde::Serialize;
use uuid::Uuid;

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct PublicSystemInfo {
    pub local_address: Option<String>,
    pub server_name: String,
    pub version: String,
    pub product_name: String,
    pub operating_system: String,
    pub id: Uuid,
    pub startup_wizard_completed: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct EndpointInfo {
    pub is_local: bool,
    pub is_in_network: bool,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct BrandingConfiguration {
    pub login_disclaimer: Option<String>,
    pub custom_css: Option<String>,
    pub splashscreen_enabled: bool,
}
