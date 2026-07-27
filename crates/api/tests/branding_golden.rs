use serde_json::json;
use tjxy_api::BrandingConfiguration;

#[test]
fn default_branding_configuration_matches_the_pinned_pascal_case_shape() {
    assert_eq!(
        serde_json::to_value(BrandingConfiguration::default()).unwrap(),
        json!({
            "LoginDisclaimer": null,
            "CustomCss": null,
            "SplashscreenEnabled": false
        })
    );
}
