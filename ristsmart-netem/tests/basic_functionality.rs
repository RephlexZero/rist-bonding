//! Basic functionality tests (unit-level, no privileged operations required)

use ristsmart_netem::{
    builder::EmulatorBuilder,
    types::{DelayProfile, GEParams, LinkSpec, OUParams, RateLimiter},
};

#[test]
fn test_link_spec_creation() {
    let spec = LinkSpec::new("test-link");
    assert_eq!(spec.name, "test-link");
    assert!(matches!(spec.rate_limiter, RateLimiter::Tbf));
    assert_eq!(spec.ou.mean_bps, 1_000_000); // Default 1 Mbps
}

#[test]
fn test_builder_validation() {
    // Empty builder should fail validation
    let builder = EmulatorBuilder::new();
    assert!(builder.validate().is_err());

    // Duplicate names should fail
    let mut builder = EmulatorBuilder::new();
    builder.add_link(LinkSpec::new("test"));
    builder.add_link(LinkSpec::new("test"));
    assert!(builder.validate().is_err());

    // Valid builder should pass
    let mut builder = EmulatorBuilder::new();
    builder.add_link(LinkSpec::new("link1"));
    builder.add_link(LinkSpec::new("link2"));
    assert!(builder.validate().is_ok());
}

#[test]
fn test_ou_params_defaults() {
    let ou = OUParams::default();
    assert_eq!(ou.mean_bps, 1_000_000);
    assert_eq!(ou.tau_ms, 1000);
    assert_eq!(ou.sigma, 0.2);
    assert_eq!(ou.tick_ms, 100);
}

#[test]
fn test_ge_params_defaults() {
    let ge = GEParams::default();
    assert_eq!(ge.p_good, 0.001);
    assert_eq!(ge.p_bad, 0.1);
    assert_eq!(ge.p, 0.01);
    assert_eq!(ge.r, 0.1);
}

#[test]
fn test_delay_profile_defaults() {
    let delay = DelayProfile::default();
    assert_eq!(delay.delay_ms, 20);
    assert_eq!(delay.jitter_ms, 5);
    assert_eq!(delay.reorder_pct, 0.0);
}

#[test]
fn test_json_serialization() {
    use serde_json;

    let spec = LinkSpec::new("test");
    let json = serde_json::to_string(&spec).expect("Should serialize to JSON");

    let deserialized: LinkSpec = serde_json::from_str(&json).expect("Should deserialize from JSON");
    assert_eq!(deserialized.name, spec.name);
    assert_eq!(deserialized.ou.mean_bps, spec.ou.mean_bps);
}
