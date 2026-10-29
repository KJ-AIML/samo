use crate::command::CommandHandler;
use crate::commands::disk::{
    DiskCleanupBeginCmd, DiskCleanupFinishCmd, DiskScanBeginCmd, DiskScanFinishCmd, DiskScanGetCmd,
    DiskScanListCmd,
};
use crate::commands::process::{ProcessTerminateBeginCmd, ProcessTerminateFinishCmd};
use crate::commands::settings::{SettingsGetCmd, SettingsThemeSetCmd};
use samo_blueprint::{CommandSpec, find_command_spec};
use std::collections::BTreeMap;
use std::sync::Arc;

pub struct RegisteredCommand {
    pub spec: &'static CommandSpec,
    pub handler: Arc<dyn CommandHandler>,
}

#[derive(Clone, Default)]
pub struct CommandRegistry {
    commands: BTreeMap<&'static str, Arc<RegisteredCommand>>,
}

impl CommandRegistry {
    pub fn new() -> Self {
        let mut reg = Self {
            commands: BTreeMap::new(),
        };
        reg.register_spec("disk.scan.begin", Arc::new(DiskScanBeginCmd));
        reg.register_spec("disk.scan.finish", Arc::new(DiskScanFinishCmd));
        reg.register_spec("disk.scan.list", Arc::new(DiskScanListCmd));
        reg.register_spec("disk.scan.get", Arc::new(DiskScanGetCmd));
        reg.register_spec("disk.cleanup.begin", Arc::new(DiskCleanupBeginCmd));
        reg.register_spec("disk.cleanup.finish", Arc::new(DiskCleanupFinishCmd));
        reg.register_spec(
            "process.terminate.begin",
            Arc::new(ProcessTerminateBeginCmd),
        );
        reg.register_spec(
            "process.terminate.finish",
            Arc::new(ProcessTerminateFinishCmd),
        );
        reg.register_spec("settings.get", Arc::new(SettingsGetCmd));
        reg.register_spec("settings.theme.set", Arc::new(SettingsThemeSetCmd));
        reg
    }

    pub fn register_spec(&mut self, name: &'static str, handler: Arc<dyn CommandHandler>) {
        if let Some(spec) = find_command_spec(name) {
            self.commands
                .insert(name, Arc::new(RegisteredCommand { spec, handler }));
        }
    }

    pub fn get(&self, name: &str) -> Option<Arc<RegisteredCommand>> {
        self.commands.get(name).cloned()
    }
}
