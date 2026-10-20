//! Opening a web address with the handler the person chose
//! (windows-base): `xdg-open` on Linux, `open` on macOS, the shell on
//! Windows. Nothing waits for the browser.

/// Whether `url` may be opened: an `http` or `https` address, one line.
pub fn web_address(url: &str) -> bool {
    (url.starts_with("https://") || url.starts_with("http://"))
        && !url.chars().any(char::is_control)
}

/// Open the web address `url` in the browser; anything but [`web_address`]
/// is ignored.
pub fn web(url: &str) {
    if web_address(url) {
        handle(url.as_ref());
    }
}

#[cfg(windows)]
fn handle(target: &std::ffi::OsStr) {
    crate::win::shell_open(target);
}

#[cfg(not(windows))]
fn handle(target: &std::ffi::OsStr) {
    let program = if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    // The browser it may start says what it opens on its stderr, and keeps
    // it open after RUNE: neither may reach RUNE's log.
    let opened = std::process::Command::new(program)
        .arg(target)
        .stderr(std::process::Stdio::null())
        .spawn();
    if let Err(error) = opened {
        // Authorization addresses contain temporary state; never log them.
        eprintln!("rune: could not open it with {program}: {error}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_web_addresses_of_one_line_are_opened() {
        assert!(web_address("https://example.com/a?b=c"));
        assert!(web_address("http://localhost:8080/"));
        for refused in [
            "file:///etc/passwd",
            "javascript:alert(1)",
            "ms-settings:privacy",
            "https://example.com/\nsecond",
            "C:\\Windows\\System32\\calc.exe",
            "",
        ] {
            assert!(!web_address(refused), "{refused}");
        }
    }
}
