extern crate alloc;

use {
    alloc::{borrow::ToOwned, boxed::Box, vec::Vec},
    uefi::{
        prelude::*,
        proto::{
            device_path::{
                build::{media::FilePath, DevicePathBuilder},
                DevicePath,
            },
            media::{
                file::{File, FileAttribute, FileMode},
                fs::SimpleFileSystem,
            },
        },
        table::boot::{HandleBuffer, SearchType},
        CStr16, Identify,
    },
};

const WINDOWS_BOOT_MANAGER_PATH: &CStr16 = cstr16!(r"\EFI\Microsoft\Boot\bootmgfw.efi");
const HYPERVISOR_PATH: &CStr16 = cstr16!(r"\EFI\Boot\illusion.efi");

/// Represents a bootable target discovered on a specific filesystem handle.
pub(crate) struct BootTarget {
    pub device_path: Box<DevicePath>,
    pub handle: Handle,
    pub handle_index: usize,
}

/// Finds the device path for a given file path and returns the first match.
///
/// # Arguments
///
/// * `boot_services` - A reference to the UEFI boot services.
/// * `path` - The file path to search for, as a `CStr16`.
///
/// # Returns
///
/// If a device containing the specified file is found, this function returns an `Option` containing
/// a `DevicePath` to the file. If no such device is found, it returns `None`.
pub(crate) fn find_device_path(boot_services: &BootServices, path: &CStr16) -> Option<Box<DevicePath>> {
    enumerate_device_paths(boot_services, path).into_iter().map(|t| t.device_path).next()
}

/// Enumerates all device paths for a given file path across all SimpleFileSystem handles.
pub(crate) fn enumerate_device_paths(boot_services: &BootServices, path: &CStr16) -> Vec<BootTarget> {
    let handles: HandleBuffer = match boot_services.locate_handle_buffer(SearchType::ByProtocol(&SimpleFileSystem::GUID)) {
        Ok(h) => {
            log::info!("Discovered {} SimpleFileSystem handle(s) while searching for the target path", h.len());
            h
        }
        Err(_) => {
            log::error!("Failed to locate handles for SimpleFileSystem protocol");
            return Vec::new();
        }
    };

    let mut targets = Vec::new();

    for (idx, handle) in handles.iter().enumerate() {
        let idx1 = idx + 1;
        log::debug!("Checking handle {}/{}", idx1, handles.len());

        let mut file_system = match boot_services.open_protocol_exclusive::<SimpleFileSystem>(*handle) {
            Ok(fs) => fs,
            Err(_) => {
                log::debug!("open_protocol(SimpleFileSystem) failed for handle {}", idx1);
                continue;
            }
        };

        let mut root = match file_system.open_volume() {
            Ok(v) => v,
            Err(_) => {
                log::debug!("open_volume failed for handle {}", idx1);
                continue;
            }
        };

        match root.open(path, FileMode::Read, FileAttribute::READ_ONLY) {
            Ok(_) => {
                log::debug!("Target file exists on handle {}", idx1);
            }
            Err(_) => {
                log::debug!("Target file not found on handle {}", idx1);
                continue;
            }
        }

        let device_path = match boot_services.open_protocol_exclusive::<DevicePath>(*handle) {
            Ok(dp) => dp,
            Err(_) => {
                log::debug!("open_protocol(DevicePath) failed for handle {}", idx1);
                continue;
            }
        };

        let mut storage = Vec::new();
        let builder = DevicePathBuilder::with_vec(&mut storage);
        let builder = device_path.node_iter().fold(builder, |builder, item| builder.push(&item).unwrap());

        let boot_path = match builder.push(&FilePath { path_name: path }).ok().and_then(|b| b.finalize().ok()) {
            Some(p) => p,
            None => {
                log::debug!("Failed to build final device path for handle {}", idx1);
                continue;
            }
        };

        log::info!("Discovered target on handle {}/{}", idx1, handles.len());
        targets.push(BootTarget {
            device_path: boot_path.to_owned(),
            handle: *handle,
            handle_index: idx1,
        });
    }

    if targets.is_empty() {
        log::debug!("No device paths found for target");
    } else {
        log::info!("Found {} candidate target(s)", targets.len());
    }

    targets
}

/// Finds all device paths of the Windows boot manager across all attached filesystems.
pub(crate) fn find_all_windows_boot_managers(boot_services: &BootServices) -> Vec<BootTarget> {
    enumerate_device_paths(boot_services, WINDOWS_BOOT_MANAGER_PATH)
}

/// Finds the device path of the Windows boot manager (first match).
///
/// # Arguments
///
/// * `boot_services` - A reference to the UEFI boot services.
///
/// # Returns
///
/// If a device containing the Windows boot manager is found, this function returns an `Option` containing
/// a `DevicePath` to the file. If no such device is found, it returns `None`.
pub(crate) fn find_windows_boot_manager(boot_services: &BootServices) -> Option<Box<DevicePath>> {
    find_device_path(boot_services, WINDOWS_BOOT_MANAGER_PATH)
}

/// Finds the device path of the Illusion hypervisor.
///
/// # Arguments
///
/// * `boot_services` - A reference to the UEFI boot services.
///
/// # Returns
///
/// If a device containing the Illusion hypervisor is found, this function returns an `Option` containing
/// a `DevicePath` to the file. If no such device is found, it returns `None`.
pub(crate) fn find_hypervisor(boot_services: &BootServices) -> Option<Box<DevicePath>> {
    find_device_path(boot_services, HYPERVISOR_PATH)
}
