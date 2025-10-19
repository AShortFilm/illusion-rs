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

/// Finds the device path for a given file path.
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
    let handles: HandleBuffer = match boot_services.locate_handle_buffer(SearchType::ByProtocol(&SimpleFileSystem::GUID)) {
        Ok(h) => {
            log::info!("Discovered {} SimpleFileSystem handle(s) while searching for the target path", h.len());
            h
        }
        Err(_) => {
            log::error!("Failed to locate handles for SimpleFileSystem protocol");
            return None;
        }
    };

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

        log::info!("Selected device path for target on handle {}/{}", idx1, handles.len());
        return Some(boot_path.to_owned());
    }

    None
}

/// Finds the device path of the Windows boot manager.
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
