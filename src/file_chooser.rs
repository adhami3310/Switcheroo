use std::path::PathBuf;

use gettextrs::gettext;
use glib::clone;
use gtk::{gio, prelude::*};

use crate::filetypes::{CompressionType, FileType, OutputType};
use crate::input_file::InputFile;
use crate::window::AppWindow;

pub struct FileChooser;

impl FileChooser {
    pub fn load_files<A, B, C>(
        files: Vec<gio::File>,
        parent: &AppWindow,
        callback_start: A,
        callback_success: B,
        callback_error: C,
    ) where
        A: Fn(&AppWindow),
        B: Fn(&AppWindow, Vec<InputFile>),
        C: Fn(&AppWindow, Option<&str>),
    {
        callback_start(parent);
        let gfiles: Vec<Option<InputFile>> = files
            .into_iter()
            .map(|file| InputFile::new(&file))
            .collect();
        let mut files = Vec::new();

        for file in gfiles.into_iter().flatten() {
            if file.kind().is_input() {
                files.push(file);
            }
        }

        if files.is_empty() {
            callback_error(parent, Some(&gettext("Unsupported filetype")));
            return;
        }

        callback_success(parent, files);
    }

    pub fn open_files_wrapper<A, B, C>(
        parent: &AppWindow,
        current_paths: Vec<PathBuf>,
        callback_start: A,
        callback_success: B,
        callback_error: C,
    ) where
        A: Fn(&AppWindow) + 'static,
        B: Fn(&AppWindow, Vec<InputFile>) + 'static,
        C: Fn(&AppWindow, Option<&str>) + 'static,
    {
        glib::MainContext::default().spawn_local(clone!(
            #[strong]
            parent,
            async move {
                Self::open_files(
                    &parent,
                    current_paths,
                    callback_start,
                    callback_success,
                    callback_error,
                )
                .await;
            }
        ));
    }

    pub async fn open_files<A, B, C>(
        parent: &AppWindow,
        current_paths: Vec<PathBuf>,
        callback_start: A,
        callback_success: B,
        callback_error: C,
    ) where
        A: Fn(&AppWindow) + 'static,
        B: Fn(&AppWindow, Vec<InputFile>) + 'static,
        C: Fn(&AppWindow, Option<&str>) + 'static,
    {
        let image_filter = gtk::FileFilter::new();
        for filter in FileType::input_formats() {
            image_filter.add_mime_type(filter.as_mime());
        }
        image_filter.set_name(Some(&gettext("Images")));

        let dialog = gtk::FileDialog::builder()
            .accept_label(gettext("_Select Images"))
            .modal(true)
            .default_filter(&image_filter)
            .build();

        let Ok(response) = dialog.open_multiple_future(Some(parent)).await else {
            callback_error(parent, None);
            return;
        };

        let files: Vec<gio::File> = response
            .into_iter()
            .map(|f| f.unwrap().downcast::<gio::File>().unwrap())
            .collect();

        if current_paths.len() == 1
            && files.len() == 1
            && current_paths[0] == files[0].path().unwrap()
        {
            return;
        }

        Self::load_files(
            files,
            parent,
            &callback_start,
            &callback_success,
            &callback_error,
        );
    }

    pub fn choose_output_file_wrapper<A, B>(
        parent: &AppWindow,
        default_name: String,
        format: OutputType,
        default_folder: String,
        callback_success: A,
        callback_error: B,
    ) where
        A: Fn(&AppWindow, OutputType, String) + 'static,
        B: Fn(&AppWindow, Option<&str>) + 'static,
    {
        glib::MainContext::default().spawn_local(clone!(
            #[strong]
            parent,
            async move {
                Self::choose_output_file(
                    &parent,
                    default_name,
                    format,
                    Some(default_folder),
                    callback_success,
                    callback_error,
                )
                .await;
            }
        ));
    }

    pub async fn choose_output_file<A, B>(
        parent: &AppWindow,
        default_name: String,
        format: OutputType,
        default_folder: Option<String>,
        callback_success: A,
        callback_error: B,
    ) where
        A: Fn(&AppWindow, OutputType, String) + 'static,
        B: Fn(&AppWindow, Option<&str>) + 'static,
    {
        let image_filter = gtk::FileFilter::new();
        image_filter.add_mime_type(format.as_mime());

        let dialog = gtk::FileDialog::builder()
            .modal(true)
            .default_filter(&image_filter)
            .build();

        dialog.set_initial_name(Some(&default_name));
        if let Some(default_folder) = default_folder {
            dialog.set_initial_folder(Some(&gio::File::for_path(default_folder)));
        }

        let Ok(file) = dialog.save_future(Some(parent)).await else {
            callback_error(parent, None);
            return;
        };

        let file_path = file.path().unwrap();

        let Some(file_extension) = file_path.extension() else {
            callback_error(parent, Some(&gettext("Unspecified filetype")));
            return;
        };

        let Some(file_extension) = OutputType::from_string(file_extension.to_str().unwrap()) else {
            callback_error(parent, Some(&gettext("Unknown filetype")));
            return;
        };

        if file_extension != format {
            callback_error(parent, Some(&gettext("Used the wrong filetype")));
            return;
        }

        callback_success(parent, format, file_path.to_str().unwrap().to_owned());
    }

    pub fn choose_output_folder_wrapper<A, B>(
        parent: &AppWindow,
        default_folder: String,
        callback_success: A,
        callback_error: B,
    ) where
        A: Fn(&AppWindow, OutputType, String) + 'static,
        B: Fn(&AppWindow, Option<&str>) + 'static,
    {
        glib::MainContext::default().spawn_local(clone!(
            #[strong]
            parent,
            async move {
                Self::choose_output_folder(
                    &parent,
                    Some(default_folder),
                    callback_success,
                    callback_error,
                )
                .await;
            }
        ));
    }

    pub async fn choose_output_folder<A, B>(
        parent: &AppWindow,
        default_folder: Option<String>,
        callback_success: A,
        callback_error: B,
    ) where
        A: Fn(&AppWindow, OutputType, String) + 'static,
        B: Fn(&AppWindow, Option<&str>) + 'static,
    {
        let dialog = gtk::FileDialog::builder().build();

        if let Some(default_folder) = default_folder {
            dialog.set_initial_folder(Some(&gio::File::for_path(default_folder)));
        }

        let Ok(file) = dialog.select_folder_future(Some(parent)).await else {
            callback_error(parent, None);
            return;
        };

        let file_path = file.path().unwrap();

        callback_success(
            parent,
            OutputType::Compression(CompressionType::Directory),
            file_path.to_str().unwrap().to_owned(),
        );
    }
}
