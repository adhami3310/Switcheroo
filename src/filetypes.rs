use std::slice::Iter;

#[derive(Clone, Copy, Debug, glib::Enum, PartialEq, Default, Eq, Hash)]
#[enum_type(name = "SwitcherooFiletype")]
pub enum FileType {
    #[enum_value(name = "PNG")]
    Png,
    #[enum_value(name = "JPG")]
    Jpg,
    #[enum_value(name = "WEBP")]
    Webp,
    #[enum_value(name = "SVG")]
    Svg,
    #[enum_value(name = "HEIF")]
    Heif,
    #[enum_value(name = "HEIC")]
    Heic,
    #[enum_value(name = "BMP")]
    Bmp,
    #[enum_value(name = "AVIF")]
    Avif,
    #[enum_value(name = "JXL")]
    Jxl,
    #[enum_value(name = "TIFF")]
    Tiff,
    #[enum_value(name = "PDF")]
    Pdf,
    #[enum_value(name = "GIF")]
    Gif,
    #[enum_value(name = "ICO")]
    Ico,
    #[enum_value(name = "DDS")]
    Dds,
    #[enum_value(name = "Unknown")]
    #[default]
    Unknown,
}

use FileType::{
    Avif, Bmp, Dds, Gif, Heic, Heif, Ico, Jpg, Jxl, Pdf, Png, Svg, Tiff, Unknown, Webp,
};

impl FileType {
    pub const fn is_input(self) -> bool {
        matches!(
            self,
            Png | Jpg | Webp | Svg | Heif | Heic | Bmp | Avif | Jxl | Tiff | Pdf | Gif | Ico | Dds
        )
    }

    pub const fn supports_animation(self) -> bool {
        matches!(self, Webp | Gif | Heic | Heif)
    }

    pub const fn is_lossy(self) -> bool {
        matches!(
            self,
            Jpg | Webp | Heif | Heic | Avif | Jxl | Tiff | Pdf | Dds
        )
    }

    pub const fn supports_alpha(self) -> bool {
        matches!(
            self,
            Png | Webp | Svg | Heif | Heic | Avif | Jxl | Pdf | Ico | Gif
        )
    }

    pub const fn supports_metadata(self) -> bool {
        matches!(
            self,
            Png | Jpg | Jxl | Tiff | Pdf | Svg | Gif | Webp | Heif | Heic | Avif | Bmp
        )
    }

    pub const fn supports_pixbuf(self) -> bool {
        !matches!(self, Pdf | Dds | Ico)
    }

    pub const fn is_output(self) -> bool {
        matches!(
            self,
            Png | Jpg | Webp | Heif | Heic | Bmp | Avif | Jxl | Tiff | Pdf | Gif | Ico | Dds
        )
    }

    pub fn iterator() -> Iter<'static, Self> {
        static FILETYPES: [FileType; 14] = [
            Png, Jpg, Webp, Svg, Heif, Heic, Bmp, Avif, Jxl, Tiff, Pdf, Gif, Ico, Dds,
        ];
        FILETYPES.iter()
    }

    pub fn input_formats() -> Iter<'static, Self> {
        static FILETYPES: [FileType; 14] = [
            Png, Jpg, Webp, Svg, Heif, Heic, Bmp, Avif, Jxl, Tiff, Pdf, Gif, Ico, Dds,
        ];
        FILETYPES.iter()
    }

    pub fn output_formats() -> Iter<'static, Self> {
        static ALL_FILETYPES: [FileType; 13] = [
            Avif, Bmp, Dds, Gif, Heic, Heif, Ico, Jpg, Jxl, Pdf, Png, Tiff, Webp,
        ];

        ALL_FILETYPES.iter()
    }

    pub const fn as_mime(self) -> &'static str {
        match self {
            Png => "image/png",
            Jpg => "image/jpeg",
            Webp => "image/webp",
            Svg => "image/svg+xml",
            Heif => "image/heif",
            Heic => "image/heic",
            Bmp => "image/bmp",
            Avif => "image/avif",
            Jxl => "image/jxl",
            Tiff => "image/tiff",
            Pdf => "application/pdf",
            Gif => "image/gif",
            Ico => "image/x-icon",
            Dds => "image/vnd-ms.dds",
            Unknown => "",
        }
    }

    pub fn from_mimetype(mimetype: &str) -> Option<Self> {
        match mimetype {
            "image/png" => Some(Png),
            "image/jpeg" | "image/jpg" => Some(Jpg),
            "image/webp" => Some(Webp),
            "image/svg+xml" => Some(Svg),
            "image/heif" => Some(Heif),
            "image/heic" => Some(Heic),
            "image/bmp" => Some(Bmp),
            "image/avif" => Some(Avif),
            "image/jxl" => Some(Jxl),
            "image/tiff" => Some(Tiff),
            "application/pdf" => Some(Pdf),
            "image/gif" => Some(Gif),
            "image/x-icon" => Some(Ico),
            "image/vnd-ms.dds" => Some(Dds),
            _ => None,
        }
    }

    pub const fn as_extension(&self) -> &str {
        match self {
            Png => "png",
            Jpg => "jpg",
            Webp => "webp",
            Svg => "svg",
            Heif => "heif",
            Heic => "heic",
            Bmp => "bmp",
            Avif => "avif",
            Jxl => "jxl",
            Tiff => "tiff",
            Pdf => "pdf",
            Gif => "gif",
            Ico => "ico",
            Dds => "dds",
            Unknown => "",
        }
    }

    pub fn as_display_string(self) -> String {
        self.as_extension().to_uppercase()
    }

    pub fn from_string(extension: &str) -> Option<Self> {
        match extension {
            "png" => Some(Png),
            "jpg" | "jpeg" => Some(Jpg),
            "webp" => Some(Webp),
            "svg" => Some(Svg),
            "heif" => Some(Heif),
            "heic" => Some(Heic),
            "bmp" => Some(Bmp),
            "avif" => Some(Avif),
            "jxl" => Some(Jxl),
            "tiff" => Some(Tiff),
            "pdf" => Some(Pdf),
            "gif" => Some(Gif),
            "ico" => Some(Ico),
            "dds" => Some(Dds),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, glib::Enum, PartialEq, Eq, Hash)]
#[enum_type(name = "SwitcherooCompressionType")]
pub enum CompressionType {
    #[enum_value(name = "ZIP")]
    Zip,
    #[enum_value(name = "Directory")]
    Directory,
}

use CompressionType::{Directory, Zip};

impl CompressionType {
    pub const fn is_compression(self) -> bool {
        matches!(self, Zip)
    }

    pub fn iterator() -> Iter<'static, Self> {
        static COMPRESSION_TYPES: [CompressionType; 2] = [Zip, Directory];
        COMPRESSION_TYPES.iter()
    }

    pub fn compression_formats() -> Iter<'static, Self> {
        static COMPRESSION_TYPES: [CompressionType; 1] = [Zip];
        COMPRESSION_TYPES.iter()
    }

    pub fn possible_output(sandboxed: bool) -> Iter<'static, Self> {
        static COMPRESSION_TYPES: [CompressionType; 1] = [Zip];
        static ALL_TYPES: [CompressionType; 2] = [Zip, Directory];
        if sandboxed {
            COMPRESSION_TYPES.iter()
        } else {
            ALL_TYPES.iter()
        }
    }

    pub const fn as_mime(self) -> &'static str {
        match self {
            Zip => "application/zip",
            Directory => "inode/directory",
        }
    }

    pub const fn as_extension(self) -> &'static str {
        match self {
            Zip => "zip",
            Directory => "directory",
        }
    }

    pub const fn as_display_string(self) -> &'static str {
        match self {
            Directory => "Directory",
            Zip => "ZIP",
        }
    }

    pub fn from_string(extension: &str) -> Option<Self> {
        match extension {
            "zip" => Some(Zip),
            "directory" => Some(Directory),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum OutputType {
    File(FileType),
    Compression(CompressionType),
}

impl OutputType {
    pub const fn as_mime(self) -> &'static str {
        match self {
            Self::File(f) => f.as_mime(),
            Self::Compression(f) => f.as_mime(),
        }
    }

    pub fn from_string(extension: &str) -> Option<Self> {
        match extension {
            "zip" => Some(Self::Compression(Zip)),
            "directory" => Some(Self::Compression(Directory)),
            "png" => Some(Self::File(Png)),
            "jpg" | "jpeg" => Some(Self::File(Jpg)),
            "webp" => Some(Self::File(Webp)),
            "svg" => Some(Self::File(Svg)),
            "heif" => Some(Self::File(Heif)),
            "heic" => Some(Self::File(Heic)),
            "bmp" => Some(Self::File(Bmp)),
            "avif" => Some(Self::File(Avif)),
            "jxl" => Some(Self::File(Jxl)),
            "tiff" => Some(Self::File(Tiff)),
            "pdf" => Some(Self::File(Pdf)),
            "gif" => Some(Self::File(Gif)),
            "ico" => Some(Self::File(Ico)),
            "dds" => Some(Self::File(Dds)),
            _ => None,
        }
    }

    pub const fn as_extension(&self) -> &str {
        match self {
            Self::File(f) => f.as_extension(),
            Self::Compression(f) => f.as_extension(),
        }
    }
}
