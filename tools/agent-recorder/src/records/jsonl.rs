use std::fs::{File, OpenOptions};
use std::io::ErrorKind;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::Path;

use anyhow::{Context, Result};

use crate::{
    records::{IndexedGraphRecord, RecordAdapter, RecordReader, RecordSelector},
    GraphRecord,
};

pub struct JsonlRecordAdapter {
    writer: BufWriter<File>,
}

pub struct JsonlRecordReader {
    path: std::path::PathBuf,
}

impl JsonlRecordAdapter {
    pub fn create(path: &Path) -> Result<Self> {
        let file = OpenOptions::new().create(true).append(true).open(path)?;
        Ok(Self {
            writer: BufWriter::new(file),
        })
    }
}

impl JsonlRecordReader {
    pub fn create(path: &Path) -> Result<Self> {
        Ok(Self {
            path: path.to_path_buf(),
        })
    }
}

impl RecordAdapter for JsonlRecordAdapter {
    fn name(&self) -> &'static str {
        "file"
    }

    fn log(&mut self, record: &GraphRecord) -> Result<()> {
        serde_json::to_writer(&mut self.writer, record)?;
        self.writer.write_all(b"\n")?;
        self.writer.flush()?;
        Ok(())
    }
}

impl RecordReader for JsonlRecordReader {
    fn name(&self) -> &'static str {
        "file"
    }

    fn read(
        &self,
        selector: RecordSelector,
        emit: &mut dyn FnMut(IndexedGraphRecord) -> Result<()>,
    ) -> Result<()> {
        let file = match File::open(&self.path) {
            Ok(file) => file,
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(()),
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("opening JSONL records at {}", self.path.display()))
            }
        };
        let reader = BufReader::new(file);

        for (line_index, line) in reader.lines().enumerate() {
            let index = line_index as u64;
            if !selector.contains(index) {
                if selector.is_before(index) {
                    continue;
                }
                if selector.is_after(index) {
                    break;
                }
            }

            let line = line.with_context(|| {
                format!(
                    "reading line {} from {}",
                    line_index + 1,
                    self.path.display()
                )
            })?;
            if line.trim().is_empty() {
                continue;
            }
            let record = serde_json::from_str::<GraphRecord>(&line).with_context(|| {
                format!(
                    "parsing GraphRecord at {}:{}",
                    self.path.display(),
                    line_index + 1
                )
            })?;
            emit(IndexedGraphRecord { index, record })?;
        }

        Ok(())
    }
}
