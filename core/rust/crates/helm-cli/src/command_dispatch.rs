use super::*;

pub(super) fn execute_command(
    command: Command,
    store: Arc<SqliteStore>,
    options: GlobalOptions,
    command_args: &[String],
) -> Result<(), String> {
    match command {
        Command::Tui => cmd_tui(store, options),
        Command::Status => cmd_status(store.as_ref(), options),
        Command::Refresh => cmd_refresh(store, options, command_args),
        Command::Ls | Command::Search | Command::Packages => {
            execute_package_domain(command, store, options, command_args)
        }
        Command::Updates => cmd_updates(store, options, command_args),
        Command::Tasks => cmd_tasks(store.as_ref(), options, command_args),
        Command::Managers => cmd_managers(store, options, command_args),
        Command::Settings => cmd_settings(store.as_ref(), options, command_args),
        Command::Diagnostics | Command::Doctor => {
            execute_diagnostics_domain(command, store, options, command_args)
        }
        Command::Onboarding => cmd_onboarding(store.as_ref(), options, command_args),
        Command::SelfCmd => cmd_self(store, options, command_args),
        Command::InternalCoordinator => cmd_internal_coordinator(store, command_args),
        Command::Completion | Command::Help | Command::Version => Ok(()),
    }
}

fn execute_package_domain(
    command: Command,
    store: Arc<SqliteStore>,
    options: GlobalOptions,
    command_args: &[String],
) -> Result<(), String> {
    match command {
        Command::Search => cmd_search(store, options, command_args),
        Command::Ls | Command::Packages => cmd_packages(store, options, command_args),
        _ => Ok(()),
    }
}

fn execute_diagnostics_domain(
    command: Command,
    store: Arc<SqliteStore>,
    options: GlobalOptions,
    command_args: &[String],
) -> Result<(), String> {
    match command {
        Command::Diagnostics => cmd_diagnostics(store.as_ref(), options, command_args),
        Command::Doctor => cmd_doctor(store.as_ref(), options, command_args),
        _ => Ok(()),
    }
}
