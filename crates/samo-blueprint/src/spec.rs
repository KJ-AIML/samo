use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Surface {
    Web,
    Cli,
    Internal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Permission {
    DiskRead,
    DiskScan,
    DiskCleanup,
    ProcessRead,
    ProcessTerminate,
    SettingsRead,
    SettingsWrite,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommandSpec {
    pub name: &'static str,
    pub permission: Permission,
    pub surfaces: &'static [Surface],
    pub mutates_state: bool,
}

pub const ALL_COMMANDS: &[CommandSpec] = &[
    CommandSpec {
        name: "disk.scan.begin",
        permission: Permission::DiskScan,
        surfaces: &[Surface::Web, Surface::Cli],
        mutates_state: true,
    },
    CommandSpec {
        name: "disk.scan.finish",
        permission: Permission::DiskScan,
        surfaces: &[Surface::Web, Surface::Cli, Surface::Internal],
        mutates_state: true,
    },
    CommandSpec {
        name: "disk.scan.list",
        permission: Permission::DiskRead,
        surfaces: &[Surface::Web, Surface::Cli],
        mutates_state: false,
    },
    CommandSpec {
        name: "disk.scan.get",
        permission: Permission::DiskRead,
        surfaces: &[Surface::Web, Surface::Cli],
        mutates_state: false,
    },
    CommandSpec {
        name: "disk.cleanup.begin",
        permission: Permission::DiskCleanup,
        surfaces: &[Surface::Web, Surface::Cli],
        mutates_state: true,
    },
    CommandSpec {
        name: "disk.cleanup.finish",
        permission: Permission::DiskCleanup,
        surfaces: &[Surface::Web, Surface::Cli, Surface::Internal],
        mutates_state: true,
    },
    CommandSpec {
        name: "process.terminate.begin",
        permission: Permission::ProcessTerminate,
        surfaces: &[Surface::Web, Surface::Cli],
        mutates_state: true,
    },
    CommandSpec {
        name: "process.terminate.finish",
        permission: Permission::ProcessTerminate,
        surfaces: &[Surface::Web, Surface::Cli, Surface::Internal],
        mutates_state: true,
    },
    CommandSpec {
        name: "settings.get",
        permission: Permission::SettingsRead,
        surfaces: &[Surface::Web, Surface::Cli],
        mutates_state: false,
    },
    CommandSpec {
        name: "settings.theme.set",
        permission: Permission::SettingsWrite,
        surfaces: &[Surface::Web, Surface::Cli],
        mutates_state: true,
    },
];

pub fn find_command_spec(name: &str) -> Option<&'static CommandSpec> {
    ALL_COMMANDS.iter().find(|spec| spec.name == name)
}
