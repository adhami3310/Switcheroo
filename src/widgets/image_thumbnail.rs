use adw::prelude::*;
use glib::{SignalHandlerId, Value};
use gtk::{gdk::Texture, gio, glib, subclass::prelude::*};

mod imp {

    use std::cell::Cell;

    use super::*;

    use glib::{ParamSpec, ParamSpecObject, ParamSpecString, ParamSpecUInt};
    use gtk::CompositeTemplate;

    #[derive(Debug, CompositeTemplate, Default)]
    #[template(resource = "/io/gitlab/adhami3310/Converter/blueprints/image-thumbnail.ui")]
    pub struct ImageThumbnail {
        #[template_child]
        pub image: TemplateChild<gtk::Image>,
        #[template_child]
        pub picture: TemplateChild<gtk::Picture>,
        #[template_child]
        pub content: TemplateChild<gtk::Label>,
        #[template_child]
        pub remove_image: TemplateChild<gtk::Button>,
        #[template_child]
        pub root: TemplateChild<gtk::Box>,
        #[template_child]
        pub child: TemplateChild<gtk::Box>,

        pub width: Cell<u32>,
        pub height: Cell<u32>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ImageThumbnail {
        const NAME: &'static str = "ImageThumbnail";
        type Type = super::ImageThumbnail;
        type ParentType = gtk::Widget;

        fn class_init(klass: &mut Self::Class) {
            Self::bind_template(klass);
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }

        fn new() -> Self {
            Self::default()
        }
    }

    impl ObjectImpl for ImageThumbnail {
        fn properties() -> &'static [ParamSpec] {
            static PROPERTIES: std::sync::LazyLock<Vec<ParamSpec>> =
                std::sync::LazyLock::new(|| {
                    vec![
                        ParamSpecObject::builder::<Texture>("image")
                            .write_only()
                            .build(),
                        ParamSpecString::builder("content").write_only().build(),
                        ParamSpecObject::builder::<gtk::Button>("remove")
                            .read_only()
                            .build(),
                        ParamSpecUInt::builder("width").write_only().build(),
                        ParamSpecUInt::builder("height").write_only().build(),
                    ]
                });
            PROPERTIES.as_ref()
        }

        fn set_property(&self, _id: usize, value: &Value, pspec: &ParamSpec) {
            match pspec.name() {
                "image" => {
                    let p = value
                        .get::<Option<&Texture>>()
                        .expect("Value must be a Pixbuf");
                    if let Some(p) = p {
                        self.picture.set_paintable(Some(p));
                        self.picture.set_visible(true);
                        self.image.set_visible(false);
                    } else {
                        self.image.set_icon_name(Some("image-symbolic"));
                        self.image.set_visible(true);
                        self.picture.set_visible(false);
                    }
                }
                "content" => {
                    let p = value.get::<&str>().expect("Value must be a string");
                    self.content.set_text(p);
                }
                "width" => {
                    let p = value.get::<u32>().expect("Value must be an usize");
                    self.width.replace(p);
                }
                "height" => {
                    let p = value.get::<u32>().expect("Value must be an usize");
                    self.height.replace(p);
                }
                _ => unimplemented!(),
            }
        }

        fn property(&self, _id: usize, pspec: &ParamSpec) -> Value {
            match pspec.name() {
                "remove_image" => self.remove_image.to_value(),
                _ => unimplemented!(),
            }
        }

        fn dispose(&self) {
            self.root.unparent();
        }
    }

    impl WidgetImpl for ImageThumbnail {
        fn measure(&self, orientation: gtk::Orientation, for_size: i32) -> (i32, i32, i32, i32) {
            let m = self.root.measure(orientation, for_size);
            let (w, h) = (self.width.get() as i32, self.height.get() as i32);
            match orientation {
                gtk::Orientation::Horizontal if h + w != 0 => (150, 150, m.2, m.3),
                _ => m,
            }
        }

        fn size_allocate(&self, width: i32, height: i32, baseline: i32) {
            self.root.allocate(width, height, baseline, None);
        }
    }
}

glib::wrapper! {
    pub struct ImageThumbnail(ObjectSubclass<imp::ImageThumbnail>)
        @extends gtk::Widget,
        @implements gtk::Buildable, gtk::Accessible, gtk::ConstraintTarget, gio::ActionMap, gio::ActionGroup, gtk::Root;
}

#[gtk::template_callbacks]
impl ImageThumbnail {
    pub fn new(image: Option<&Texture>, content: &str, width: u32, height: u32) -> Self {
        let bin = glib::Object::builder::<ImageThumbnail>()
            .property("image", image)
            .property("content", content)
            .property("width", width)
            .property("height", height)
            .build();

        bin.add_css_class("card");
        bin.add_css_class("image-thumbnail");

        bin
    }

    pub fn connect_remove_clicked<F>(&self, func: F) -> SignalHandlerId
    where
        F: Fn(&gtk::Button) + 'static,
    {
        self.imp().remove_image.connect_clicked(func)
    }
}
