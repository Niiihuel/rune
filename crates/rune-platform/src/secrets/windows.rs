//! Windows Credential Manager (windows-base): one generic credential per
//! service and account, protected by the system for this user, as Secret
//! Service keeps them on Linux. The shown name and the avatar key are the
//! credential's attributes, readable by anyone with the store, as on Linux.

use rune_i18n::tr;
use windows::core::{HRESULT, HSTRING, PWSTR};
use windows::Win32::Foundation::{ERROR_NOT_FOUND, ERROR_NO_SUCH_LOGON_SESSION, FILETIME};
use windows::Win32::Security::Credentials::{
    CredDeleteW, CredEnumerateW, CredFree, CredReadW, CredWriteW, CREDENTIALW,
    CREDENTIAL_ATTRIBUTEW, CRED_FLAGS, CRED_PERSIST_LOCAL_MACHINE, CRED_TYPE_GENERIC,
};
use zeroize::Zeroizing;

use super::{Entry, Saved, SecretError, SecretFuture, SecretStore};

/// Every target RUNE writes starts so: `RUNE/{service}/{account}`.
const PREFIX: &str = "RUNE/";
/// The system's bound on a generic credential's secret.
const MAX_SECRET: usize = 5 * 512;
/// The system's bound on an attribute's value.
const MAX_ATTRIBUTE: usize = 256;

/// The user's Credential Manager.
pub struct CredentialManager;

fn target(service: &str, account: &str) -> HSTRING {
    HSTRING::from(format!("{PREFIX}{service}/{account}"))
}

fn error(error: windows::core::Error) -> SecretError {
    if error.code() == HRESULT::from_win32(ERROR_NO_SUCH_LOGON_SESSION.0) {
        // A service or a logon without a profile has no store.
        SecretError::UNAVAILABLE
    } else {
        SecretError::FAILED
    }
}

fn not_found(error: &windows::core::Error) -> bool {
    error.code() == HRESULT::from_win32(ERROR_NOT_FOUND.0)
}

/// `text` cut to `max` bytes on a character boundary.
fn cut(text: &str, max: usize) -> &str {
    let mut end = text.len().min(max);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

/// A wide string the system reads, or an empty one for null.
///
/// # Safety
/// `text` is null or a NUL-terminated string alive for the call.
unsafe fn read(text: PWSTR) -> String {
    if text.is_null() {
        return String::new();
    }
    unsafe { text.to_string() }.unwrap_or_default()
}

/// An attribute's value, as RUNE writes them: UTF-8 bytes.
///
/// # Safety
/// `credential` is one the system returned, alive for the call.
unsafe fn attribute(credential: &CREDENTIALW, keyword: &str) -> String {
    if credential.Attributes.is_null() {
        return String::new();
    }
    let attributes = unsafe {
        std::slice::from_raw_parts(credential.Attributes, credential.AttributeCount as usize)
    };
    attributes
        .iter()
        .find(|attribute| unsafe { read(attribute.Keyword) } == keyword)
        .filter(|attribute| !attribute.Value.is_null())
        .map(|attribute| {
            let bytes = unsafe {
                std::slice::from_raw_parts(attribute.Value, attribute.ValueSize as usize)
            };
            String::from_utf8_lossy(bytes).into_owned()
        })
        .unwrap_or_default()
}

fn written(time: FILETIME) -> u64 {
    (u64::from(time.dwHighDateTime) << 32) | u64::from(time.dwLowDateTime)
}

fn list(service: &str) -> Result<Vec<Entry>, SecretError> {
    let filter = HSTRING::from(format!("{PREFIX}{service}/*"));
    let mut count = 0u32;
    let mut found: *mut *mut CREDENTIALW = std::ptr::null_mut();
    // SAFETY: both out-pointers are valid; the array is freed below.
    match unsafe { CredEnumerateW(&filter, None, &mut count, &mut found) } {
        Ok(()) => {}
        Err(error) if not_found(&error) => return Ok(Vec::new()),
        Err(failed) => return Err(error(failed)),
    }
    let mut entries = Vec::new();
    // SAFETY: the system returned `count` credentials, alive until CredFree.
    for credential in unsafe { std::slice::from_raw_parts(found, count as usize) } {
        let credential = unsafe { &**credential };
        let account = unsafe { read(credential.UserName) };
        if account.is_empty() {
            continue;
        }
        entries.push((
            written(credential.LastWritten),
            Entry {
                account,
                name: unsafe { attribute(credential, "name") },
                avatar: unsafe { attribute(credential, "avatar") },
            },
        ));
    }
    // SAFETY: the array CredEnumerateW allocated, freed once.
    unsafe { CredFree(found.cast()) };
    entries.sort_by_key(|(written, _)| std::cmp::Reverse(*written));
    Ok(entries.into_iter().map(|(_, entry)| entry).collect())
}

fn load(service: &str, account: &str) -> Result<Option<Saved>, SecretError> {
    let mut found: *mut CREDENTIALW = std::ptr::null_mut();
    // SAFETY: a valid out-pointer; the credential is freed below.
    match unsafe {
        CredReadW(
            &target(service, account),
            CRED_TYPE_GENERIC,
            None,
            &mut found,
        )
    } {
        Ok(()) => {}
        Err(error) if not_found(&error) => return Ok(None),
        Err(failed) => return Err(error(failed)),
    }
    // SAFETY: the system's credential, alive until CredFree; its secret is
    // erased in place before it is freed.
    let secret = unsafe {
        let credential = &*found;
        let blob = std::slice::from_raw_parts_mut(
            credential.CredentialBlob,
            if credential.CredentialBlob.is_null() {
                0
            } else {
                credential.CredentialBlobSize as usize
            },
        );
        let secret = std::str::from_utf8(blob).map(|text| Zeroizing::new(text.to_owned()));
        zeroize::Zeroize::zeroize(blob);
        CredFree(found.cast());
        secret
    };
    Ok(Some(Saved {
        account: account.to_owned(),
        secret: secret.map_err(|_| SecretError::FAILED)?,
    }))
}

fn save(
    service: &str,
    account: &str,
    name: &str,
    avatar: &str,
    secret: &str,
) -> Result<(), SecretError> {
    if secret.len() > MAX_SECRET {
        return Err(SecretError::FAILED);
    }
    // Shown by the Credential Manager, in the language it was saved in.
    let comment = if service == super::CACHE_KEYS {
        tr!("RUNE: clave de los datos locales ({account})", account)
    } else {
        tr!("RUNE: sesión guardada de {name} ({service})", name, service)
    };
    let mut target: Vec<u16> = target(service, account)
        .iter()
        .copied()
        .chain([0])
        .collect();
    let mut comment: Vec<u16> = comment.encode_utf16().chain([0]).collect();
    let mut user: Vec<u16> = account.encode_utf16().chain([0]).collect();
    let mut keywords: Vec<Vec<u16>> = ["name", "avatar"]
        .iter()
        .map(|keyword| keyword.encode_utf16().chain([0]).collect())
        .collect();
    let mut values = [
        cut(name, MAX_ATTRIBUTE).as_bytes().to_vec(),
        cut(avatar, MAX_ATTRIBUTE).as_bytes().to_vec(),
    ];
    let mut attributes: Vec<CREDENTIAL_ATTRIBUTEW> = keywords
        .iter_mut()
        .zip(values.iter_mut())
        .filter(|(_, value)| !value.is_empty())
        .map(|(keyword, value)| CREDENTIAL_ATTRIBUTEW {
            Keyword: PWSTR(keyword.as_mut_ptr()),
            Flags: 0,
            ValueSize: value.len() as u32,
            Value: value.as_mut_ptr(),
        })
        .collect();
    let mut blob = Zeroizing::new(secret.as_bytes().to_vec());
    let credential = CREDENTIALW {
        Flags: CRED_FLAGS(0),
        Type: CRED_TYPE_GENERIC,
        TargetName: PWSTR(target.as_mut_ptr()),
        Comment: PWSTR(comment.as_mut_ptr()),
        CredentialBlobSize: blob.len() as u32,
        CredentialBlob: blob.as_mut_ptr(),
        // This computer only: a roaming profile does not carry it.
        Persist: CRED_PERSIST_LOCAL_MACHINE,
        AttributeCount: attributes.len() as u32,
        Attributes: if attributes.is_empty() {
            std::ptr::null_mut()
        } else {
            attributes.as_mut_ptr()
        },
        UserName: PWSTR(user.as_mut_ptr()),
        ..Default::default()
    };
    // SAFETY: every pointer is to a buffer above, alive for the call. The
    // write replaces the entry whole: a failure keeps the old one.
    unsafe { CredWriteW(&credential, 0) }.map_err(error)
}

fn delete(service: &str, account: &str) -> Result<(), SecretError> {
    // SAFETY: a NUL-terminated target alive for the call.
    match unsafe { CredDeleteW(&target(service, account), CRED_TYPE_GENERIC, None) } {
        Err(failed) if !not_found(&failed) => Err(error(failed)),
        _ => Ok(()),
    }
}

// Each call is local and short; the futures run on the core's runtime,
// never on the interface's thread.
impl SecretStore for CredentialManager {
    fn list<'a>(&'a self, service: &'a str) -> SecretFuture<'a, Vec<Entry>> {
        Box::pin(async move { list(service) })
    }

    fn load<'a>(&'a self, service: &'a str, account: &'a str) -> SecretFuture<'a, Option<Saved>> {
        Box::pin(async move { load(service, account) })
    }

    fn save<'a>(
        &'a self,
        service: &'a str,
        account: &'a str,
        name: &'a str,
        avatar: &'a str,
        secret: &'a str,
    ) -> SecretFuture<'a, ()> {
        Box::pin(async move { save(service, account, name, avatar, secret) })
    }

    fn delete<'a>(&'a self, service: &'a str, account: &'a str) -> SecretFuture<'a, ()> {
        Box::pin(async move { delete(service, account) })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attributes_are_cut_on_a_character_boundary() {
        assert_eq!(cut("ñandú", 3), "ña");
        assert_eq!(cut("ñandú", 1), "");
        assert_eq!(cut("corto", MAX_ATTRIBUTE), "corto");
    }

    // The runner's own store (CI, windows-latest): run on purpose with
    // `--ignored`, never on a person's computer by default.
    #[tokio::test]
    #[ignore = "writes to this user's Credential Manager"]
    async fn a_credential_is_saved_listed_loaded_and_deleted() {
        let service = format!("rune-test-{}", std::process::id());
        let store = CredentialManager;
        store
            .save(&service, "1", "Ada", "abc", "SYNTHETIC_SECRET_ONE")
            .await
            .unwrap();
        store
            .save(&service, "2", "Grace", "", "SYNTHETIC_SECRET_TWO")
            .await
            .unwrap();
        let listed = store.list(&service).await.unwrap();
        let accounts: Vec<_> = listed.iter().map(|entry| entry.account.as_str()).collect();
        assert_eq!(accounts.len(), 2, "{listed:?}");
        let loaded = store.load(&service, "1").await.unwrap().unwrap();
        assert_eq!(loaded.secret.as_str(), "SYNTHETIC_SECRET_ONE");
        // Wine keeps no attributes; Windows does.
        let ada = listed.iter().find(|entry| entry.account == "1").unwrap();
        let shown = (ada.name.clone(), ada.avatar.clone());
        for account in ["1", "2"] {
            store.delete(&service, account).await.unwrap();
        }
        assert!(store.load(&service, "1").await.unwrap().is_none());
        assert!(store.list(&service).await.unwrap().is_empty());
        // Deleting what is not there is not an error.
        store.delete(&service, "1").await.unwrap();
        assert_eq!(shown, ("Ada".into(), "abc".into()));
    }
}
