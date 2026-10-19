//! Explicit local QA of destination selection. No destination is opened or written.

#[cfg(target_os = "linux")]
use std::{path::PathBuf, time::Duration};

#[cfg(target_os = "linux")]
use rune_platform::file_dialogs::{FileChooser, PickError, PickId, SaveRequest};

#[cfg(target_os = "linux")]
#[tokio::main(worker_threads = 2)]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut arguments = std::env::args_os().skip(1);
    let mut request = SaveRequest {
        id: PickId(1),
        parent: None,
        suggested_name: "archivo.txt".into(),
        current_folder: None,
    };
    let mut expected = None;
    let mut cancel_after = None;
    while let Some(argument) = arguments.next() {
        match argument.to_str() {
            Some("--folder") => request.current_folder = Some(PathBuf::from(arguments.next().ok_or("--folder needs an absolute fixture directory")?)),
            Some("--suggested-name") => request.suggested_name = arguments.next().and_then(|value| value.into_string().ok()).ok_or("--suggested-name needs a UTF-8 filename")?,
            Some("--expect-destination") => {
                let path = PathBuf::from(arguments.next().ok_or("--expect-destination needs an absolute fixture path")?);
                if !path.is_absolute() { return Err("--expect-destination needs an absolute fixture path".into()); }
                expected = Some(path);
            }
            Some("--cancel-after-ms") => {
                cancel_after = Some(arguments.next().and_then(|value| value.to_str().and_then(|value| value.parse::<u64>().ok())).ok_or("--cancel-after-ms needs an integer")?);
            }
            _ => return Err("usage: save_file [--folder /absolute/fixture/directory] [--suggested-name filename] [--expect-destination /absolute/fixture/path] [--cancel-after-ms integer]".into()),
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
    let result = chooser.save_file(&request, signal).await;
    if let Some(cancellation) = cancellation {
        cancellation.abort();
    }
    match result {
        Ok(destination) => {
            let path = destination.into_path();
            if expected.as_ref().is_some_and(|expected| *expected != path) {
                return Err(
                    "the selected destination does not match the explicit fixture path".into(),
                );
            }
            println!("Destino seleccionado. No se abrió ni escribió el archivo.");
            if expected.is_some() {
                println!("Coincide con el destino de prueba esperado.");
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
