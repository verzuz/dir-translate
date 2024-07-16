use anyhow::{anyhow, Result};
use clap::*;
use docx_rust::DocxFile;
use image::*;
use libretranslate::{translate_url, Language};
use pdfium_render::prelude::*;
use serde::*;
use std::io::Write;
use std::{fs::File, io::Cursor, path::Path};
use walkdir::*;

const TARGET_LANG: Language = Language::English;
const SOURCE_LANG: Language = Language::Russian;

#[derive(Deserialize)]
struct Config {
    tesserac_data: String,
    libretranslate_url: String,
}

#[derive(Parser)]
#[command(version, about, long_about = None)]
#[command(propagate_version = true)]
struct Args {
    #[command(subcommand)]
    command: Commands,
    /// directory to translate
    #[arg(short, long)]
    source_dir: String,
}

#[derive(Subcommand)]
enum Commands {
    /// translate filenames only
    Filenames,
    /// translate source folder into target folder
    Translate { target_dir: String },
}

#[tokio::main]
async fn main() {
    let config: Config = toml::from_str(&std::fs::read_to_string("config.toml").unwrap()).unwrap();
    let args = Args::parse();
    let mut translator = Translator::new(config);
    match args.command {
        Commands::Filenames => {
            for entry in WalkDir::new(args.source_dir) {
                let entry = entry.unwrap();
                if entry.metadata().unwrap().is_file() {
                    println!(
                        "{}",
                        translator
                            .translate(entry.path().to_str().unwrap())
                            .await
                            .unwrap()
                    );
                }
            }
        }
        Commands::Translate { target_dir } => {
            for entry in WalkDir::new(args.source_dir) {
                let entry = entry.unwrap();
                if entry.metadata().unwrap().is_file() {
                    let path = entry.into_path();
                    if let Some(ext) = path.extension() {
                        let ext = ext
                            .to_str()
                            .expect("could not create string from extension")
                            .to_lowercase();
                        match ext.as_str() {
                            "pdf" => {
                                let path_out = Path::new(&target_dir);
                                translator.translate_pdf(&path, &path_out).await.unwrap()
                            }
                            "png" | "jpg" => {
                                let path_out = Path::new(&target_dir);
                                translator.translate_img(&path, &path_out).await.unwrap()
                            }
                            "docx" => {
                                let path_out = Path::new(&target_dir);
                                translator.translate_docx(&path, &path_out).await.unwrap()
                            }
                            _ => (),
                        }
                    }
                }
            }
        }
    }
}

struct Translator {
    lt: leptess::LepTess,
    pdfium: Pdfium,
    config: Config,
}

impl Translator {
    pub fn new(config: Config) -> Self {
        Translator {
            lt: leptess::LepTess::new(Some(&config.tesserac_data), "rus").unwrap(),
            pdfium: Pdfium::new(
                Pdfium::bind_to_library(Pdfium::pdfium_platform_library_name_at_path("./"))
                    .or_else(|_| Pdfium::bind_to_system_library())
                    .unwrap(),
            ),
            config,
        }
    }

    pub async fn translate(&mut self, text: &str) -> Result<String> {
        let data = translate_url(
            SOURCE_LANG,
            TARGET_LANG,
            text,
            &self.config.libretranslate_url,
            None,
        )
        .await?;
        Ok(data.output.to_owned())
    }

    pub async fn translate_docx(&mut self, file: &Path, out: &Path) -> Result<()> {
        let docx_file = DocxFile::from_file(
            file.to_str()
                .ok_or_else(|| anyhow!("could not get file string"))?,
        )
        .map_err(|f| anyhow!("{:?}", f))?;
        let docx = docx_file.parse().map_err(|f| anyhow!("{:?}", f))?;

        let mut new_txt_file = file.file_name().unwrap().to_string_lossy().to_string();
        new_txt_file.push_str(".txt");
        let file_path = Path::new(&new_txt_file);
        let mut out_path = out.to_path_buf();
        out_path.push(file_path);
        let mut output = File::create(out_path).unwrap();
        let text = docx.document.body.text();
        let parts = text.split(".");
        for p in parts {
            if let Ok(data) = translate_url(
                SOURCE_LANG,
                TARGET_LANG,
                p,
                &self.config.libretranslate_url,
                None,
            )
            .await
            {
                write!(output, "{}.\n", data.output).unwrap();
            }
        }
        Ok(())
    }

    pub async fn translate_img(&mut self, file: &Path, out: &Path) -> Result<()> {
        println!("{:?}", self.lt.set_image(&file));
        let boxes = self
            .lt
            .get_component_boxes(leptess::capi::TessPageIteratorLevel_RIL_BLOCK, true);
        for b in &boxes {
            for x in b.into_iter() {
                self.lt.set_rectangle_from_box(&x);
                let input = self.lt.get_utf8_text().unwrap();

                if let Ok(data) = translate_url(
                    SOURCE_LANG,
                    TARGET_LANG,
                    &input,
                    &self.config.libretranslate_url,
                    None,
                )
                .await
                {
                    let mut new_txt_file = file.file_name().unwrap().to_string_lossy().to_string();
                    new_txt_file.push_str(".txt");
                    let file_path = Path::new(&new_txt_file);
                    let mut out_path = out.to_path_buf();
                    out_path.push(file_path);
                    let mut output = File::create(out_path).unwrap();
                    write!(output, "{}", data.output).unwrap();
                }
            }
        }
        Ok(())
    }

    pub async fn translate_pdf(&mut self, file: &Path, out: &Path) -> Result<()> {
        if let Ok(document) = self.pdfium.load_pdf_from_file(file, None) {
            let render_config = PdfRenderConfig::new()
                .set_target_width(2000)
                .set_maximum_height(2000)
                .rotate_if_landscape(PdfPageRenderRotation::Degrees90, true);
            for (index, page) in document.pages().iter().enumerate() {
                let rendered = page.render_with_config(&render_config).unwrap();
                let image = rendered.as_image();
                let mut bytes: Vec<u8> = Vec::new();
                image
                    .write_to(&mut Cursor::new(&mut bytes), ImageFormat::Png)
                    .unwrap();
                println!("{:?}", self.lt.set_image_from_mem(&bytes));
                let new_txt_file = file
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .to_string()
                    .to_lowercase()
                    .replace(".pdf", &format!("-page-{}.txt", index));
                let file_path = Path::new(&new_txt_file);
                let mut out_path = out.to_path_buf();
                out_path.push(file_path);
                let mut output = File::create(out_path).unwrap();
                let boxes = self
                    .lt
                    .get_component_boxes(leptess::capi::TessPageIteratorLevel_RIL_BLOCK, true);
                for b in &boxes {
                    for x in b.into_iter() {
                        self.lt.set_rectangle_from_box(&x);
                        let input = self.lt.get_utf8_text().unwrap();

                        if let Ok(data) = translate_url(
                            SOURCE_LANG,
                            TARGET_LANG,
                            &input,
                            &self.config.libretranslate_url,
                            None,
                        )
                        .await
                        {
                            write!(output, "{}", data.output).unwrap();
                        }
                    }
                }
                let rgba8 = image.as_rgba8().unwrap();
                let new_file = file
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .to_string()
                    .to_lowercase()
                    .replace(".pdf", &format!("-page-{}.jpg", index));
                let file_path = Path::new(&new_file);
                let mut out_path = out.to_path_buf();
                out_path.push(file_path);
                rgba8
                    .save_with_format(out_path.to_str().unwrap(), ImageFormat::Jpeg)
                    .unwrap();
            }
        }
        Ok(())
    }
}
