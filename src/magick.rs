use crate::temp::get_temporary_file_path;
use crate::{color::Color, filetypes::FileType, window::ResizeFilter, window::StripType};
use gettextrs::gettext;
use itertools::Itertools;
use shared_child::SharedChild;
use std::ffi::OsStr;
use std::io::Read;
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use tempfile::TempDir;

pub async fn count_frames(path: &Path) -> Result<(NonZeroUsize, Option<(usize, usize)>), ()> {
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

#[derive(Debug)]
pub struct IndividualConvertJob {
    pub input_file: PathBuf,
    pub input_file_type: FileType,
    pub output_stem: String,
}

impl IndividualConvertJob {
    pub fn get_magick_job(
        self,
        temporary_directory: &TempDir,
        output_file_type: FileType,
        pdf_dpi: usize,
        magick_arguments: &DefaultMagickArguments,
    ) -> MagickConvertJob {
        generate_job(
            self.input_file,
            self.input_file_type,
            get_temporary_file_path(
                temporary_directory,
                &JobFile::new(output_file_type, Some(self.output_stem)),
            ),
            output_file_type,
            pdf_dpi,
            magick_arguments,
        )
    }
}

pub struct DefaultMagickArguments {
    pub background: Color,
    pub quality: usize,
    pub filter: Option<ResizeFilter>,
    pub resize_arg: ResizeArgument,
    pub strip: StripType,
}

#[derive(Debug, Clone)]
pub struct MagickConvertJob {
    pub input_file: PathBuf,
    pub output_file: PathBuf,
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
    pub const fn from_default_arguments(
        input_file: PathBuf,
        output_file: PathBuf,
        density: Option<usize>,
        first_frame: bool,
        remove_alpha: bool,
        default_arguments: &DefaultMagickArguments,
    ) -> Self {
        Self {
            input_file,
            output_file,
            background: default_arguments.background,
            quality: default_arguments.quality,
            first_frame,
            filter: default_arguments.filter,
            strip: default_arguments.strip,
            resize_arg: default_arguments.resize_arg,
            density,
            remove_alpha,
        }
    }

    pub fn get_command(&self) -> Command {
        let mut command = Command::new("magick");

        dbg!(self);

        let input_file_ext = self
            .input_file
            .extension()
            .and_then(OsStr::to_str)
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

pub const fn generate_job(
    input_path: PathBuf,
    input_type: FileType,
    output_path: PathBuf,
    output_type: FileType,
    pdf_dpi: usize,
    default_arguments: &DefaultMagickArguments,
) -> MagickConvertJob {
    match (input_type, output_type) {
        (FileType::Pdf, _) => MagickConvertJob::from_default_arguments(
            input_path,
            output_path,
            Some(pdf_dpi),
            false,
            false,
            default_arguments,
        ),
        (input, output) if input.supports_animation() && output.supports_animation() => {
            MagickConvertJob::from_default_arguments(
                input_path,
                output_path,
                None,
                false,
                false,
                default_arguments,
            )
        }
        (input, output) => MagickConvertJob::from_default_arguments(
            input_path,
            output_path,
            None,
            true,
            !input.supports_alpha() && output.supports_alpha(),
            default_arguments,
        ),
    }
}

pub fn wait_for_child(child: &std::sync::Arc<SharedChild>) -> Result<(), std::io::Error> {
    let exit_status = child.wait()?;
    if exit_status.success() {
        Ok(())
    } else {
        let mut stderr = String::new();
        child
            .take_stdout()
            .map(|mut s| s.read_to_string(&mut stderr).ok());
        child
            .take_stderr()
            .map(|mut s| s.read_to_string(&mut stderr).ok());
        Err(std::io::Error::other(stderr))
    }
}
