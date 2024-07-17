// Ecosystem related messages
pub(super) const MSG_CHAIN_NOT_FOUND_ERR: &str = "Chain not found";

pub(super) fn msg_global_chain_does_not_exist(chain: &str, available_chains: &str) -> String {
    format!("Chain with name {chain} doesnt exist, please choose one of: {available_chains}")
}

// Subcommands help
pub(super) const MSG_SUBCOMMAND_DATABASE_ABOUT: &str = "Database related commands";
pub(super) const MSG_SUBCOMMAND_TESTS_ABOUT: &str = "Run tests";
pub(super) const MSG_SUBCOMMAND_CLEAN: &str = "Clean artifacts";

// Database related messages
pub(super) const MSG_NO_DATABASES_SELECTED: &str = "No databases selected";

pub(super) fn msg_database_info(gerund_verb: &str) -> String {
    format!("{gerund_verb} databases")
}

pub(super) fn msg_database_success(past_verb: &str) -> String {
    format!("Databases {past_verb} successfully")
}

pub(super) fn msg_database_loading(gerund_verb: &str, dal: &str) -> String {
    format!("{gerund_verb} database for dal {dal}...")
}

pub(super) const MSG_DATABASE_CHECK_SQLX_DATA_GERUND: &str = "Checking";
pub(super) const MSG_DATABASE_CHECK_SQLX_DATA_PAST: &str = "checked";
pub(super) const MSG_DATABASE_DROP_GERUND: &str = "Dropping";
pub(super) const MSG_DATABASE_DROP_PAST: &str = "dropped";
pub(super) const MSG_DATABASE_MIGRATE_GERUND: &str = "Migrating";
pub(super) const MSG_DATABASE_MIGRATE_PAST: &str = "migrated";
pub(super) const MSG_DATABASE_PREPARE_GERUND: &str = "Preparing";
pub(super) const MSG_DATABASE_PREPARE_PAST: &str = "prepared";
pub(super) const MSG_DATABASE_RESET_GERUND: &str = "Resetting";
pub(super) const MSG_DATABASE_RESET_PAST: &str = "reset";
pub(super) const MSG_DATABASE_SETUP_GERUND: &str = "Setting up";
pub(super) const MSG_DATABASE_SETUP_PAST: &str = "set up";

pub(super) const MSG_PROVER_URL_MUST_BE_PRESENTED: &str = "Prover url must be presented";

pub(super) const MSG_DATABASE_COMMON_PROVER_HELP: &str = "Prover database";
pub(super) const MSG_DATABASE_COMMON_CORE_HELP: &str = "Core database";
pub(super) const MSG_DATABASE_NEW_MIGRATION_DATABASE_HELP: &str =
    "Database to create new migration for";
pub(super) const MSG_DATABASE_NEW_MIGRATION_NAME_HELP: &str = "Migration name";

pub(super) const MSG_DATABASE_CHECK_SQLX_DATA_ABOUT: &str = "Check sqlx-data.json is up to date. If no databases are selected, all databases will be checked.";
pub(super) const MSG_DATABASE_DROP_ABOUT: &str =
    "Drop databases. If no databases are selected, all databases will be dropped.";
pub(super) const MSG_DATABASE_MIGRATE_ABOUT: &str =
    "Migrate databases. If no databases are selected, all databases will be migrated.";
pub(super) const MSG_DATABASE_NEW_MIGRATION_ABOUT: &str = "Create new migration";
pub(super) const MSG_DATABASE_PREPARE_ABOUT: &str =
    "Prepare sqlx-data.json. If no databases are selected, all databases will be prepared.";
pub(super) const MSG_DATABASE_RESET_ABOUT: &str =
    "Reset databases. If no databases are selected, all databases will be reset.";
pub(super) const MSG_DATABASE_SETUP_ABOUT: &str =
    "Setup databases. If no databases are selected, all databases will be setup.";

// Database new_migration messages
pub(super) const MSG_DATABASE_NEW_MIGRATION_DB_PROMPT: &str =
    "What database do you want to create a new migration for?";
pub(super) const MSG_DATABASE_NEW_MIGRATION_NAME_PROMPT: &str =
    "How do you want to name the migration?";

pub(super) fn msg_database_new_migration_loading(dal: &str) -> String {
    format!("Creating new database migration for dal {}...", dal)
}

pub(super) const MSG_DATABASE_NEW_MIGRATION_SUCCESS: &str = "Migration created successfully";

// Tests related messages
pub(super) const MSG_INTEGRATION_TESTS_ABOUT: &str = "Run integration tests";
pub(super) const MSG_REVERT_TEST_ABOUT: &str = "Run revert tests";
pub(super) const MSG_RECOVERY_TEST_ABOUT: &str = "Run recovery tests";
pub(super) const MSG_TESTS_EXTERNAL_NODE_HELP: &str = "Run tests for external node";
pub(super) const MSG_TESTS_RECOVERY_SNAPSHOT_HELP: &str =
    "Run recovery from a snapshot instead of genesis";

// Integration tests related messages
pub(super) fn msg_integration_tests_run(external_node: bool) -> String {
    let base = "Running integration tests";
    if external_node {
        format!("{} for external node", base)
    } else {
        format!("{} for main server", base)
    }
}

pub(super) const MSG_INTEGRATION_TESTS_RUN_SUCCESS: &str = "Integration tests ran successfully";
pub(super) const MSG_INTEGRATION_TESTS_BUILDING_DEPENDENCIES: &str =
    "Building repository dependencies...";
pub(super) const MSG_INTEGRATION_TESTS_BUILDING_CONTRACTS: &str = "Building test contracts...";

// Revert tests related messages
pub(super) const MSG_REVERT_TEST_ENABLE_CONSENSUS_HELP: &str = "Enable consensus";
pub(super) const MSG_REVERT_TEST_RUN_INFO: &str = "Running revert and restart test";
pub(super) const MSG_REVERT_TEST_RUN_SUCCESS: &str = "Revert and restart test ran successfully";

// Recovery tests related messages
pub(super) const MSG_RECOVERY_TEST_RUN_INFO: &str = "Running recovery test";
pub(super) const MSG_RECOVERY_TEST_RUN_SUCCESS: &str = "Recovery test ran successfully";

// Cleaning related messages
pub(super) const MSG_DOCKER_COMPOSE_DOWN: &str = "docker compose down";
pub(super) const MSG_DOCKER_COMPOSE_REMOVE_VOLUMES: &str = "docker compose remove volumes";
pub(super) const MSG_DOCKER_COMPOSE_CLEANED: &str = "docker compose network cleaned";
pub(super) const MSG_CONTRACTS_CLEANING: &str =
    "Removing contracts building and deployment artifacts";
pub(super) const MSG_CONTRACTS_CLEANING_FINISHED: &str =
    "Contracts building and deployment artifacts are cleaned up";
