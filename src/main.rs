mod application;
#[rustfmt::skip]
mod config;
mod color;
mod drag_overlay;
mod file_chooser;
mod filetypes;
mod input_file;
mod magick;
mod temp;
mod widgets;
mod window;

use std::sync::OnceLock;

use gettextrs::{LocaleCategory, gettext};
use glib::ExitCode;
use gtk::{gio, glib};
use log::warn;
use tokio::runtime::Runtime;

use self::application::App;
use self::config::{GETTEXT_PACKAGE, LOCALEDIR, RESOURCES_FILE};

fn runtime() -> &'static Runtime {
    static RUNTIME: OnceLock<Runtime> = OnceLock::new();
    RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("Setting up tokio runtime needs to succeed.")
    })
}

const ZIP_BINARY_NAME: &str = "zip";

/// # Safety
///
/// This function calls `setlocale()`, which is not thread-safe. It should be called before any threads are spawned or POSIX signals are enabled.
unsafe fn setup_i18n() -> Result<(), std::io::Error> {
    unsafe {
        gettextrs::setlocale(LocaleCategory::LcAll, "");
    }
    gettextrs::bindtextdomain(GETTEXT_PACKAGE, LOCALEDIR)?;
    gettextrs::textdomain(GETTEXT_PACKAGE)?;
    Ok(())
}

fn main() -> ExitCode {
    // Initialize logger
    pretty_env_logger::init();

    if let Err(err) = unsafe { setup_i18n() } {
        warn!("Failed to set up internationalization: {err}");
    }

    glib::set_application_name(&gettext("Switcheroo"));

    let res = gio::Resource::load(RESOURCES_FILE).expect("Could not load gresource file");
    gio::resources_register(&res);

    let app = App::new();
    app.run()
}
