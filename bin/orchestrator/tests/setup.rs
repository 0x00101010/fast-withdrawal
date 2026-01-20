//! Common test setup utilities shared across integration tests.

use orchestrator::config::Config;

/// Load test configuration. Panics if not found or invalid.
pub fn load_test_config() -> Config {
    let config_path = "tests/test-config.toml";

    // Debug: print current directory
    let current_dir = std::env::current_dir().unwrap();
    eprintln!("Current working directory: {:?}", current_dir);
    eprintln!("Looking for config at: {:?}", current_dir.join(config_path));

    Config::from_file(config_path).expect("Failed to load tests/test-config.toml.")
}
