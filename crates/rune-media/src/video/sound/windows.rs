//! The sound of the shared screen on Windows (windows-medios): WASAPI's
//! process loopback (Windows 10 2004 or later), which gives everything this
//! computer plays except what RUNE's own processes play, so the call never
//! hears itself. Unlike PipeWire it does not say which application plays
//! what: another voice client's call (the official client, say) would be
//! in it. Read every 10 ms as 48 kHz stereo, mixed and encoded every 20 ms
//! as on Linux.

use std::mem::ManuallyDrop;
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

use rune_i18n::{es, t};
use windows::core::{implement, Interface, Ref, HRESULT};
use windows::Win32::Media::Audio::{
    ActivateAudioInterfaceAsync, IActivateAudioInterfaceAsyncOperation,
    IActivateAudioInterfaceCompletionHandler, IActivateAudioInterfaceCompletionHandler_Impl,
    IAudioCaptureClient, IAudioClient, AUDCLNT_BUFFERFLAGS_SILENT, AUDCLNT_SHAREMODE_SHARED,
    AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM, AUDCLNT_STREAMFLAGS_LOOPBACK,
    AUDIOCLIENT_ACTIVATION_PARAMS, AUDIOCLIENT_ACTIVATION_PARAMS_0,
    AUDIOCLIENT_ACTIVATION_TYPE_PROCESS_LOOPBACK, AUDIOCLIENT_PROCESS_LOOPBACK_PARAMS,
    PROCESS_LOOPBACK_MODE_EXCLUDE_TARGET_PROCESS_TREE, VIRTUAL_AUDIO_DEVICE_PROCESS_LOOPBACK,
    WAVEFORMATEX, WAVE_FORMAT_PCM,
};
use windows::Win32::System::Com::StructuredStorage::{
    PROPVARIANT, PROPVARIANT_0, PROPVARIANT_0_0, PROPVARIANT_0_0_0,
};
use windows::Win32::System::Com::BLOB;
use windows::Win32::System::Variant::VT_BLOB;

use super::super::windows::Com;
use super::super::{lock, Shared};
use super::{encoder, send_frame, Heard, Mix, Verdict, TICK, UNAVAILABLE};

/// How often WASAPI's buffer is read.
const READ: Duration = Duration::from_millis(10);
/// How long Windows may take to give the loopback.
const ACTIVATION: Duration = Duration::from_secs(5);
const RATE: u32 = 48_000;

const OLD_WINDOWS: &str =
    es("El sonido de la pantalla necesita Windows 10 (versión 2004) o posterior.");

/// Told when Windows activated the loopback. It keeps the parameters the
/// activation reads alive until then.
#[implement(IActivateAudioInterfaceCompletionHandler)]
struct Activated {
    done: mpsc::SyncSender<()>,
    _parameters: Box<AUDIOCLIENT_ACTIVATION_PARAMS>,
}

impl IActivateAudioInterfaceCompletionHandler_Impl for Activated_Impl {
    fn ActivateCompleted(
        &self,
        _: Ref<IActivateAudioInterfaceAsyncOperation>,
    ) -> windows::core::Result<()> {
        let _ = self.done.try_send(());
        Ok(())
    }
}

/// A borrowed view of the handler's parameters, never an owner of its blob.
/// `PROPVARIANT::drop` calls `PropVariantClear`, which would free the same
/// allocation the handler's Rust box owns. Its outer destructor must not run.
fn borrowed_activation_blob(
    parameters: &mut AUDIOCLIENT_ACTIVATION_PARAMS,
) -> ManuallyDrop<PROPVARIANT> {
    ManuallyDrop::new(PROPVARIANT {
        Anonymous: PROPVARIANT_0 {
            Anonymous: ManuallyDrop::new(PROPVARIANT_0_0 {
                vt: VT_BLOB,
                wReserved1: 0,
                wReserved2: 0,
                wReserved3: 0,
                Anonymous: PROPVARIANT_0_0_0 {
                    blob: BLOB {
                        cbSize: std::mem::size_of::<AUDIOCLIENT_ACTIVATION_PARAMS>() as u32,
                        pBlobData: parameters as *mut AUDIOCLIENT_ACTIVATION_PARAMS as *mut u8,
                    },
                },
            }),
        },
    })
}

/// An audio client of everything played but this process's tree.
fn loopback() -> windows::core::Result<IAudioClient> {
    let mut parameters = Box::new(AUDIOCLIENT_ACTIVATION_PARAMS {
        ActivationType: AUDIOCLIENT_ACTIVATION_TYPE_PROCESS_LOOPBACK,
        Anonymous: AUDIOCLIENT_ACTIVATION_PARAMS_0 {
            ProcessLoopbackParams: AUDIOCLIENT_PROCESS_LOOPBACK_PARAMS {
                TargetProcessId: std::process::id(),
                ProcessLoopbackMode: PROCESS_LOOPBACK_MODE_EXCLUDE_TARGET_PROCESS_TREE,
            },
        },
    });
    let blob = borrowed_activation_blob(&mut parameters);
    let (done, activated) = mpsc::sync_channel(1);
    let handler: IActivateAudioInterfaceCompletionHandler = Activated {
        done,
        _parameters: parameters,
    }
    .into();
    // SAFETY: the parameters live in the handler, which the operation holds
    // until it completes; the blob is read during the call.
    let operation = unsafe {
        ActivateAudioInterfaceAsync(
            VIRTUAL_AUDIO_DEVICE_PROCESS_LOOPBACK,
            &IAudioClient::IID,
            Some(&*blob),
            &handler,
        )
    }?;
    activated
        .recv_timeout(ACTIVATION)
        .map_err(|_| windows::core::Error::empty())?;
    let (mut result, mut client) = (HRESULT(0), None);
    // SAFETY: valid out-pointers, once the operation completed.
    unsafe { operation.GetActivateResult(&mut result, &mut client) }?;
    result.ok()?;
    client.ok_or_else(windows::core::Error::empty)?.cast()
}

/// 16-bit stereo samples as the mix takes them.
fn samples(bytes: &[u8]) -> Vec<f32> {
    bytes
        .chunks_exact(2)
        .map(|sample| f32::from(i16::from_le_bytes([sample[0], sample[1]])) / 32_768.0)
        .collect()
}

/// Everything WASAPI has buffered, into `mix`.
fn drain(capture: &IAudioCaptureClient, mix: &mut Mix) -> windows::core::Result<()> {
    // SAFETY: each buffer is read within the frames it says, then released.
    unsafe {
        while capture.GetNextPacketSize()? > 0 {
            let (mut data, mut frames, mut flags) = (std::ptr::null_mut(), 0, 0);
            capture.GetBuffer(&mut data, &mut frames, &mut flags, None, None)?;
            let length = frames as usize * 2;
            let silent = flags & AUDCLNT_BUFFERFLAGS_SILENT.0 as u32 != 0;
            let got = if silent || data.is_null() {
                vec![0.0; length]
            } else {
                samples(std::slice::from_raw_parts(data, length * 2))
            };
            capture.ReleaseBuffer(frames)?;
            mix.push(0, &got);
        }
    }
    Ok(())
}

pub(super) fn work(
    shared: &Arc<Shared>,
    heard: Arc<Mutex<Vec<(u32, Heard)>>>,
    mut send: impl FnMut(Vec<u8>) -> bool + 'static,
) -> Result<(), String> {
    let mut encoder = encoder()?;
    let _com = Com::start()?;
    let client = loopback().map_err(|_| t(OLD_WINDOWS).to_owned())?;
    let unavailable = |_| t(UNAVAILABLE).to_owned();
    let format = WAVEFORMATEX {
        wFormatTag: WAVE_FORMAT_PCM as u16,
        nChannels: 2,
        nSamplesPerSec: RATE,
        nAvgBytesPerSec: RATE * 4,
        nBlockAlign: 4,
        wBitsPerSample: 16,
        cbSize: 0,
    };
    // SAFETY: COM calls on this thread's own client; the format lives for
    // the call.
    let capture: IAudioCaptureClient = unsafe {
        client
            .Initialize(
                AUDCLNT_SHAREMODE_SHARED,
                AUDCLNT_STREAMFLAGS_LOOPBACK | AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM,
                // 200 ms, in 100 ns.
                2_000_000,
                0,
                &format,
                None,
            )
            .map_err(unavailable)?;
        let capture = client.GetService().map_err(unavailable)?;
        client.Start().map_err(unavailable)?;
        capture
    };
    // What the diagnostic line can say: not each application, only RUNE out.
    *lock(&heard) = vec![
        (
            0,
            Heard {
                name: "other applications".into(),
                kind: "process loopback".into(),
                verdict: Verdict::Mixed,
            },
        ),
        (
            1,
            Heard {
                name: "RUNE".into(),
                kind: "own processes".into(),
                verdict: Verdict::Own,
            },
        ),
    ];
    let mut mix = Mix::default();
    mix.add(0);
    let mut tick = Instant::now();
    let result = loop {
        if shared.stopped() {
            break Ok(());
        }
        if drain(&capture, &mut mix).is_err() {
            break Err(t(UNAVAILABLE).to_owned());
        }
        let now = Instant::now();
        // Far behind (the computer slept): the rhythm starts again.
        if now.duration_since(tick) > TICK * 10 {
            tick = now;
        }
        let mut failed = None;
        while failed.is_none() && now.duration_since(tick) >= TICK {
            tick += TICK;
            if let Some(frame) = mix.frame() {
                failed = send_frame(shared, &mut encoder, &frame, &mut send).err();
            }
        }
        if let Some(reason) = failed {
            break Err(reason);
        }
        std::thread::sleep(READ);
    };
    // SAFETY: this thread's own client.
    let _ = unsafe { client.Stop() };
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_activation_blob_leaves_its_parameters_owned_by_rust() {
        const CHILD: &str = "RUNE_TEST_BORROWED_ACTIVATION_BLOB";
        const COMPLETE: &str = "rune: activation ownership fixture completed";
        if std::env::var_os(CHILD).is_some() {
            // No activation or audio capture: only the two ownership scopes.
            // A regression calls PropVariantClear on Rust's allocation and
            // aborts this child with heap corruption when the box frees it.
            for _ in 0..128 {
                let mut parameters = Box::new(AUDIOCLIENT_ACTIVATION_PARAMS {
                    ActivationType: AUDIOCLIENT_ACTIVATION_TYPE_PROCESS_LOOPBACK,
                    Anonymous: AUDIOCLIENT_ACTIVATION_PARAMS_0 {
                        ProcessLoopbackParams: AUDIOCLIENT_PROCESS_LOOPBACK_PARAMS {
                            TargetProcessId: 42,
                            ProcessLoopbackMode: PROCESS_LOOPBACK_MODE_EXCLUDE_TARGET_PROCESS_TREE,
                        },
                    },
                });
                let owned = parameters.as_mut() as *mut AUDIOCLIENT_ACTIVATION_PARAMS as *mut u8;
                {
                    let borrowed = borrowed_activation_blob(&mut parameters);
                    // SAFETY: the helper sets VT_BLOB and the box is alive.
                    let value = unsafe { &borrowed.Anonymous.Anonymous };
                    assert_eq!(value.vt, VT_BLOB);
                    // SAFETY: the discriminant above identifies this field.
                    let view = unsafe { value.Anonymous.blob };
                    assert_eq!(
                        view.cbSize as usize,
                        std::mem::size_of::<AUDIOCLIENT_ACTIVATION_PARAMS>()
                    );
                    assert_eq!(view.pBlobData, owned);
                }
                // The box is the sole owner after the borrowed view ends.
                drop(parameters);
            }
            println!("{COMPLETE}");
            return;
        }
        // libtest omits the crate name from test names, including when this
        // source module is compiled in the standalone Windows harness.
        let qualified = concat!(
            module_path!(),
            "::the_activation_blob_leaves_its_parameters_owned_by_rust"
        );
        let (_, name) = qualified.split_once("::").unwrap();
        let child = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", name, "--nocapture"])
            .env(CHILD, "1")
            .output()
            .unwrap();
        assert!(
            child.status.success() && String::from_utf8_lossy(&child.stdout).contains(COMPLETE),
            "borrowed activation parameters must be freed only by Rust: {}\n{}\n{}",
            child.status,
            String::from_utf8_lossy(&child.stdout),
            String::from_utf8_lossy(&child.stderr)
        );
    }

    #[test]
    fn sixteen_bit_samples_become_the_mix_s() {
        let bytes = [0x00, 0x80, 0xFF, 0x7F, 0x00, 0x00, 0x00, 0x40];
        assert_eq!(samples(&bytes), [-1.0, 32_767.0 / 32_768.0, 0.0, 0.5]);
        assert!(samples(&[1]).is_empty());
    }
}
