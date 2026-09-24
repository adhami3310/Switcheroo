use glib::{ParamSpec, ParamSpecEnum, ParamSpecString, Value};
use gtk::{
    gdk::{Texture, gdk_pixbuf::Pixbuf},
    gio, glib,
    prelude::*,
    subclass::prelude::*,
};
use std::num::NonZeroUsize;
use std::{
    cell::{Cell, Ref, RefCell},
    path::PathBuf,
};

use crate::filetypes::FileType;

mod imp {

    use glib::{ParamSpecBoolean, ParamSpecObject};

    use super::*;

    pub struct InputFile {
        pub path: RefCell<PathBuf>,
        pub kind: Cell<FileType>,
        pub pixbuf: RefCell<Option<Texture>>,
        pub frame_count: Cell<NonZeroUsize>,
        pub is_behind_sandbox: Cell<bool>,
        pub width: Cell<Option<usize>>,
        pub height: Cell<Option<usize>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for InputFile {
        const NAME: &'static str = "SwitcherooInputFile";
        type Type = crate::input_file::InputFile;

        fn new() -> Self {
            Self {
                path: RefCell::new(PathBuf::from("/invalid-path")),
                kind: Cell::new(FileType::Unknown),
                pixbuf: RefCell::new(None),
                frame_count: Cell::new(NonZeroUsize::MIN),
                is_behind_sandbox: Cell::new(true),
                width: Cell::new(None),
                height: Cell::new(None),
            }
        }
    }

    impl ObjectImpl for InputFile {
        fn properties() -> &'static [ParamSpec] {
            static PROPERTIES: std::sync::LazyLock<Vec<ParamSpec>> =
                std::sync::LazyLock::new(|| {
                    vec![
                        ParamSpecString::builder("path").readwrite().build(),
                        ParamSpecEnum::builder::<FileType>("kind")
                            .readwrite()
                            .build(),
                        ParamSpecObject::builder::<Pixbuf>("pixbuf")
                            .write_only()
                            .build(),
                        ParamSpecBoolean::builder("is-behind-sandbox")
                            .readwrite()
                            .build(),
                    ]
                });
            PROPERTIES.as_ref()
        }

        fn set_property(&self, _id: usize, value: &Value, pspec: &ParamSpec) {
            match pspec.name() {
                "path" => {
                    let p = value.get::<PathBuf>().expect("Value must be a PathBuf");
                    self.path.replace(p);
                }
                "kind" => {
                    let p = value.get::<FileType>().expect("Value must be a filetype");
                    self.kind.set(p);
                }
                "pixbuf" => {
                    let p = value.get::<Texture>().expect("Value must be a Pixbuf");
                    self.pixbuf.replace(Some(p));
                }
                "is-behind-sandbox" => {
                    let p = value.get::<bool>().expect("Value must be a boolean");
                    self.is_behind_sandbox.replace(p);
                }
                _ => unimplemented!(),
            }
        }

        fn property(&self, _id: usize, pspec: &ParamSpec) -> Value {
            match pspec.name() {
                "path" => self.path.borrow().to_value(),
                "kind" => self.kind.get().to_value(),
                "is-behind-sandbox" => self.is_behind_sandbox.get().to_value(),
                _ => unimplemented!(),
            }
        }
    }
}

glib::wrapper! {
    pub struct InputFile(ObjectSubclass<imp::InputFile>);
}

impl Default for InputFile {
    fn default() -> Self {
        Self::empty()
    }
}

impl InputFile {
    pub fn new(file: &gio::File) -> Option<Self> {
        let path = file.path().unwrap();
        let is_behind_sandbox = !path.starts_with("/home");

        let file_info = file
            .query_info(
                gio::FILE_ATTRIBUTE_STANDARD_CONTENT_TYPE,
                gio::FileQueryInfoFlags::NONE,
                gio::Cancellable::NONE,
            )
            .unwrap();

        let mimetype = file_info.content_type().unwrap().as_str().to_owned();

        let extension = FileType::from_mimetype(&mimetype);

        extension.map(|extension| {
            glib::Object::builder::<Self>()
                .property("path", &path)
                .property("kind", extension)
                .property("is-behind-sandbox", is_behind_sandbox)
                .build()
        })
    }

    pub fn empty() -> Self {
        glib::Object::new()
    }

    pub fn pixbuf(&self) -> Ref<'_, Option<Texture>> {
        self.imp().pixbuf.borrow()
    }

    pub fn frame_count(&self) -> NonZeroUsize {
        self.imp().frame_count.get()
    }

    pub fn width(&self) -> Option<usize> {
        self.imp().width.get()
    }

    pub fn height(&self) -> Option<usize> {
        self.imp().height.get()
    }

    pub fn dimensions(&self) -> Option<(usize, usize)> {
        let (w, h) = (self.width(), self.height());
        w.zip(h)
    }

    pub fn set_frame_count(&self, frame_count: NonZeroUsize) {
        self.imp().frame_count.replace(frame_count);
    }

    pub fn set_width(&self, f: usize) {
        self.imp().width.replace(Some(f));
    }

    pub fn set_height(&self, f: usize) {
        self.imp().height.replace(Some(f));
    }

    pub fn area(&self) -> Option<usize> {
        let (w, h) = (self.width(), self.height());
        w.and_then(|w| h.map(|h| w * h))
    }

    pub fn set_pixbuf(&self, p: Texture) {
        self.imp().pixbuf.replace(Some(p));
    }

    pub fn path(&self) -> PathBuf {
        self.imp().path.borrow().clone()
    }

    pub fn exists(&self) -> bool {
        std::path::Path::new(&self.path()).exists()
    }

    pub fn is_behind_sandbox(&self) -> bool {
        self.imp().is_behind_sandbox.get()
    }

    pub fn kind(&self) -> FileType {
        self.imp().kind.get()
    }
}
