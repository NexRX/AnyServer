pub mod builtin;
pub mod update_check;

// Re-export commonly used items from builtin
pub use builtin::{get as get_builtin, is_builtin, list as list_builtin};

// Re-export all update_check functions
pub use update_check::{
    build_check_variables, execute_api_provider, execute_command_provider, extract_version,
    find_version_param_name, get_installed_version, perform_check, substitute_variables,
};
