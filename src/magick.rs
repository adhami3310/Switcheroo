use crate::{color::Color, filetypes::FileType, window::ResizeFilter, window::StripType};
use gettextrs::gettext;
use itertools::Itertools;
use log::error;
use shared_child::SharedChild;
use std::io::Read;
use std::num::NonZeroUsize;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};

pub async fn count_frames(path: String) -> Result<(NonZeroUsize, Option<(usize, usize)>), ()> {
    let output = tokio::process::Command::new("magick")
        .stdout(std::process::Stdio::piped())
        .arg("identify")
        .arg(path)
        .output()
        .await
        .map_err(|_| ())?;

    let output_string = std::str::from_utf8(&output.stdout).map_err(|_| ())?;

    let mut lines = output_string.lines();
    // No output at all means ImageMagick could not identify the file: it is
    // corrupted or relies on a delegate library that is missing (e.g. libjxl
    // for JXL, libwebp for WebP). Report it as an error rather than a valid
    // image with zero frames.
    let first_line = lines.next().ok_or(())?;
    // One line per frame; the first line also carries the dimensions.
    let count = NonZeroUsize::MIN.saturating_add(lines.count());
    let dimensions = regex::Regex::new(r" \d+x\d+ ")
        .unwrap()
        .find(first_line)
        .and_then(|regex_match| {
            let dimensions = regex_match
                .as_str()
                .trim()
                .split('x')
                .map(str::parse::<usize>)
                .collect_vec();
            match dimensions[..] {
                [Ok(width), Ok(height)] => Some((width, height)),
                _ => None,
            }
        });
    Ok((count, dimensions))
}

pub trait MagickArgument {
    fn get_argument(&self) -> Vec<String>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ResizeArgument {
    Percentage { width: usize, height: usize },
    ExactPixels { width: usize, height: usize },
}

impl Default for ResizeArgument {
    fn default() -> Self {
        Self::Percentage {
            width: 100,
            height: 100,
        }
    }
}

impl MagickArgument for ResizeFilter {
    fn get_argument(&self) -> Vec<String> {
        self.as_display_string()
            .map_or_else(std::vec::Vec::new, |f| {
                vec!["-filter".to_string(), f.to_owned()]
            })
    }
}

impl MagickArgument for StripType {
    fn get_argument(&self) -> Vec<String> {
        match self {
            Self::StripAll => vec!["-auto-orient".to_string(), "-strip".to_string()],
            Self::None => vec![],
        }
    }
}

impl MagickArgument for ResizeArgument {
    fn get_argument(&self) -> Vec<String> {
        match self {
            Self::Percentage { width, height } => {
                vec!["-resize".to_owned(), format!("{width}%x{height}%")]
            }
            Self::ExactPixels { width, height } => {
                vec!["-resize".to_owned(), format!("{width}x{height}!")]
            }
        }
    }
}

impl<T> MagickArgument for Option<T>
where
    T: MagickArgument,
{
    fn get_argument(&self) -> Vec<String> {
        self.as_ref()
            .map_or_else(std::vec::Vec::new, MagickArgument::get_argument)
    }
}

#[derive(Debug, Clone)]
pub struct MagickConvertJob {
    pub input_file: String,
    pub output_file: String,
    pub background: Color,
    pub quality: usize,
    pub first_frame: bool,
    pub filter: Option<ResizeFilter>,
    pub strip: StripType,
    pub resize_arg: ResizeArgument,
    pub density: Option<usize>,
    pub remove_alpha: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct JobFile {
    pub id: usize,
    pub desired_name: Option<String>,
    pub file_extension: FileType,
}

static FILE_COUNT: AtomicUsize = AtomicUsize::new(0);

impl JobFile {
    pub fn new(file_extension: FileType, desired_name: Option<String>) -> Self {
        let id = FILE_COUNT.fetch_add(1, Ordering::SeqCst) + 1;
        Self {
            id,
            desired_name,
            file_extension,
        }
    }

    pub fn from_clipboard() -> Self {
        let id = FILE_COUNT.fetch_add(1, Ordering::SeqCst) + 1;
        Self {
            id,
            desired_name: Some(format!("{}.png", gettext("Pasted Image"))),
            file_extension: FileType::Png,
        }
    }

    pub fn as_filename(&self) -> String {
        self.desired_name.as_ref().map_or_else(
            || {
                format!(
                    "TEMPORARY_SWITCHEROO_{}.{}",
                    self.id,
                    self.file_extension.as_extension()
                )
            },
            std::borrow::ToOwned::to_owned,
        )
    }
}

impl MagickConvertJob {
    pub fn get_command(&self) -> Command {
        let mut command = Command::new("magick");

        dbg!(self);

        let input_file_ext = self
            .input_file
            .rsplit('.')
            .next()
            .unwrap_or("")
            .split('[')
            .next()
            .unwrap_or("")
            .to_lowercase();

        let (resize_arg, size_arg) = match input_file_ext.as_str() {
            "svg" => (
                vec![],
                match self.resize_arg {
                    ResizeArgument::ExactPixels { width, height } => {
                        vec!["-size".to_owned(), format!("{width}x{height}")]
                    }
                    ResizeArgument::Percentage { width, height: _ } => {
                        let all_pixels = width as f64 / 100.0;
                        vec![
                            "-density".to_owned(),
                            ((all_pixels * 96.0) as usize).to_string(),
                        ]
                    }
                },
            ),
            "pdf" => (
                self.resize_arg.get_argument(),
                self.density.map_or_else(std::vec::Vec::new, |density| {
                    vec!["-density".to_owned(), density.to_string()]
                }),
            ),
            _ => (self.resize_arg.get_argument(), vec![]),
        };

        dbg!(&resize_arg);
        dbg!(&size_arg);

        if self.first_frame {
            command
                .args(size_arg)
                .args(["-background", &self.background.as_hex_string()])
                .arg(self.input_file.clone())
                .arg("-flatten");

            if self.remove_alpha {
                command.arg("-alpha").arg("off");
            }

            command
                .args(["-quality".to_string(), self.quality.to_string()])
                .args(self.filter.get_argument())
                .args(resize_arg)
                .args(self.strip.get_argument())
                .arg(self.output_file.clone());
        } else {
            command
                .arg(self.input_file.clone())
                .arg("-coalesce")
                .args(vec![
                    "-fill",
                    &self.background.as_hex_string(),
                    "-opaque",
                    "none",
                ])
                .args(vec!["-quality".to_string(), self.quality.to_string()])
                .args(self.filter.get_argument())
                .args(resize_arg)
                .arg(self.output_file.clone());
        }

        command.stdout(Stdio::piped()).stderr(Stdio::piped());

        command
    }
}

pub fn generate_job(
    input_path: &str,
    input_type: FileType,
    output_path: &str,
    output_type: FileType,
    pdf_dpi: usize,
    default_arguments: &MagickConvertJob,
) -> Vec<MagickConvertJob> {
    use FileType::Pdf;
    match (input_type, output_type) {
        (Pdf, _) => std::iter::once(MagickConvertJob {
            input_file: input_path.to_owned(),
            output_file: output_path.to_owned(),
            density: Some(pdf_dpi),
            ..*default_arguments
        })
        .collect(),
        (input, output) if input.supports_animation() && output.supports_animation() => {
            std::iter::once(MagickConvertJob {
                input_file: input_path.to_owned(),
                output_file: output_path.to_owned(),
                first_frame: false,
                ..*default_arguments
            })
            .collect()
        }
        (input, output) => std::iter::once(MagickConvertJob {
            input_file: input_path.to_owned(),
            output_file: output_path.to_owned(),
            first_frame: true,
            remove_alpha: !input.supports_alpha() && output.supports_alpha(),
            ..*default_arguments
        })
        .collect(),
    }
}

pub fn wait_for_child(child: &std::sync::Arc<SharedChild>) -> Result<(), String> {
    let command = child.wait();
    match command {
        Ok(output) => {
            if output.success() {
                Ok(())
            } else {
                let mut stderr = String::new();
                child
                    .take_stdout()
                    .map(|mut s| s.read_to_string(&mut stderr).ok());
                child
                    .take_stderr()
                    .map(|mut s| s.read_to_string(&mut stderr).ok());
                Err(stderr)
            }
        }
        Err(err) => {
            error!("IO error happened: {err}");
            Err(gettext("Unknown IO error happened"))
        }
    }
}
