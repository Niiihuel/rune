# Development shell for RUNE on NixOS: the compiler's C toolchain, the window
# libraries limn's backend opens, and the audio libraries rune-media links.
# `--arg withWindows true` adds the MinGW-w64 cross toolchain and Wine, to
# check, link and test RUNE for Windows from Linux (docs/ARCHITECTURE.md#windows-desde-linux).
{ pkgs ? import <nixpkgs> { }, withLogin ? false, withDocs ? false, withWindows ? false }:
let
  # winit and wgpu dlopen the first four at run time; cpal links the next two;
  # OpenH264, built from source in C++, links libstdc++; GStreamer's bindings
  # link GLib in every build, not only the login's.
  # WebKit/GTK are only for the optional, separate login process.
  login = pkgs.lib.optionals withLogin (with pkgs; [
    gtk4 webkitgtk_6_0 glib-networking
    # Rust's direct GObject bindings also need these at executable load time.
    glib cairo pango gdk-pixbuf graphene libsoup_3
  ]);
  # WebKit discovers GStreamer elements at run time. The core setup hook adds
  # these plugin directories to GST_PLUGIN_SYSTEM_PATH_1_0: base supplies
  # appsink/appsrc, good supplies autoaudiosink, and bad supplies fakevideosink
  # and webvttenc used by its media and subtitle pipelines.
  # Keep plugins out of the executable RPATH; they are loaded by GStreamer.
  loginMedia = pkgs.lib.optionals withLogin (with pkgs.gst_all_1; [
    gstreamer gst-plugins-base gst-plugins-good gst-plugins-bad
  ]);
  # The GPU's H264 encoder and decoder (VA-API, NVENC/NVDEC) run in
  # GStreamer: core and base are linked; the plugins that hold them (va,
  # nvcodec, h264parse in bad; videoconvert in base) are found at run time.
  # WebM and MKV videos in conversations too: matroskademux, vp8dec and
  # vp9dec in good; av1dec (libaom) and openh264dec in bad.
  gst = with pkgs.gst_all_1; [ gstreamer gst-plugins-base ];
  gstPlugins = with pkgs.gst_all_1; [ gst-plugins-base gst-plugins-good gst-plugins-bad ];
  # The `x86_64-pc-windows-gnu` target: MinGW-w64's GCC compiles the C that
  # crates bundle (SQLite, libopus, ring) and links; its winpthreads is what
  # Rust's standard library for that target links. Releases are built with
  # MSVC on Windows itself; this is a check from here, not a release.
  mingw = pkgs.pkgsCross.mingwW64;
  runtime = pkgs.lib.makeLibraryPath ((with pkgs; [ wayland libxkbcommon vulkan-loader libGL alsa-lib pipewire stdenv.cc.cc.lib glib ]) ++ gst ++ login);
in
pkgs.mkShell ({
  # dbus-daemon runs isolated FileChooser contract fixtures, never the session bus.
  # cmake builds the libopus that the `opus` crate bundles for calls; nasm
  # speeds up the OpenH264 that `openh264` builds from source for video.
  nativeBuildInputs = with pkgs; [ stdenv.cc pkg-config rustPlatform.bindgenHook dbus cmake nasm ]
    ++ pkgs.lib.optionals withWindows [ mingw.stdenv.cc pkgs.wine64 ];
  buildInputs = (with pkgs; [ wayland libxkbcommon alsa-lib pipewire ]) ++ gst ++ gstPlugins ++ login ++ loginMedia;
  LD_LIBRARY_PATH = runtime;
  RUNE_TEST_DBUS_DAEMON = "${pkgs.dbus}/bin/dbus-daemon";
  # Compiled into rune-media, so a binary run outside this shell still finds
  # the GPU's encoder; inside it the setup hook's GST_PLUGIN_SYSTEM_PATH does.
  RUNE_GST_PLUGINS = pkgs.lib.makeSearchPathOutput "lib" "lib/gstreamer-1.0" gstPlugins;
  # rust-lld, Rust's default linker, does not record where Nix keeps those
  # libraries, so a binary built here failed outside this shell
  # ("libpipewire-0.3.so.0: cannot open shared object file"). The run path
  # makes it find them, and the ones it dlopens, from any shell. Only for
  # this machine's own target: a Windows build must not get a Linux run path.
  "CARGO_TARGET_${pkgs.stdenv.hostPlatform.rust.cargoEnvVarTarget}_RUSTFLAGS" = "-C link-arg=-Wl,-rpath,${runtime}";
} // pkgs.lib.optionalAttrs withDocs {
  # Optional PDF rasterizer and process sandbox. Paths are compiled into RUNE,
  # so a document cannot select an executable through PATH or a file extension.
  RUNE_PDF_RENDERER = "${pkgs.poppler-utils}/bin/pdftoppm";
  RUNE_PDF_SANDBOX = "${pkgs.bubblewrap}/bin/bwrap";
  RUNE_PDF_LIMITER = "${pkgs.util-linux}/bin/prlimit";
} // pkgs.lib.optionalAttrs withWindows {
  CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER = "${mingw.stdenv.cc.targetPrefix}gcc";
  CARGO_TARGET_X86_64_PC_WINDOWS_GNU_RUSTFLAGS = "-L native=${mingw.windows.pthreads}/lib";
  # `cargo test --target x86_64-pc-windows-gnu` runs the tests under Wine:
  # Windows' API as Wine implements it, not Windows itself. Quiet, without
  # its .NET and HTML engines, in a prefix of the checkout's target folder
  # rather than ~/.wine.
  CARGO_TARGET_X86_64_PC_WINDOWS_GNU_RUNNER = "${pkgs.wine64}/bin/wine";
  # MinGW's C++ and thread runtimes, which OpenH264's fixtures link (MSVC
  # builds take the C++ runtime statically), as Wine names folders.
  WINEPATH = pkgs.lib.concatMapStringsSep ";" (folder: "Z:" + builtins.replaceStrings [ "/" ] [ "\\" ] folder) [
    "${mingw.stdenv.cc.cc.lib}/x86_64-w64-mingw32/lib"
    "${mingw.windows.mcfgthreads}/bin"
  ];
  WINEDEBUG = "-all";
  WINEDLLOVERRIDES = "mscoree,mshtml=";
  shellHook = ''
    export WINEPREFIX="''${CARGO_TARGET_DIR:-$PWD/target}/wine"
  '';
} // pkgs.lib.optionalAttrs withLogin {
  GIO_EXTRA_MODULES = "${pkgs.glib-networking}/lib/gio/modules";
})
