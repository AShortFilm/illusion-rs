#![no_main]
#![no_std]

mod images;

use uefi::{prelude::*, proto::loaded_image::LoadedImage, table::boot::LoadImageSource};

#[entry]
unsafe fn main(image_handle: Handle, mut system_table: SystemTable<Boot>) -> Status {
    if let Err(error) = uefi::helpers::init(&mut system_table) {
        log::error!("[0/8] Failed to initialize UEFI services ({:?})", error);
        return Status::ABORTED;
    }

    log::info!("[1/8] UEFI services initialized");

    log::info!("[2/8] Searching Illusion hypervisor (illusion.efi)..");

    match images::find_hypervisor(system_table.boot_services()) {
        Some(hypervisor_device_path) => {
            log::info!("[3/8] Found hypervisor device path");
            log::info!("[4/8] Loading hypervisor into memory..");

            match system_table.boot_services().load_image(
                image_handle,
                LoadImageSource::FromDevicePath {
                    device_path: &hypervisor_device_path,
                    from_boot_manager: false,
                },
            ) {
                Ok(handle) => {
                    // Provide detailed information about the loaded hypervisor image before starting it
                    match system_table.boot_services().open_protocol_exclusive::<LoadedImage>(handle) {
                        Ok(li) => {
                            let (base, size) = li.info();
                            log::info!("[5/8] Loaded hypervisor image: base={:#x}, size={:#x} ({} bytes)", base as usize, size, size);
                            log::debug!("[5/8] Hypervisor memory types: code={:?}, data={:?}", li.code_type(), li.data_type());
                        }
                        Err(e) => {
                            log::warn!("[5/8] Loaded hypervisor, but failed to query LoadedImage info ({:?})", e);
                        }
                    }

                    log::info!("[5/8] Transferring control to hypervisor entry (StartImage)..");
                    if let Err(error) = system_table.boot_services().start_image(handle) {
                        log::error!("Failed to start hypervisor ({:?})", error);
                        return Status::ABORTED;
                    }
                    log::info!("[5/8] Hypervisor returned control to loader");
                }
                Err(error) => {
                    log::error!("Failed to load hypervisor ({:?})", error);
                    return Status::ABORTED;
                }
            }
        }
        None => {
            log::error!("Failed to find hypervisor image");
            return Status::ABORTED;
        }
    };

    log::info!("[6/8] Searching Windows boot manager (bootmgfw.efi)..");

    match images::find_windows_boot_manager(system_table.boot_services()) {
        Some(bootmgr_device_path) => {
            log::info!("[7/8] Found Windows boot manager device path");
            log::info!("Loading boot manager into memory..");

            log::info!("Stalling for 3 seconds before handing off to Windows boot manager..");
            system_table.boot_services().stall(3_000_000);

            match system_table.boot_services().load_image(
                image_handle,
                LoadImageSource::FromDevicePath {
                    device_path: &bootmgr_device_path,
                    from_boot_manager: false,
                },
            ) {
                Ok(handle) => {
                    log::info!("[8/8] Loaded boot manager into memory, starting..");

                    if let Err(error) = system_table.boot_services().start_image(handle) {
                        log::error!("Failed to start boot manager ({:?})", error);
                        return Status::ABORTED;
                    }
                }
                Err(error) => {
                    log::error!("Failed to load boot manager ({:?})", error);
                    return Status::ABORTED;
                }
            }
        }
        None => {
            log::error!("Failed to find Windows boot manager image");
            return Status::ABORTED;
        }
    }

    Status::SUCCESS
}
