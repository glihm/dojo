use serde::Deserialize;

#[derive(Debug, Clone, Deserialize, Default)]
pub struct MigrationConfig {
    pub skip_contracts: Vec<String>,
    pub do_multicall: bool,
}
