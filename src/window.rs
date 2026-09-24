use std::collections::HashSet;
use std::ffi::OsStr;
use std::os::fd::AsFd;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use crate::color::Color;
use crate::config::APP_ID;
use crate::drag_overlay::DragOverlay;
use crate::file_chooser::FileChooser;
use crate::filetypes::{CompressionType, FileType, OutputType};
use crate::input_file::InputFile;
use crate::magick::{
    DefaultMagickArguments, IndividualConvertJob, JobFile, MagickConvertJob, ResizeArgument,
    count_frames, wait_for_child,
};
use crate::temp::{clean_dir, create_temporary_directory};
use crate::widgets::about_window::SwitcherooAbout;
use crate::widgets::image_rest::ImageRest;
use crate::widgets::image_thumbnail::ImageThumbnail;
use crate::{ZIP_BINARY_NAME, runtime};
use adw::prelude::*;
use futures::future::join_all;
use gettextrs::gettext;
use glib::{MainContext, clone, idle_add_local_once};
use gtk::accessible::Property;
use gtk::gdk::Texture;
use gtk::{gdk, gio, glib, subclass::prelude::*};
use itertools::Itertools;
use shared_child::SharedChild;
use std::sync::Arc;
use tempfile::TempDir;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResizeFilter {
    Default,
    Point,
}

enum TaskThreadMessage {
    InformOfChildProcess(Arc<SharedChild>),
    TaskTerminated(Result<(), std::io::Error>),
}

#[derive(Debug, Default, Clone)]
pub struct StopSignal(Arc<AtomicBool>);

impl StopSignal {
    pub fn stop(&self) {
        self.0.store(true, std::sync::atomic::Ordering::SeqCst);
    }

    pub fn reset(&self) {
        self.0.store(false, std::sync::atomic::Ordering::SeqCst);
    }

    pub fn is_stopped(&self) -> bool {
        self.0.load(std::sync::atomic::Ordering::SeqCst)
    }
}

#[allow(dead_code)]
impl ResizeFilter {
    pub const fn as_display_string(&self) -> Option<&str> {
        match self {
            Self::Default => None,
            Self::Point => Some("Point"),
        }
    }

    pub const fn from_index(index: usize) -> Option<Self> {
        match index {
            0 => Some(Self::Default),
            1 => Some(Self::Point),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum ResizeType {
    Percentage,
    ExactPixels,
}

impl ResizeType {
    pub const fn from_index(index: usize) -> Option<Self> {
        match index {
            0 => Some(Self::Percentage),
            1 => Some(Self::ExactPixels),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StripType {
    None,
    StripAll,
}

mod imp {
    use std::cell::{Cell, RefCell};

    use crate::config::PKGDATADIR;

    use super::*;

    use adw::subclass::prelude::AdwApplicationWindowImpl;
    use derivative::Derivative;
    use gtk::CompositeTemplate;

    #[derive(Debug, CompositeTemplate, Derivative)]
    #[derivative(Default)]
    #[template(resource = "/io/gitlab/adhami3310/Converter/blueprints/window.ui")]
    pub struct AppWindow {
        #[template_child]
        pub toast_overlay: TemplateChild<adw::ToastOverlay>,
        #[template_child]
        pub drag_overlay: TemplateChild<DragOverlay>,
        #[template_child]
        pub stack: TemplateChild<gtk::Stack>,
        #[template_child]
        pub all_images_stack: TemplateChild<gtk::Stack>,
        #[template_child]
        pub open_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub add_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub other_add_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub convert_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub cancel_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub loading_spinner: TemplateChild<gtk::Spinner>,
        #[template_child]
        pub loading_spinner_images: TemplateChild<gtk::Spinner>,
        #[template_child]
        pub image_container: TemplateChild<gtk::FlowBox>,
        #[template_child]
        pub full_image_container: TemplateChild<gtk::FlowBox>,
        #[template_child]
        pub supported_output_filetypes: TemplateChild<gtk::StringList>,
        #[template_child]
        pub progress_bar: TemplateChild<gtk::ProgressBar>,

        #[template_child]
        pub output_filetype: TemplateChild<adw::ComboRow>,
        #[template_child]
        pub output_compression: TemplateChild<adw::ActionRow>,
        #[template_child]
        pub output_compression_value: TemplateChild<gtk::Switch>,
        #[template_child]
        pub single_pdf: TemplateChild<adw::ActionRow>,
        #[template_child]
        pub single_pdf_value: TemplateChild<gtk::Switch>,

        #[template_child]
        pub quality: TemplateChild<gtk::Scale>,
        #[template_child]
        pub strip: TemplateChild<gtk::Switch>,
        #[template_child]
        pub bgcolor: TemplateChild<gtk::ColorDialogButton>,
        #[template_child]
        pub resize_filter_default: TemplateChild<gtk::ToggleButton>,
        #[template_child]
        pub resize_filter_pixel: TemplateChild<gtk::ToggleButton>,
        #[template_child]
        pub resize_filter_row: TemplateChild<adw::ActionRow>,
        #[template_child]
        pub resize_amount_row: TemplateChild<adw::ActionRow>,
        #[template_child]
        pub resize_type: TemplateChild<gtk::DropDown>,
        #[template_child]
        pub resize_width_value: TemplateChild<gtk::Entry>,
        #[template_child]
        pub resize_height_value: TemplateChild<gtk::Entry>,
        #[template_child]
        pub link_axis: TemplateChild<gtk::ToggleButton>,
        #[template_child]
        pub resize_scale_width_value: TemplateChild<gtk::Entry>,
        #[template_child]
        pub resize_scale_height_value: TemplateChild<gtk::Entry>,
        #[template_child]
        pub dpi_value: TemplateChild<gtk::Entry>,

        #[template_child]
        pub quality_row: TemplateChild<adw::ActionRow>,
        #[template_child]
        pub strip_row: TemplateChild<adw::ActionRow>,
        #[template_child]
        pub bgcolor_row: TemplateChild<adw::ActionRow>,
        #[template_child]
        pub dpi_row: TemplateChild<adw::ActionRow>,
        #[template_child]
        pub navigation: TemplateChild<adw::NavigationView>,
        #[template_child]
        pub help_overlay: TemplateChild<adw::ShortcutsDialog>,

        pub provider: gtk::CssProvider,
        #[derivative(Default(value = "gio::ListStore::new::<InputFile>()"))]
        pub input_file_store: gio::ListStore,
        #[derivative(Default(value = "gio::Settings::new(APP_ID)"))]
        pub settings: gio::Settings,
        pub cancel_signal: StopSignal,
        pub current_jobs: RefCell<Vec<Arc<SharedChild>>>,
        pub image_width: Cell<Option<u32>>,
        pub image_height: Cell<Option<u32>>,
        pub removed: RefCell<HashSet<u32>>,
        pub elements: Cell<usize>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for AppWindow {
        const NAME: &'static str = "AppWindow";
        type Type = super::AppWindow;
        type ParentType = adw::ApplicationWindow;

        fn class_init(klass: &mut Self::Class) {
            Self::bind_template(klass);
            klass.bind_template_instance_callbacks();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }

        fn new() -> Self {
            Self::default()
        }
    }

    impl ObjectImpl for AppWindow {
        fn constructed(&self) {
            self.parent_constructed();

            let theme = gtk::IconTheme::for_display(
                &gtk::gdk::Display::default().expect("cannot find display"),
            );
            theme.add_search_path(PKGDATADIR.to_owned() + "/icons");

            let obj = self.obj();
            obj.load_window_size();
            obj.setup_gactions();
        }
    }

    impl WidgetImpl for AppWindow {}
    impl WindowImpl for AppWindow {
        fn close_request(&self) -> glib::Propagation {
            if let Err(err) = self.obj().save_window_size() {
                dbg!("Failed to save window state, {}", &err);
            }

            if self.cancel_signal.is_stopped() {
                // Pass close request on to the parent
                self.parent_close_request()
            } else {
                self.obj().close_dialog();
                glib::Propagation::Stop
            }
        }
    }

    impl ApplicationWindowImpl for AppWindow {}
    impl AdwApplicationWindowImpl for AppWindow {}
}

glib::wrapper! {
    pub struct AppWindow(ObjectSubclass<imp::AppWindow>)
        @extends gtk::Widget, gtk::Window,  gtk::ApplicationWindow, adw::ApplicationWindow,
        @implements gio::ActionMap, gio::ActionGroup,
                    gtk::Root, gtk::Native, gtk::ShortcutManager,
                    gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

#[gtk::template_callbacks]
impl AppWindow {
    pub fn new<P: glib::prelude::IsA<gtk::Application>>(app: &P) -> Self {
        let win = glib::Object::builder::<AppWindow>()
            .property("application", app)
            .build();

        win.setup_filters();
        win.setup_drop_target();
        win.load_options();

        win
    }

    /// Shows a basic toast with the given text.
    fn show_toast(&self, text: &str) {
        self.imp().toast_overlay.add_toast(adw::Toast::new(text));
    }

    fn setup_gactions(&self) {
        self.add_action_entries([
            gio::ActionEntry::builder("close")
                .activate(clone!(
                    #[weak(rename_to=window)]
                    self,
                    move |_, _, _| {
                        window.destroy();
                    }
                ))
                .build(),
            gio::ActionEntry::builder("add")
                .activate(clone!(
                    #[weak(rename_to=window)]
                    self,
                    move |_, _, _| {
                        window.add_dialog();
                    }
                ))
                .build(),
            gio::ActionEntry::builder("clear")
                .activate(clone!(
                    #[weak(rename_to=window)]
                    self,
                    move |_, _, _| {
                        window.clear();
                    }
                ))
                .build(),
            gio::ActionEntry::builder("show-help-overlay")
                .activate(clone!(
                    #[weak(rename_to=window)]
                    self,
                    move |_, _, _| {
                        window.show_help_overlay();
                    }
                ))
                .build(),
            gio::ActionEntry::builder("about")
                .activate(clone!(
                    #[weak(rename_to=window)]
                    self,
                    move |_, _, _| {
                        window.show_about();
                    }
                ))
                .build(),
            gio::ActionEntry::builder("paste")
                .activate(clone!(
                    #[weak(rename_to=window)]
                    self,
                    move |_, _, _| {
                        window.load_clipboard();
                    }
                ))
                .build(),
        ]);
    }

    #[template_callback]
    fn on_add_clicked(&self) {
        self.add_dialog();
    }

    #[template_callback]
    fn on_convert_clicked(&self) {
        self.save_files();
    }

    #[template_callback]
    fn on_cancel_clicked(&self) {
        self.convert_cancel();
    }

    #[template_callback]
    fn on_output_filetype_changed(&self) {
        self.update_advanced_options();
        self.update_compression_options();
        self.update_resize();
    }

    #[template_callback]
    fn on_single_pdf_changed(&self) {
        self.update_compression_options();
    }

    #[template_callback]
    fn on_resize_type_changed(&self) {
        self.update_resize();
    }

    #[template_callback]
    fn on_resize_width_changed(&self) {
        self.update_height_from_width();
    }

    #[template_callback]
    fn on_resize_height_changed(&self) {
        self.update_width_from_height();
    }

    #[template_callback]
    fn on_link_axis_clicked(&self) {
        if self.imp().link_axis.is_active() && self.imp().link_axis.is_visible() {
            self.imp().link_axis.set_icon_name("chain-link-symbolic");
            let old_value = self
                .imp()
                .resize_scale_width_value
                .text()
                .as_str()
                .to_owned();
            let new_value = self
                .imp()
                .resize_scale_height_value
                .text()
                .as_str()
                .to_owned();
            if old_value != new_value && !new_value.is_empty() {
                self.imp().resize_scale_width_value.set_text(&new_value);
            }
            self.update_width_from_height();
        } else {
            self.imp()
                .link_axis
                .set_icon_name("chain-link-loose-symbolic");
        }
    }

    #[template_callback]
    fn on_resize_scale_height_changed(&self) {
        if self.imp().link_axis.is_active() && self.imp().link_axis.is_visible() {
            let old_value = self
                .imp()
                .resize_scale_width_value
                .text()
                .as_str()
                .to_owned();
            let new_value = self
                .imp()
                .resize_scale_height_value
                .text()
                .as_str()
                .to_owned();
            if old_value != new_value && !new_value.is_empty() {
                self.imp().resize_scale_width_value.set_text(&new_value);
            }
        }
    }

    #[template_callback]
    fn on_resize_scale_width_changed(&self) {
        if self.imp().link_axis.is_active() && self.imp().link_axis.is_visible() {
            let old_value = self
                .imp()
                .resize_scale_height_value
                .text()
                .as_str()
                .to_owned();
            let new_value = self
                .imp()
                .resize_scale_width_value
                .text()
                .as_str()
                .to_owned();
            if old_value != new_value && !new_value.is_empty() {
                self.imp().resize_scale_height_value.set_text(&new_value);
            }
        }
    }

    #[template_callback]
    fn on_resize_filter_default_toggled(&self, button: &gtk::ToggleButton) {
        if button.is_active() == self.imp().resize_filter_pixel.is_active() {
            self.imp()
                .resize_filter_pixel
                .set_active(!button.is_active());
        }
    }

    #[template_callback]
    fn on_resize_filter_pixel_toggled(&self, button: &gtk::ToggleButton) {
        if button.is_active() == self.imp().resize_filter_default.is_active() {
            self.imp()
                .resize_filter_default
                .set_active(!button.is_active());
        }
    }

    #[template_callback]
    fn on_bgcolor_changed(button: &gtk::ColorDialogButton) {
        let hex = Color::from(button.rgba()).as_hex_string();
        button
            .first_child()
            .unwrap()
            .update_property(&[Property::Label(
                &gettext("New transparency layer color: {}").replace("{}", &hex),
            )]);
    }

    fn setup_filters(&self) {
        let imp = self.imp();
        imp.image_container.set_filter_func(clone!(
            #[weak(rename_to=this)]
            self,
            #[upgrade_or_default]
            move |f| {
                return (f.index() as usize) >= this.imp().elements.get()
                    || !this.imp().removed.borrow().contains(&(f.index() as u32));
            }
        ));
        imp.full_image_container.set_filter_func(clone!(
            #[weak(rename_to=this)]
            self,
            #[upgrade_or_default]
            move |f| {
                return !this.imp().removed.borrow().contains(&(f.index() as u32));
            }
        ));
    }

    fn setup_drop_target(&self) {
        let drop_target = gtk::DropTarget::builder()
            .name("file-drop-target")
            .actions(gdk::DragAction::COPY)
            .formats(&gdk::ContentFormats::for_type(gdk::FileList::static_type()))
            .build();

        drop_target.connect_drop(clone!(
            #[weak(rename_to=win)]
            self,
            #[upgrade_or_default]
            move |_, value, _, _| {
                if let Ok(file_list) = value.get::<gdk::FileList>() {
                    if file_list.files().is_empty() {
                        win.show_toast(&gettext("Unable to access dropped files"));
                        return false;
                    }

                    let input_files = file_list.files().iter().map(InputFile::new).collect_vec();
                    win.open_files(input_files);
                    return true;
                }

                false
            }
        ));

        self.imp().drag_overlay.set_drop_target(&drop_target);
    }

    fn show_about(&self) {
        SwitcherooAbout::show(self);
    }

    fn show_help_overlay(&self) {
        self.imp().help_overlay.present(Some(self));
    }

    fn close_dialog(&self) {
        let stop_converting_dialog = adw::AlertDialog::new(
            Some(&gettext("Stop converting?")),
            Some(&gettext("You will lose all progress.")),
        );

        stop_converting_dialog.add_response("cancel", &gettext("_Cancel"));
        stop_converting_dialog.add_response("stop", &gettext("_Stop"));
        stop_converting_dialog
            .set_response_appearance("stop", adw::ResponseAppearance::Destructive);
        stop_converting_dialog.connect_response(
            None,
            clone!(
                #[weak(rename_to=this)]
                self,
                move |_, response_id| {
                    if response_id == "stop" {
                        this.imp().cancel_signal.stop();
                        let mut current_jobs = this.imp().current_jobs.borrow_mut();
                        for x in current_jobs.iter() {
                            match x.kill() {
                                Ok(()) => {}
                                Err(_) => {
                                    x.wait().ok();
                                }
                            }
                        }
                        current_jobs.clear();
                        this.close();
                    }
                }
            ),
        );
        stop_converting_dialog.present(Some(self));
    }

    fn set_convert_progress(&self, done: usize, total: usize) {
        let msg = format!("{done}/{total}");
        self.imp().progress_bar.set_text(Some(&msg));
        self.imp()
            .progress_bar
            .set_fraction((done as f64) / (total as f64));
    }

    fn set_collecting_progress(&self) {
        let msg = gettext("Collecting files");
        self.imp().progress_bar.set_text(Some(&msg));
    }

    pub fn load_clipboard(&self) {
        let clipboard = self.clipboard();
        if clipboard.formats().contain_mime_type("image/png") {
            MainContext::default().spawn_local(clone!(
                #[weak(rename_to=this)]
                self,
                async move {
                    let t = clipboard.read_texture_future().await;
                    if let Ok(Some(t)) = t {
                        let interim = JobFile::from_clipboard();
                        t.save_to_png(interim.as_filename()).ok();
                        let file =
                            InputFile::new(&gio::File::for_path(interim.as_filename())).unwrap();
                        this.open_success(vec![file]);
                    }
                }
            ));
        } else if clipboard
            .formats()
            .contain_mime_type("application/vnd.portal.files")
        {
            MainContext::default().spawn_local(clone!(
                #[weak(rename_to=this)]
                self,
                async move {
                    let t = clipboard.read_text_future().await.unwrap().unwrap();
                    let files = t
                        .lines()
                        .filter_map(|p| InputFile::new(&gio::File::for_path(p)))
                        .collect();
                    this.open_success(files);
                }
            ));
        }
    }

    fn open_success(&self, mut files: Vec<InputFile>) {
        let prev_files = self.active_files();
        let prev_files_paths = prev_files
            .iter()
            .map(super::input_file::InputFile::path)
            .collect_vec();
        files = files
            .into_iter()
            .filter(|f| !prev_files_paths.contains(&f.path()))
            .chain(prev_files)
            .filter(super::input_file::InputFile::exists)
            .collect();

        self.imp().input_file_store.remove_all();
        self.imp().removed.replace(HashSet::new());

        self.switch_to_stack_loading_generally();

        for file in &files {
            self.imp().input_file_store.append(file);
        }

        let _ = fdlimit::raise_fd_limit();

        self.load_frames();
    }

    fn load_frames(&self) {
        let files = self.files();
        let file_paths = files
            .iter()
            .map(super::input_file::InputFile::path)
            .collect_vec();

        let (sender, receiver) = async_channel::bounded(1);

        std::thread::spawn(move || {
            let jobs = file_paths
                .into_iter()
                .map(|file_path| async move { count_frames(&file_path).await })
                .collect_vec();

            let res = runtime().block_on(join_all(jobs));

            sender.send_blocking(res).expect("Concurrency Issues");
        });

        glib::spawn_future_local(clone!(
            #[weak(rename_to=this)]
            self,
            async move {
                if let Ok(image_info) = receiver.recv().await {
                    // If any file could not be identified, show it as invalid
                    // rather than proceeding (which would panic later when
                    // building the conversion jobs).
                    let Ok(image_info) = image_info.into_iter().collect::<Result<Vec<_>, _>>()
                    else {
                        this.switch_to_stack_invalid_image();
                        return;
                    };

                    let real_files = files.clone();
                    for (file, (frame_count, dimensions)) in
                        real_files.iter().zip(image_info.iter())
                    {
                        file.set_frame_count(*frame_count);
                        let dimensions = *dimensions;
                        idle_add_local_once(clone!(
                            #[weak(rename_to=ff)]
                            file,
                            move || {
                                if let Some((width, height)) = dimensions {
                                    ff.set_width(width);
                                    ff.set_height(height);
                                }
                            }
                        ));
                        glib::MainContext::default().iteration(true);
                    }
                    idle_add_local_once(clone!(
                        #[weak(rename_to=these)]
                        this,
                        move || {
                            these.load_pixbuf();
                        }
                    ));
                }
            }
        ));
    }

    fn remove_file(&self, i: u32) {
        self.imp().removed.borrow_mut().insert(i);
        if self.files_count() == 0 {
            self.clear();
        } else {
            self.construct_short_thumbnail();
            self.update_options();
        }
    }

    pub fn clear(&self) {
        self.imp().input_file_store.remove_all();

        self.switch_to_stack_welcome();
    }

    fn construct_short_thumbnail(&self) {
        let imp = self.imp();

        let input_files_count = self.files().len();

        let mut elements = 0;
        let mut visible = 0;

        while visible < 6 && elements < input_files_count {
            if !imp.removed.borrow().contains(&(elements as u32)) {
                visible += 1;
            }
            elements += 1;
        }

        let mut remaining_visible = false;

        let mut remaining_elements = elements;

        while !remaining_visible && remaining_elements < input_files_count {
            if !imp.removed.borrow().contains(&(remaining_elements as u32)) {
                remaining_visible = true;
            }
            remaining_elements += 1;
        }

        if remaining_visible {
            elements -= 1;
        }

        self.update_image_container(elements, remaining_visible);
    }

    fn active_files(&self) -> Vec<InputFile> {
        let removed = self.imp().removed.borrow().clone();
        self.files()
            .into_iter()
            .enumerate()
            .filter(|(i, _)| !removed.contains(&(*i as u32)))
            .map(|(_, f)| f)
            .collect_vec()
    }

    fn files(&self) -> Vec<InputFile> {
        self.imp()
            .input_file_store
            .iter::<InputFile>()
            .flatten()
            .collect_vec()
    }

    fn load_pixbuf_finished(&self) {
        let imp = self.imp();

        let files_dims = self
            .active_files()
            .into_iter()
            .map(|f| f.dimensions())
            .unique()
            .collect_vec();

        if let Some((w, h)) = match files_dims[..] {
            [Some(d)] => Some(d),
            _ => None,
        } {
            imp.image_width.set(Some(w as u32));
            imp.image_height.set(Some(h as u32));
        } else {
            imp.image_width.set(None);
            imp.image_height.set(None);
        }

        self.construct_short_thumbnail();

        idle_add_local_once(clone!(
            #[weak(rename_to=that)]
            self,
            move || {
                that.update_full_image_container();
            }
        ));

        self.update_options();
        self.switch_back_from_loading();
        self.imp()
            .all_images_stack
            .set_visible_child_name("all_images");
        if matches!(self.imp().navigation.visible_page().and_then(|x| x.tag()), Some(x) if x == "main")
        {
            self.switch_to_stack_convert();
        }
    }

    fn selected_output(&self) -> Option<FileType> {
        self.imp().output_filetype.selected_item().and_then(|o| {
            o.downcast::<gtk::StringObject>().map_or(None, |o| {
                Some(FileType::from_string(&o.string().as_str().to_lowercase()).unwrap())
            })
        })
    }

    fn selected_compression(&self) -> Option<CompressionType> {
        if self.imp().output_compression.is_visible() {
            if self.imp().output_compression_value.is_active() {
                Some(CompressionType::Zip)
            } else {
                Some(CompressionType::Directory)
            }
        } else {
            None
        }
    }

    fn files_count(&self) -> usize {
        (self.imp().input_file_store.n_items() as usize) - self.imp().removed.borrow().len()
    }

    fn load_pixbuf(&self) {
        let files = self.active_files();

        let file_path_things = files
            .iter()
            .map(|f| {
                (
                    f.kind().supports_pixbuf() && f.area().is_some_and(|x| x < 2000 * 2000), // image isn't too big
                    f.path(),
                )
            })
            .scan(0, |image_index, (should_load_thumbnail, path)| {
                // only load 10 images
                if should_load_thumbnail {
                    *image_index += 1;
                }

                if *image_index > 10 {
                    Some((false, path))
                } else {
                    Some((should_load_thumbnail, path))
                }
            })
            .collect_vec();

        let (sender, receiver) = async_channel::bounded(1);
        std::thread::spawn(move || {
            let file_paths_pixbuf = file_path_things
                .into_iter()
                .enumerate()
                .map(|(image_index, (should_load_thumbnail, path))| {
                    let sender = sender.clone();
                    async move {
                        sender
                            .send_blocking((
                                image_index,
                                if should_load_thumbnail {
                                    Some(Texture::from_filename(&path))
                                } else {
                                    None
                                },
                            ))
                            .expect("Concurrency Issues");
                    }
                })
                .collect_vec();

            runtime().block_on(join_all(file_paths_pixbuf));
        });

        let total = self.files_count();

        glib::spawn_future_local(clone!(
            #[weak(rename_to=this)]
            self,
            async move {
                let mut completed = 0;
                while let Ok((i, p)) = receiver.recv().await {
                    if let Some(Ok(p)) = p {
                        this.imp()
                            .input_file_store
                            .item(i as u32)
                            .and_downcast::<InputFile>()
                            .unwrap()
                            .set_pixbuf(p);
                    }
                    glib::MainContext::default().iteration(true);
                    completed += 1;
                    if completed == total {
                        this.load_pixbuf_finished();
                        break;
                    }
                }
            }
        ));
    }

    fn prepare_ui_for_conversion_start(&self) {
        self.imp().convert_button.set_sensitive(false);
        self.imp().progress_bar.set_text(Some(&gettext("Loading…")));
        self.imp().progress_bar.set_fraction(0.0);
    }

    fn save_conversion_options(&self) {
        self.save_resolution_settings().ok();
        self.save_selected_output().ok();
        self.save_selected_compression().ok();
    }

    fn get_path_with_index(path: &Path, index: usize) -> PathBuf {
        let mut original_file_name = path.file_name().unwrap().to_owned();
        original_file_name.push(format!("[{index}]"));
        path.with_file_name(original_file_name)
    }

    fn generate_input_output_pairing(
        input_file: &InputFile,
        output_stem: &str,
        output_type: FileType,
    ) -> Vec<IndividualConvertJob> {
        let (path, input_file_type, frame_count) = (
            input_file.path(),
            input_file.kind(),
            input_file.frame_count().get(),
        );
        match (input_file_type, output_type, frame_count) {
            (FileType::Pdf, _, c) => (0..c)
                .map(|f| IndividualConvertJob {
                    input_file: Self::get_path_with_index(&path, f),
                    input_file_type,
                    output_stem: format!("{output_stem}[{f}].{}", output_type.as_extension()),
                })
                .collect_vec(),
            (_, _, 1) => vec![IndividualConvertJob {
                input_file: Self::get_path_with_index(&path, 0),
                input_file_type,
                output_stem: format!("{output_stem}.{}", output_type.as_extension()),
            }],
            (input, output, _) if input.supports_animation() && output.supports_animation() => {
                vec![IndividualConvertJob {
                    input_file: path,
                    input_file_type,
                    output_stem: format!("{output_stem}.{}", output_type.as_extension()),
                }]
            }
            (input, _, count) if input.supports_animation() => (0..count)
                .map(|f| IndividualConvertJob {
                    input_file: Self::get_path_with_index(&path, f),
                    input_file_type,
                    output_stem: format!("{output_stem}[{f}].{}", output_type.as_extension()),
                })
                .collect_vec(),
            _ => vec![IndividualConvertJob {
                input_file: Self::get_path_with_index(&path, 0),
                input_file_type,
                output_stem: format!("{output_stem}.{}", output_type.as_extension()),
            }],
        }
    }

    fn choose_output_stem(
        chosen_output_stems: &HashSet<String>,
        preferred_output_stem: String,
    ) -> String {
        if chosen_output_stems.contains(&preferred_output_stem) {
            let mut i = 1;
            while chosen_output_stems.contains(&format!("{preferred_output_stem}_{i}")) {
                i += 1;
            }
            format!("{preferred_output_stem}_{i}")
        } else {
            preferred_output_stem
        }
    }

    fn generate_individual_convert_jobs(
        input_files: Vec<InputFile>,
        output_file_type: FileType,
        temporary_directory: &TempDir,
        pdf_dpi: usize,
        magick_arguments: &DefaultMagickArguments,
    ) -> Vec<MagickConvertJob> {
        input_files
            .into_iter()
            .map(|input_file| {
                let preferred_output_stem = input_file
                    .path()
                    .file_stem()
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .to_owned();
                (input_file, preferred_output_stem)
            })
            .sorted_by_key(|(_, preferred_output_stem)| preferred_output_stem.to_owned())
            .scan(
                HashSet::new(),
                |chosen_output_stems, (input_file, preferred_output_stem)| {
                    let chosen_output_stem =
                        Self::choose_output_stem(chosen_output_stems, preferred_output_stem);
                    chosen_output_stems.insert(chosen_output_stem.clone());

                    Some((input_file, chosen_output_stem))
                },
            )
            .flat_map(|(f, output_stem)| {
                Self::generate_input_output_pairing(&f, &output_stem, output_file_type)
            })
            .map(|job| {
                job.get_magick_job(
                    temporary_directory,
                    output_file_type,
                    pdf_dpi,
                    magick_arguments,
                )
            })
            .collect_vec()
    }

    fn wrap_shared_child_with_signal<F>(
        sender: &async_channel::Sender<TaskThreadMessage>,
        stop_flag: &StopSignal,
        shared_child_factory: F,
    ) where
        F: FnOnce() -> Result<SharedChild, std::io::Error>,
    {
        let shared_child = match shared_child_factory() {
            std::io::Result::Ok(shared_child) => shared_child,
            std::io::Result::Err(e) => {
                sender
                    .send_blocking(TaskThreadMessage::TaskTerminated(Err(e)))
                    .expect("Concurrency Issues");
                return;
            }
        };

        if stop_flag.is_stopped() {
            return;
        }

        let child_arc = std::sync::Arc::new(shared_child);
        sender
            .send_blocking(TaskThreadMessage::InformOfChildProcess(child_arc.clone()))
            .expect("Concurrency Issues");
        let output = wait_for_child(&child_arc);

        if stop_flag.is_stopped() {
            return;
        }

        sender
            .send_blocking(TaskThreadMessage::TaskTerminated(output))
            .expect("Concurrency Issues");
    }

    fn run_jobs_in_tokio_runtime<J: FnOnce() + Send + 'static>(jobs: Vec<J>) {
        std::thread::spawn(move || {
            let handles = jobs
                .into_iter()
                .map(|job| runtime().spawn_blocking(job))
                .collect_vec();

            runtime().block_on(join_all(handles));
        });
    }

    fn convert_start(&self, save_format: OutputType, path: PathBuf) {
        self.prepare_ui_for_conversion_start();
        self.imp().cancel_signal.reset();
        self.imp().current_jobs.replace(vec![]);
        self.save_conversion_options();

        let output_file_type = self.selected_output().unwrap();

        let input_files = self.active_files();

        let temporary_directory = create_temporary_directory().unwrap();

        let magick_arguments = DefaultMagickArguments {
            background: self.get_bgcolor_argument(),
            quality: self.get_quality_argument(),
            filter: self.get_filter_argument(),
            resize_arg: self.get_resize_argument(),
            strip: self.get_strip_argument(),
        };

        let magick_jobs = Self::generate_individual_convert_jobs(
            input_files,
            output_file_type,
            &temporary_directory,
            self.get_dpi_argument(),
            &magick_arguments,
        );

        let total_job_count = magick_jobs.len();

        let output_files = magick_jobs
            .iter()
            .map(|job| job.output_file.clone())
            .collect_vec();

        let (sender, receiver) = async_channel::bounded(1);

        let stop_flag = self.imp().cancel_signal.clone();

        Self::run_jobs_in_tokio_runtime(
            magick_jobs
                .into_iter()
                .map(|job| {
                    let stop_flag = stop_flag.clone();
                    let sender = sender.clone();
                    move || {
                        let mut magick_command = job.get_command();
                        if stop_flag.is_stopped() {
                            return;
                        }

                        Self::wrap_shared_child_with_signal(&sender, &stop_flag, move || {
                            SharedChild::spawn(&mut magick_command)
                        });
                    }
                })
                .collect_vec(),
        );

        let dir_path = TempDir::keep(temporary_directory);

        glib::spawn_future_local(clone!(
            #[weak(rename_to=this)]
            self,
            async move {
                let mut completed_count = 0;

                while let Ok(task_thread_message) = receiver.recv().await {
                    match task_thread_message {
                        TaskThreadMessage::InformOfChildProcess(child_process) => {
                            if stop_flag.is_stopped() {
                                match child_process.kill() {
                                    Ok(()) => {}
                                    Err(_) => {
                                        child_process.wait().ok();
                                    }
                                }
                            } else {
                                this.imp().current_jobs.borrow_mut().push(child_process);
                            }
                        }
                        TaskThreadMessage::TaskTerminated(Ok(())) => {
                            completed_count += 1;
                            this.set_convert_progress(completed_count, total_job_count);
                            if completed_count == total_job_count {
                                this.move_output(
                                    save_format,
                                    path.clone(),
                                    &output_files,
                                    dir_path,
                                );
                                break;
                            }
                        }
                        TaskThreadMessage::TaskTerminated(Err(err)) => {
                            this.convert_failed(err.to_string(), dir_path.clone());
                            break;
                        }
                    }
                }
            }
        ));

        self.switch_to_stack_converting();
    }

    fn combine_files<P: AsRef<OsStr>, Q: AsRef<OsStr>>(
        files: &[P],
        output_path: &Q,
    ) -> Result<SharedChild, std::io::Error> {
        SharedChild::spawn(
            std::process::Command::new("magick")
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .args(files)
                .arg(output_path),
        )
    }

    fn zip_files<P: AsRef<OsStr>, Q: AsRef<OsStr>>(
        files: &[P],
        output_path: &Q,
    ) -> Result<SharedChild, std::io::Error> {
        SharedChild::spawn(
            std::process::Command::new(ZIP_BINARY_NAME)
                .arg("-jFSm0")
                .arg(output_path)
                .args(files)
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped()),
        )
    }

    fn wrap_io_action_in_thread<F>(sender: async_channel::Sender<TaskThreadMessage>, action: F)
    where
        F: FnOnce() -> Result<(), std::io::Error> + Send + 'static,
    {
        std::thread::spawn(move || {
            sender
                .send_blocking(TaskThreadMessage::TaskTerminated(action()))
                .expect("Concurrency Issues");
        });
    }

    fn wrap_shared_child<F>(
        sender: async_channel::Sender<TaskThreadMessage>,
        shared_child_factory: F,
    ) where
        F: FnOnce() -> Result<SharedChild, std::io::Error> + Send + 'static,
    {
        std::thread::spawn(move || match shared_child_factory() {
            Ok(shared_child) => {
                let child_arc = std::sync::Arc::new(shared_child);
                sender
                    .send_blocking(TaskThreadMessage::InformOfChildProcess(child_arc.clone()))
                    .expect("Concurrency Issues");

                sender
                    .send_blocking(TaskThreadMessage::TaskTerminated(wait_for_child(
                        &child_arc,
                    )))
                    .expect("Concurrency Issues");
            }
            Err(err) => {
                sender
                    .send_blocking(TaskThreadMessage::TaskTerminated(Err(err)))
                    .expect("Concurrency Issues");
            }
        });
    }
}

pub trait FileOperations {
    fn add_dialog(&self);
    fn open_files(&self, files: Vec<Option<InputFile>>);
    fn save_error(&self, error: Option<&str>);
    fn save_files(&self);
    fn open_load(&self);
    fn open_error(&self, error: Option<&str>);
    fn add_success_wrapper(&self, files: Vec<InputFile>);
}

trait StackNavigation {
    fn switch_to_stack_convert(&self);
    fn switch_to_stack_converting(&self);
    fn switch_to_stack_welcome(&self);
    fn switch_to_stack_invalid_image(&self);
    fn switch_to_stack_loading(&self);
    fn switch_back_from_loading(&self);
    fn switch_to_stack_loading_generally(&self);
}

pub trait WindowUI {
    fn update_options(&self);
    fn update_output_options(&self);
    fn update_compression_options(&self);
    fn update_advanced_options(&self);
    fn update_width_from_height(&self);
    fn update_height_from_width(&self);
    fn update_resize(&self);
    fn update_full_image_container(&self);
    fn update_image_container(&self, count: usize, remaining_visible: bool);
}

trait ConvertArguments {
    fn get_quality_argument(&self) -> usize;
    fn get_dpi_argument(&self) -> usize;
    fn get_bgcolor_argument(&self) -> Color;
    fn get_filter_argument(&self) -> Option<ResizeFilter>;
    fn get_resize_argument(&self) -> ResizeArgument;
    fn get_strip_argument(&self) -> StripType;
}
trait ConvertOperations {
    fn convert_start_wrapper(&self, save_format: OutputType, path: PathBuf);
    fn move_output(
        &self,
        save_format: OutputType,
        path: PathBuf,
        output_files: &[PathBuf],
        dir_path: PathBuf,
    );
    fn convert_failed(&self, error_message: String, temp_dir_path: PathBuf);
    fn convert_success(&self, temp_dir_path: PathBuf, path: PathBuf, save_format: OutputType);
    fn convert_clean(&self, temp_dir_path: PathBuf);
    fn convert_cancel(&self);
}

trait SettingsStore {
    fn save_window_size(&self) -> Result<(), glib::BoolError>;
    fn load_window_size(&self);
    fn save_resolution_settings(&self) -> Result<(), glib::BoolError>;
    fn load_options(&self);
    fn save_selected_output(&self) -> Result<(), glib::BoolError>;
    fn load_selected_output(&self) -> FileType;
    fn save_selected_compression(&self) -> Result<(), glib::BoolError>;
    fn load_selected_compression(&self) -> CompressionType;
}

impl ConvertOperations for AppWindow {
    fn convert_start_wrapper(&self, save_format: OutputType, path: PathBuf) {
        self.convert_start(save_format, path);
    }

    fn move_output(
        &self,
        save_format: OutputType,
        path: PathBuf,
        output_files: &[PathBuf],
        dir_path: PathBuf,
    ) {
        let stop_flag = self.imp().cancel_signal.clone();
        if stop_flag.is_stopped() {
            return;
        }

        let path_r = path.clone();

        self.set_collecting_progress();
        let (sender, receiver) = async_channel::bounded(1);
        match save_format {
            OutputType::File(FileType::Pdf) if output_files.len() > 1 => {
                let cloned_output_files = output_files.to_owned();
                Self::wrap_shared_child(sender, move || {
                    Self::combine_files(&cloned_output_files, &path)
                });
            }
            OutputType::File(_) => {
                let file = output_files.first().unwrap().to_owned();

                Self::wrap_io_action_in_thread(sender, move || move_file(&file, &path));
            }
            OutputType::Compression(CompressionType::Directory) => {
                let cloned_output_files = output_files.to_owned();
                Self::wrap_io_action_in_thread(sender, move || {
                    move_files(&cloned_output_files, &path)
                });
            }
            OutputType::Compression(CompressionType::Zip) => {
                let cloned_output_files = output_files.to_owned();
                Self::wrap_shared_child(sender, move || {
                    Self::zip_files(&cloned_output_files, &path)
                });
            }
        }

        glib::spawn_future_local(clone!(
            #[weak(rename_to=this)]
            self,
            async move {
                while let Ok(message) = receiver.recv().await {
                    match message {
                        TaskThreadMessage::InformOfChildProcess(child_process) => {
                            if this.imp().cancel_signal.is_stopped() {
                                match child_process.kill() {
                                    Ok(()) => {}
                                    Err(_) => {
                                        child_process.wait().ok();
                                    }
                                }
                            } else {
                                this.imp().current_jobs.borrow_mut().push(child_process);
                            }
                        }
                        TaskThreadMessage::TaskTerminated(Ok(())) => {
                            this.convert_success(dir_path.clone(), path_r.clone(), save_format);
                            break;
                        }
                        TaskThreadMessage::TaskTerminated(Err(err)) => {
                            this.convert_failed(err.to_string(), dir_path.clone());
                            break;
                        }
                    }
                }
            }
        ));
    }

    fn convert_failed(&self, error_message: String, temp_dir_path: PathBuf) {
        self.convert_clean(temp_dir_path);
        if self.imp().cancel_signal.is_stopped() {
            return;
        }
        let mut current_jobs = self.imp().current_jobs.borrow_mut();
        self.imp().cancel_signal.stop();
        for x in current_jobs.iter() {
            match x.kill() {
                Ok(()) => {}
                Err(_) => {
                    x.wait().ok();
                }
            }
        }
        current_jobs.clear();

        let dialog = adw::AlertDialog::new(Some(&gettext("Error While Processing")), None);

        let sw = gtk::ScrolledWindow::new();
        sw.set_min_content_height(200);
        sw.set_max_content_height(400);
        sw.add_css_class("card");

        let text = gtk::Label::new(Some(&error_message));
        text.set_margin_top(12);
        text.set_margin_bottom(12);
        text.set_margin_start(12);
        text.set_margin_end(12);
        text.set_xalign(0.0);
        text.set_yalign(0.0);
        text.add_css_class("monospace");
        text.set_wrap(true);
        text.set_wrap_mode(gtk::pango::WrapMode::WordChar);
        sw.set_child(Some(&text));
        dialog.set_extra_child(Some(&sw));

        dialog.add_responses(&[
            ("ok", &gettext("_Close")),
            ("copy", &gettext("_Copy to Clipboard")),
        ]);
        dialog.set_response_appearance("copy", adw::ResponseAppearance::Suggested);
        dialog.connect_response(
            None,
            clone!(
                #[weak(rename_to=this)]
                self,
                move |d, response_id| {
                    if response_id == "copy" {
                        this.clipboard().set_text(&error_message);
                        this.show_toast(&gettext("Error copied to clipboard"));
                    }
                    d.close();
                }
            ),
        );
        dialog.present(Some(self));

        self.switch_to_stack_convert();
    }

    fn convert_success(&self, temp_dir_path: PathBuf, path: PathBuf, save_format: OutputType) {
        self.convert_clean(temp_dir_path);
        self.imp().cancel_signal.stop();
        let toast = adw::Toast::new(&gettext("Image converted"));
        toast.set_button_label(Some(&gettext("Open")));
        toast.connect_button_clicked(move |_| {
            let p = path.clone();
            runtime().spawn(async move {
                match save_format {
                    OutputType::Compression(CompressionType::Directory) => {
                        ashpd::desktop::open_uri::OpenDirectoryRequest::default()
                            .send(&std::fs::File::open(&p).unwrap().as_fd())
                            .await
                            .ok();
                    }
                    _ => {
                        ashpd::desktop::open_uri::OpenFileRequest::default()
                            .ask(true)
                            .send_file(&std::fs::File::open(&p).unwrap().as_fd())
                            .await
                            .ok();
                    }
                }
            });
        });
        self.imp().toast_overlay.add_toast(toast);
        self.switch_to_stack_convert();
    }

    fn convert_clean(&self, temp_dir_path: PathBuf) {
        clean_dir(temp_dir_path);
        self.imp().convert_button.set_sensitive(true);
    }

    fn convert_cancel(&self) {
        let stop_converting_dialog = adw::AlertDialog::new(
            Some(&gettext("Stop converting?")),
            Some(&gettext("You will lose all progress.")),
        );

        stop_converting_dialog
            .add_responses(&[("cancel", &gettext("_Cancel")), ("stop", &gettext("_Stop"))]);
        stop_converting_dialog
            .set_response_appearance("stop", adw::ResponseAppearance::Destructive);

        stop_converting_dialog.connect_response(
            None,
            clone!(
                #[weak(rename_to=this)]
                self,
                move |_, response_id| {
                    if response_id == "stop" {
                        this.imp().cancel_signal.stop();
                        let mut current_jobs = this.imp().current_jobs.borrow_mut();
                        for x in current_jobs.iter() {
                            match x.kill() {
                                Ok(()) => {}
                                Err(_) => {
                                    x.wait().ok();
                                }
                            }
                        }
                        current_jobs.clear();
                        this.switch_to_stack_convert();
                        this.show_toast(&gettext("Converting Cancelled"));
                    }
                }
            ),
        );

        stop_converting_dialog.present(Some(self));
    }
}

impl ConvertArguments for AppWindow {
    fn get_quality_argument(&self) -> usize {
        self.imp().quality.value() as usize
    }

    fn get_strip_argument(&self) -> StripType {
        if self.imp().strip.is_active() {
            StripType::StripAll
        } else {
            StripType::None
        }
    }

    fn get_dpi_argument(&self) -> usize {
        self.imp().dpi_value.text().parse().unwrap()
    }

    fn get_bgcolor_argument(&self) -> Color {
        self.imp().bgcolor.rgba().into()
    }

    fn get_filter_argument(&self) -> Option<ResizeFilter> {
        if self.imp().resize_filter_default.is_active() {
            Some(ResizeFilter::Default)
        } else {
            Some(ResizeFilter::Point)
        }
    }

    fn get_resize_argument(&self) -> ResizeArgument {
        let imp = self.imp();

        let resize_type = ResizeType::from_index(imp.resize_type.selected() as usize).unwrap();

        match resize_type {
            ResizeType::Percentage => ResizeArgument::Percentage {
                width: imp
                    .resize_scale_width_value
                    .text()
                    .to_string()
                    .parse()
                    .unwrap(),
                height: imp
                    .resize_scale_height_value
                    .text()
                    .to_string()
                    .parse()
                    .unwrap(),
            },
            ResizeType::ExactPixels => ResizeArgument::ExactPixels {
                width: imp.resize_width_value.text().to_string().parse().unwrap(),
                height: imp.resize_height_value.text().to_string().parse().unwrap(),
            },
        }
    }
}

impl WindowUI for AppWindow {
    fn update_options(&self) {
        let imp = self.imp();
        imp.resize_scale_height_value.set_text("100");
        imp.resize_scale_width_value.set_text("100");
        if let (Some(image_width), Some(image_height)) =
            (imp.image_width.get(), imp.image_height.get())
        {
            imp.resize_width_value.set_text(&image_width.to_string());
            imp.resize_height_value.set_text(&image_height.to_string());
        } else {
            imp.resize_width_value.set_text("");
            imp.resize_height_value.set_text("");
        }
        self.update_output_options();
        self.update_advanced_options();
    }

    fn update_output_options(&self) {
        let previous_option = self
            .selected_output()
            .unwrap_or_else(|| self.load_selected_output());

        let new_options = gtk::StringList::new(&[]);
        let new_list = FileType::output_formats().collect_vec();
        for ft in &new_list {
            new_options.append(&ft.as_display_string());
        }
        self.imp().output_filetype.set_model(Some(&new_options));
        if let Some(index) = new_list.into_iter().position(|p| p == &previous_option) {
            self.imp().output_filetype.set_selected(index as u32);
        }
        self.update_compression_options();
    }

    fn update_compression_options(&self) {
        let files = self.active_files();
        let multiple_files = files.len() > 1;
        let multiple_frames = multiple_files
            || files
                .iter()
                .map(|input_file| input_file.frame_count().get())
                .sum::<usize>()
                > 1;
        let output_option = self.selected_output().unwrap();
        if multiple_files || multiple_frames && !output_option.supports_animation() {
            let previous_option = self
                .selected_compression()
                .unwrap_or_else(|| self.load_selected_compression());

            let pdf_selected = matches!(output_option, FileType::Pdf);
            self.imp().single_pdf.set_visible(pdf_selected);

            let single_pdf_enabled = self.imp().single_pdf_value.state();

            self.imp()
                .output_compression
                .set_visible(!pdf_selected || !single_pdf_enabled);

            match previous_option {
                CompressionType::Zip => self.imp().output_compression_value.set_active(true),
                CompressionType::Directory => self.imp().output_compression_value.set_active(false),
            }
        } else {
            self.imp().output_compression.set_visible(false);
            self.imp().single_pdf.set_visible(false);
        }
    }

    fn update_advanced_options(&self) {
        let imp = self.imp();

        let input_files = self.active_files();
        let input_filetypes: Vec<FileType> = input_files
            .iter()
            .map(super::input_file::InputFile::kind)
            .collect();
        let Some(output_filetype) =
            FileType::output_formats().nth(imp.output_filetype.selected() as usize)
        else {
            return;
        };

        imp.quality_row.set_visible(false);
        imp.bgcolor_row.set_visible(false);
        imp.dpi_row.set_visible(false);
        imp.strip_row.set_visible(false);

        if output_filetype.is_lossy() {
            imp.quality_row.set_visible(true);
        }

        if input_filetypes
            .iter()
            .copied()
            .any(super::filetypes::FileType::supports_alpha)
        {
            imp.bgcolor_row.set_visible(true);

            if output_filetype.supports_alpha() {
                imp.bgcolor.set_rgba(
                    &gdk::RGBA::builder()
                        .red(0.00)
                        .green(0.0)
                        .blue(0.0)
                        .alpha(0.000_000_1)
                        .build(),
                );
                let color_dialog = imp.bgcolor.dialog().unwrap();
                color_dialog.set_with_alpha(true);
            } else {
                imp.bgcolor.set_rgba(&gdk::RGBA::WHITE);
                let color_dialog = imp.bgcolor.dialog().unwrap();
                color_dialog.set_with_alpha(false);
            }
        }

        if input_filetypes
            .iter()
            .all(|input_filetype| *input_filetype == FileType::Svg)
        {
            imp.resize_filter_row.set_visible(false);
        } else {
            imp.resize_filter_row.set_visible(true);
        }

        if input_filetypes.contains(&FileType::Pdf) {
            imp.dpi_row.set_visible(true);
        }

        if input_filetypes
            .iter()
            .copied()
            .any(super::filetypes::FileType::supports_metadata)
        {
            imp.strip_row.set_visible(true);
        }
    }

    fn update_width_from_height(&self) {
        if self.imp().link_axis.is_active()
            && self.imp().link_axis.is_visible()
            && let (Some(image_width), Some(image_height)) =
                (self.imp().image_width.get(), self.imp().image_height.get())
        {
            let old_value = self.imp().resize_width_value.text().as_str().to_owned();
            let other_text = self.imp().resize_height_value.text().as_str().to_owned();
            if other_text.is_empty() {
                return;
            }

            let other_way = generate_height_from_width(
                old_value.parse().unwrap_or_default(),
                (image_width, image_height),
            )
            .to_string();

            if other_way == other_text {
                return;
            }

            let new_value = generate_width_from_height(
                other_text.parse().unwrap_or_default(),
                (image_width, image_height),
            )
            .to_string();

            if old_value != new_value && new_value != "0" {
                self.imp().resize_width_value.set_text(&new_value);
            }
        }
    }

    fn update_height_from_width(&self) {
        if self.imp().link_axis.is_active()
            && self.imp().link_axis.is_visible()
            && let (Some(image_width), Some(image_height)) =
                (self.imp().image_width.get(), self.imp().image_height.get())
        {
            let old_value = self.imp().resize_height_value.text().as_str().to_owned();
            let other_text = self.imp().resize_width_value.text().as_str().to_owned();
            if other_text.is_empty() {
                return;
            }

            let other_way = generate_width_from_height(
                old_value.parse().unwrap_or_default(),
                (image_width, image_height),
            )
            .to_string();

            if other_way == other_text {
                return;
            }

            let new_value = generate_height_from_width(
                other_text.parse().unwrap_or_default(),
                (image_width, image_height),
            )
            .to_string();

            if old_value != new_value && new_value != "0" {
                self.imp().resize_height_value.set_text(&new_value);
            }
        }
    }

    fn update_resize(&self) {
        let imp = self.imp();

        let resize_type = ResizeType::from_index(imp.resize_type.selected() as usize).unwrap();
        imp.resize_height_value.set_visible(false);
        imp.resize_width_value.set_visible(false);
        imp.resize_scale_height_value.set_visible(false);
        imp.resize_scale_width_value.set_visible(false);
        imp.link_axis.set_visible(false);

        match resize_type {
            ResizeType::Percentage => {
                imp.resize_scale_width_value.set_visible(true);
                imp.resize_scale_height_value.set_visible(true);
                imp.link_axis.set_visible(true);
            }
            ResizeType::ExactPixels => {
                imp.resize_width_value.set_visible(true);
                imp.resize_height_value.set_visible(true);
                if self.imp().image_width.get().is_some() && self.imp().image_height.get().is_some()
                {
                    imp.link_axis.set_visible(true);
                }
            }
        }
    }

    fn update_full_image_container(&self) {
        let imp = self.imp();

        let input_files = self
            .files()
            .into_iter()
            .map(|f| {
                let (k, d) = (f.kind(), f.dimensions());
                (f, k, d)
            })
            .collect_vec();

        while let Some(child) = imp.full_image_container.first_child() {
            imp.full_image_container.remove(&child);
        }

        for (file_index, (input_file, file_type, dimensions)) in input_files.into_iter().enumerate()
        {
            let caption = match dimensions {
                Some((w, h)) => {
                    format!("{} · {}×{}", file_type.as_display_string(), w, h)
                }
                None => file_type.as_display_string().clone(),
            };

            let (w, h) = dimensions.unwrap_or_default();

            let image_thumbnail =
                ImageThumbnail::new(input_file.pixbuf().as_ref(), &caption, w as u32, h as u32);

            let image_flow_box_child = gtk::FlowBoxChild::new();
            image_flow_box_child.set_child(Some(&image_thumbnail));

            image_flow_box_child.update_property(&[Property::Label(&caption)]);

            imp.full_image_container.append(&image_flow_box_child);
            image_thumbnail.connect_remove_clicked(clone!(
                #[weak(rename_to=this)]
                self,
                move |_| {
                    this.remove_file(file_index as u32);
                    this.imp().image_container.invalidate_filter();
                    this.imp().full_image_container.invalidate_filter();
                }
            ));
        }
    }

    fn update_image_container(&self, count: usize, remaining_visible: bool) {
        let imp = self.imp();

        let input_files = self
            .files()
            .into_iter()
            .map(|f| {
                let (k, d) = (f.kind(), f.dimensions());
                (f, k, d)
            })
            .collect_vec();

        while let Some(child) = imp.image_container.first_child() {
            imp.image_container.remove(&child);
        }

        let removed = self.imp().removed.borrow().clone();

        for (index, (input_file, file_type, dimensions)) in
            input_files.into_iter().take(count).enumerate()
        {
            if removed.contains(&(index as u32)) {
                imp.image_container.append(&gtk::FlowBoxChild::new());
            } else {
                let caption = match dimensions {
                    Some((w, h)) => {
                        format!("{} · {}×{}", file_type.as_display_string(), w, h)
                    }
                    None => file_type.as_display_string().clone(),
                };

                let (w, h) = dimensions.unwrap_or_default();

                let image_thumbnail =
                    ImageThumbnail::new(input_file.pixbuf().as_ref(), &caption, w as u32, h as u32);

                let image_flow_box_child = gtk::FlowBoxChild::new();
                image_flow_box_child.set_child(Some(&image_thumbnail));

                image_flow_box_child.update_property(&[Property::Label(&caption)]);

                imp.image_container.append(&image_flow_box_child);
                image_thumbnail.connect_remove_clicked(clone!(
                    #[weak(rename_to=this)]
                    self,
                    move |_| {
                        this.remove_file(index as u32);
                        this.imp().image_container.invalidate_filter();
                        this.imp().full_image_container.invalidate_filter();
                    }
                ));
            }
        }

        imp.elements.replace(count);

        if remaining_visible {
            let image_rest = ImageRest::new(self.files_count() - 5);
            let image_flow_box_child = gtk::FlowBoxChild::new();
            image_flow_box_child.set_child(Some(&image_rest));
            image_flow_box_child.set_focusable(false);
            imp.image_container.append(&image_flow_box_child);
            image_rest.connect_clicked(clone!(
                #[weak(rename_to=this)]
                self,
                move |_| {
                    this.imp().navigation.push_by_tag("all_images");
                }
            ));
        }

        match self.files_count() {
            1 => {
                imp.image_container.set_hexpand(true);
                imp.image_container.set_max_children_per_line(1);
                imp.image_container.set_halign(gtk::Align::Fill);
            }
            2 => {
                imp.image_container.set_hexpand(true);
                imp.image_container.set_max_children_per_line(2);
                imp.image_container.set_halign(gtk::Align::Fill);
            }
            _ => {
                imp.image_container.set_hexpand(false);
                imp.image_container.set_max_children_per_line(3);
                imp.image_container.set_halign(gtk::Align::Baseline);
            }
        }

        imp.image_container.invalidate_filter();
    }
}

impl StackNavigation for AppWindow {
    fn switch_to_stack_convert(&self) {
        self.imp().add_button.set_visible(true);
        self.imp().stack.set_visible_child_name("stack_convert");
    }

    fn switch_to_stack_converting(&self) {
        self.imp().add_button.set_visible(false);
        self.imp().stack.set_visible_child_name("stack_converting");
    }

    fn switch_to_stack_welcome(&self) {
        self.imp().add_button.set_visible(false);
        self.imp()
            .stack
            .set_visible_child_name("stack_welcome_page");
    }

    fn switch_to_stack_invalid_image(&self) {
        self.imp().add_button.set_visible(false);
        self.imp()
            .stack
            .set_visible_child_name("stack_invalid_image");
    }

    fn switch_to_stack_loading(&self) {
        self.imp().add_button.set_visible(false);
        self.imp().stack.set_visible_child_name("stack_loading");
        self.imp().loading_spinner.start();
    }

    fn switch_back_from_loading(&self) {
        self.imp().loading_spinner.stop();
        self.imp().loading_spinner_images.stop();
        self.imp().other_add_button.set_visible(true);
    }

    fn switch_to_stack_loading_generally(&self) {
        if matches!(self.imp().navigation.visible_page().and_then(|x| x.tag()), Some(x) if x == "main")
        {
            self.switch_to_stack_loading();
        } else {
            self.imp().other_add_button.set_visible(false);
            self.imp()
                .all_images_stack
                .set_visible_child_name("stack_loading");
            self.imp().loading_spinner_images.start();
        }
    }
}

impl SettingsStore for AppWindow {
    fn save_window_size(&self) -> Result<(), glib::BoolError> {
        let imp = self.imp();

        let (width, height) = self.default_size();

        imp.settings.set_int("window-width", width)?;
        imp.settings.set_int("window-height", height)?;

        imp.settings
            .set_boolean("is-maximized", self.is_maximized())?;

        Ok(())
    }

    fn load_window_size(&self) {
        let imp = self.imp();

        let width = imp.settings.int("window-width");
        let height = imp.settings.int("window-height");
        let is_maximized = imp.settings.boolean("is-maximized");

        self.set_default_size(width, height);

        if is_maximized {
            self.maximize();
        }
    }

    fn save_resolution_settings(&self) -> Result<(), glib::BoolError> {
        let imp = self.imp();

        imp.settings
            .set_int("quality", imp.quality.value() as i32)?;
        imp.settings
            .set_int("dpi", imp.dpi_value.text().parse().unwrap())?;
        imp.settings
            .set_enum("resize-type", imp.resize_type.selected() as i32)?;

        Ok(())
    }

    fn load_options(&self) {
        let imp = self.imp();

        imp.quality
            .set_value(f64::from(imp.settings.int("quality")));
        imp.dpi_value.set_text(&imp.settings.int("dpi").to_string());
        imp.resize_type
            .set_selected(imp.settings.enum_("resize-type") as u32);
    }

    fn save_selected_output(&self) -> Result<(), glib::BoolError> {
        let imp = self.imp();

        let output_format = self.selected_output().unwrap();

        let pos = FileType::output_formats()
            .position(|&x| x == output_format)
            .unwrap();

        imp.settings.set_enum("output-format", pos as i32)?;

        Ok(())
    }

    fn load_selected_output(&self) -> FileType {
        let imp = self.imp();

        *FileType::output_formats().collect_vec()[imp.settings.enum_("output-format") as usize]
    }

    fn save_selected_compression(&self) -> Result<(), glib::BoolError> {
        let imp = self.imp();

        if let Some(output_format) = self.selected_compression() {
            let pos = CompressionType::possible_output(false)
                .position(|&x| x == output_format)
                .unwrap();

            imp.settings.set_enum("compression-format", pos as i32)?;
        }
        Ok(())
    }

    fn load_selected_compression(&self) -> CompressionType {
        let imp = self.imp();

        **CompressionType::possible_output(false)
            .collect_vec()
            .get(imp.settings.enum_("compression-format") as usize)
            .unwrap_or(&&CompressionType::Directory)
    }
}

impl FileOperations for AppWindow {
    fn open_files(&self, files: Vec<Option<InputFile>>) {
        let files = files.into_iter().flatten().collect_vec();
        if files.is_empty() {
            self.show_toast(&gettext("Unsupported filetype"));
            return;
        }
        self.add_success_wrapper(files);
    }

    fn save_error(&self, error: Option<&str>) {
        if let Some(s) = error {
            self.show_toast(s);
        }
    }

    fn save_files(&self) {
        let files = self.active_files();
        let multiple_files = files.len() > 1;
        let multiple_frames = multiple_files
            || files
                .iter()
                .map(|input_file| input_file.frame_count().get())
                .sum::<usize>()
                > 1;
        let output_option = self.selected_output().unwrap();
        let first_file_path = files.first().unwrap().path();
        let first_file_path = Path::new(&first_file_path);
        let (save_format, default_name) =
            if multiple_files || multiple_frames && !output_option.supports_animation() {
                if matches!(output_option, FileType::Pdf) && self.imp().single_pdf_value.state() {
                    (OutputType::File(FileType::Pdf), "images".to_owned())
                } else {
                    (
                        OutputType::Compression(self.selected_compression().unwrap()),
                        "images".to_owned(),
                    )
                }
            } else {
                let file_stem = first_file_path
                    .file_stem()
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .to_owned();

                (OutputType::File(output_option), file_stem)
            };

        let default_folder = first_file_path.parent().unwrap().to_owned();

        if save_format == OutputType::Compression(CompressionType::Directory) {
            FileChooser::choose_output_folder_wrapper(
                self,
                default_folder,
                Self::convert_start_wrapper,
                Self::save_error,
            );
        } else {
            FileChooser::choose_output_file_wrapper(
                self,
                format!("{default_name}.{}", save_format.as_extension()),
                save_format,
                default_folder,
                Self::convert_start_wrapper,
                Self::save_error,
            );
        }
    }

    fn add_dialog(&self) {
        FileChooser::open_files_wrapper(
            self,
            vec![],
            Self::open_load,
            Self::add_success_wrapper,
            Self::open_error,
        );
    }

    fn open_error(&self, error: Option<&str>) {
        if error.is_some() {
            self.switch_to_stack_invalid_image();
        }
    }

    fn open_load(&self) {
        self.switch_to_stack_loading_generally();
        self.imp().loading_spinner.start();
    }

    fn add_success_wrapper(&self, files: Vec<InputFile>) {
        self.open_success(files);
    }
}

/// Move a file, falling back to copy + delete when source and destination live
/// on different filesystems (where [`std::fs::rename`] fails with `EXDEV`).
fn move_file<P: AsRef<Path>, Q: AsRef<Path>>(from: &P, to: &Q) -> std::io::Result<()> {
    if matches!(std::fs::rename(from, to), Ok(())) {
        Ok(())
    } else {
        std::fs::copy(from.as_ref(), to.as_ref())?;
        std::fs::remove_file(from.as_ref())
    }
}

fn move_files<P: AsRef<Path>, Q: AsRef<Path>>(from: &[P], to: &Q) -> std::io::Result<()> {
    from.iter()
        .try_for_each(|file| move_file(file, &to.as_ref().join(file.as_ref().file_name().unwrap())))
}

fn generate_width_from_height(height: u32, image_dim: (u32, u32)) -> u32 {
    (f64::from(height) * f64::from(image_dim.0) / f64::from(image_dim.1)).round() as u32
}

fn generate_height_from_width(width: u32, image_dim: (u32, u32)) -> u32 {
    (f64::from(width) * f64::from(image_dim.1) / f64::from(image_dim.0)).round() as u32
}
