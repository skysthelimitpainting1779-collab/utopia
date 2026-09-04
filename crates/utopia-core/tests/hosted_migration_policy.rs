use utopia_core::config::AppConfig;

#[test]
fn self_hosted_runtime_keeps_startup_migrations_by_default() {
    assert!(AppConfig::default().migrate_on_startup);
}

#[test]
fn hosted_runtime_can_disable_startup_migrations_after_deploy_time_migration() {
    let cfg = AppConfig {
        hosted: true,
        migrate_on_startup: false,
        secret_key: Some("00".repeat(32)),
        control_plane_token: Some("test-control-token".into()),
        ..AppConfig::default()
    };

    assert!(!cfg.migrate_on_startup);
    assert!(cfg.validate().is_ok());
}
