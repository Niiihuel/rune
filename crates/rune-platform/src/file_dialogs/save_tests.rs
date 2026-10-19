use super::*;

fn request(name: &str) -> SaveRequest {
    SaveRequest {
        id: PickId(1),
        parent: None,
        suggested_name: name.into(),
        current_folder: None,
    }
}

#[test]
fn save_request_validates_a_bounded_filename_without_trimming_or_rewriting_it() {
    for name in [
        "archivo.txt".into(),
        " archivo ü.txt ".into(),
        "   ".into(),
        "é".repeat(90),
    ] {
        let request = request(&name);
        assert_eq!(request.validate(), Ok(()));
        assert_eq!(request.suggested_name, name);
    }
    for name in [
        "".into(),
        ".".into(),
        "..".into(),
        "../other".into(),
        "a/b".into(),
        "a\\b".into(),
        "nul\0byte".into(),
        "line\nbreak".into(),
        "tab\tname".into(),
        "delete\u{7f}".into(),
        "x".repeat(181),
        "é".repeat(91),
    ] {
        assert_eq!(
            request(&name).validate(),
            Err(PickError::InvalidRequest),
            "{name:?}"
        );
    }
}

// Unix paths and file names (windows-base).
#[cfg(unix)]
#[test]
fn save_request_requires_its_own_id_and_an_absolute_bounded_optional_folder() {
    let mut zero = request("safe.txt");
    zero.id = PickId(0);
    assert_eq!(zero.validate(), Err(PickError::InvalidRequest));
    for folder in [
        PathBuf::from("relative"),
        PathBuf::from("/nul\0folder"),
        PathBuf::from(format!("/{}", "x".repeat(4096))),
    ] {
        let mut request = request("safe.txt");
        request.current_folder = Some(folder);
        assert_eq!(request.validate(), Err(PickError::InvalidRequest));
    }
    let mut valid = request("safe.txt");
    valid.current_folder = Some(PathBuf::from("/not-opened/owned-fixture"));
    assert_eq!(valid.validate(), Ok(()));
    let debug = format!("{valid:?}");
    assert!(!debug.contains("safe.txt"));
    assert!(!debug.contains("owned-fixture"));
}

// Windows (windows-base): the window a dialog is modal to, by its handle.
#[test]
fn a_win32_parent_keeps_its_handle_and_nothing_else_reads_as_one() {
    let window = ParentWindow::win32(std::num::NonZeroUsize::new(0x1_02a4).unwrap());
    assert_eq!(window.as_str(), "win32:102a4");
    assert_eq!(window.win32_handle(), Some(0x1_02a4));
    let x11 = ParentWindow::x11(std::num::NonZeroU64::new(0x2a).unwrap());
    assert_eq!(x11.win32_handle(), None);
}
