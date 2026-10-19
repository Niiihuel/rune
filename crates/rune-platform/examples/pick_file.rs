//! Explicit local QA. This example prepares bounded descriptors and reads no contents.

#[cfg(target_os = "linux")]
use std::{os::unix::fs::MetadataExt, path::PathBuf, time::Duration};

#[cfg(target_os = "linux")]
use rune_platform::file_dialogs::{FileChooser, PickError, PickId, PickRequest};

#[cfg(target_os = "linux")]
#[tokio::main(worker_threads = 2)]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut arguments = std::env::args_os().skip(1);
    let mut request = PickRequest {
        id: PickId(1),
        max_files: 1,
        max_file_bytes: 20 * 1024 * 1024,
        max_total_bytes: 25 * 1024 * 1024,
        parent: None,
        current_folder: None,
    };
    let mut cancel_after = None;
    while let Some(argument) = arguments.next() {
        match argument.to_str() {
            Some("--folder") => request.current_folder = Some(PathBuf::from(arguments.next().ok_or("--folder needs an absolute fixture directory")?)),
            Some("--max-files") => {
                request.max_files = arguments.next().and_then(|value| value.to_str().and_then(|value| value.parse::<usize>().ok()))
                    .ok_or("--max-files needs an integer from 1 to 8")?;
            }
            Some("--cancel-after-ms") => {
                cancel_after = Some(arguments.next().and_then(|value| value.to_str().and_then(|value| value.parse::<u64>().ok()))
                    .ok_or("--cancel-after-ms needs an integer")?);
            }
            _ => return Err("usage: pick_file [--folder /absolute/fixture/directory] [--cancel-after-ms integer] [--max-files 1..8]".into()),
        }
    }
    request.validate()?;
    let mut chooser = FileChooser::session().await?;
    let (cancel, signal) = tokio::sync::watch::channel(false);
    let cancellation = cancel_after.map(|milliseconds| {
        let cancel = cancel.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(milliseconds)).await;
            cancel.send_replace(true);
        })
    });
    let result = chooser.choose_files(&request, signal).await;
    if let Some(cancellation) = cancellation {
        cancellation.abort();
    }
    match result {
        Ok(files) => {
            for file in files {
                let (_descriptor, metadata, info) = file.into_parts();
                println!(
                    "Archivo preparado: {} bytes; dispositivo {}; inodo {}.",
                    info.size_bytes,
                    metadata.dev(),
                    metadata.ino()
                );
            }
        }
        Err(PickError::Cancelled) => println!("Selección cancelada."),
        Err(error) => return Err(error.into()),
    }
    Ok(())
}

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("El selector del ejemplo sólo está disponible en Linux.");
}
