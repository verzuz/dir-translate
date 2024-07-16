# Setup

- install libpdfium.so on your system:

https://pdfium.googlesource.com/pdfium/


- install tesserac orc

https://github.com/tesseract-ocr/tesseract

- install libretranslate

https://github.com/LibreTranslate/LibreTranslate

- install ru_en to libretranslate

# Usage

## Prerequisite

- run libretranslate locally with ru_en

## Translating filenames

> cargo run -- --source-dir my/source/dir/ filenames

## Translating content

> cargo run -- --source-dir my/source/dir/ translate destination/dir/
