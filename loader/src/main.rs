#![no_main]
#![no_std]

extern crate alloc;

mod images;

use uefi::{
    prelude::*,
    proto::{loaded_image::LoadedImage, media::block::BlockIO},
    table::boot::LoadImageSource,
};

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

    let candidates = {
        let bs = system_table.boot_services();
        images::find_all_windows_boot_managers(bs)
    };

    if candidates.is_empty() {
        log::error!("Failed to find Windows boot manager image");
        return Status::ABORTED;
    }

    // If there are multiple candidates, present a simple manual selection menu.
    let selected_device_path = if candidates.len() == 1 {
        log::info!("[7/8] Found Windows boot manager device path");
        candidates[0].device_path.as_ref()
    } else {
        log::info!("[7/8] Multiple Windows boot manager candidates detected ({}).", candidates.len());
        log::info!("Please select which one to start by pressing 1-{}.", candidates.len());
        {
            let bs = system_table.boot_services();
            for (i, target) in candidates.iter().enumerate() {
                // Try to provide some context using BlockIO information.
                let mut desc = alloc::format!("handle {}", target.handle_index);
                if let Ok(blockio) = bs.open_protocol_exclusive::<BlockIO>(target.handle) {
                    let media = blockio.media();
                    let size_bytes = (media.last_block().saturating_add(1)).saturating_mul(media.block_size() as u64);
                    let size_mb = size_bytes / (1024 * 1024) as u64;
                    desc = alloc::format!(
                        "{} | {} | {} | approx {} MiB",
                        desc,
                        if media.is_removable_media() { "removable" } else { "fixed" },
                        if media.is_logical_partition() { "partition" } else { "whole-disk" },
                        size_mb
                    );
                }
                log::info!("  {}. {}", i + 1, desc);
            }
        }
        log::info!("Press ENTER to select option 1 (default). Press ESC to abort.");
        log::info!("Defaulting to option 1 automatically in 5 seconds if no input is received.");

        // Read from console input until a valid selection is made or a timeout occurs
        let _ = system_table.stdin().reset(false);

        let mut selection: usize = 0; // default to first option
        let timeout_ms: u64 = 5_000; // 5 seconds
        let poll_interval_us: u64 = 10_000; // 10ms per poll
        let mut waited_us: u64 = 0;

        'sel_loop: loop {
            match system_table.stdin().read_key() {
                Ok(Some(key)) => {
                    use uefi::proto::console::text::{Key, ScanCode};
                    match key {
                        Key::Printable(c) => {
                            let ch: char = c.into();
                            if let Some(d) = ch.to_digit(10) {
                                let idx = d as usize;
                                if idx >= 1 && idx <= candidates.len() {
                                    selection = idx - 1;
                                    break 'sel_loop;
                                }
                            } else if ch == '\r' || ch == '\n' {
                                break 'sel_loop; // default selection (0)
                            }
                        }
                        Key::Special(ScanCode::ESCAPE) => {
                            log::error!("Selection aborted by user");
                            return Status::ABORTED;
                        }
                        _ => {}
                    }
                }
                Ok(None) => {
                    if waited_us >= timeout_ms * 1000 {
                        log::info!("No selection made within {} seconds, defaulting to option 1.", timeout_ms / 1000);
                        break 'sel_loop;
                    }
                    system_table.boot_services().stall(poll_interval_us as usize);
                    waited_us += poll_interval_us;
                }
                Err(e) => {
                    log::warn!("Failed to read key from console ({:?}), defaulting to option 1", e);
                    break 'sel_loop;
                }
            }
        }

        let target = &candidates[selection];
        log::info!("Selected candidate {} (handle {})", selection + 1, target.handle_index);
        target.device_path.as_ref()
    };

    log::info!("Loading boot manager into memory..");

    log::info!("Stalling for 3 seconds before handing off to Windows boot manager..");
    system_table.boot_services().stall(3_000_000);

    match system_table.boot_services().load_image(
        image_handle,
        LoadImageSource::FromDevicePath {
            device_path: selected_device_path,
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

    Status::SUCCESS
}
