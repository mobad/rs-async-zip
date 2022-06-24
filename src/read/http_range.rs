// Copyright (c) 2021 Harry [Majored] [hello@majored.pw]
// MIT License (https://github.com/Majored/rs-async-zip/blob/main/LICENSE)

//! A module for reading ZIP file entries concurrently from the filesystem.
//!
//! # Example
//! ```no_run
//! # use async_zip::read::fs::ZipHttpRangeReader;
//! # use async_zip::error::ZipError;
//! #
//! # async fn run() -> Result<(), ZipError> {
//! let zip = ZipHttpRangeReader::new(String::from("./Archive.zip")).await.unwrap();
//! assert_eq!(zip.entries().len(), 2);
//!
//! let mut reader1 = zip.entry_reader(0).await.unwrap();
//! let mut reader2 = zip.entry_reader(1).await.unwrap();
//!
//! tokio::select! {
//!    _ = reader1.read_to_string_crc() => {}
//!    _ = reader2.read_to_string_crc() => {}
//! };
//! #   Ok(())
//! # }
//! ```

use super::CompressionReader;
use crate::error::{Result, ZipError};
use crate::read::{OwnedReader, PrependReader, ZipEntry, ZipEntryReader};
use crate::spec::header::LocalFileHeader;

use async_io_utilities::AsyncDelimiterReader;
use futures::stream::TryStreamExt;
use http_content_range::ContentRange;
use reqwest::header::{CONTENT_RANGE, RANGE};
use reqwest::Client;
use std::io::Cursor;
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio_util::compat::FuturesAsyncReadCompatExt;

/// A reader which acts concurrently over a filesystem file.sink
pub struct ZipHttpRangeReader {
    pub(crate) client: Client,
    pub(crate) url: String,
    pub(crate) entries: Vec<ZipEntry>,
    pub(crate) comment: Option<String>,
}

impl ZipHttpRangeReader {
    /// Constructs a new ZIP file reader from a filename.
    pub async fn new(url: String) -> Result<ZipHttpRangeReader> {
        const MAX_ENDING_LENGTH: u64 = (u16::MAX - 2) as u64;

        let client = Client::new();
        let response = client.get(&url).header(RANGE, format!("bytes=-{}", MAX_ENDING_LENGTH)).send().await.unwrap();
        let content_range =
            response.headers().get(CONTENT_RANGE).map(|range| ContentRange::parse_bytes(range.as_bytes()));
        let offset = if let Some(ContentRange::Bytes(r)) = content_range {
            r.first_byte
        } else {
            return Err(ZipError::EntryIndexOutOfBounds);
        };

        // TODO: Check for accept-range header and fail if not supported
        let cd_cache = response.bytes().await.map_err(|e| futures::io::Error::new(futures::io::ErrorKind::Other, e))?;

        let (entries, comment) = crate::read::seek::read_cd_at_offset(&mut Cursor::new(cd_cache), offset).await?;

        Ok(ZipHttpRangeReader { client, url, entries, comment })
    }

    crate::read::reader_entry_impl!();

    /// Opens an entry at the provided index for reading.
    pub async fn entry_reader(&self, index: usize) -> Result<ZipEntryReader<'_, impl AsyncRead + Unpin>> {
        let entry = self.entries.get(index).ok_or(ZipError::EntryIndexOutOfBounds)?;
        let start = entry.offset.unwrap() as u64 + 4;
        let extra = 26 + entry.name().len() + entry.extra().map_or(0, |e| e.len());
        let end = start + entry.compressed_size().unwrap() as u64 + extra as u64 - 1;

        let response = self
            .client
            .get(&self.url)
            .header(RANGE, format!("bytes={}-{}", start, end))
            .send()
            .await
            .map_err(|e| futures::io::Error::new(futures::io::ErrorKind::Other, e))?;

        let mut reader = response
            .bytes_stream()
            .map_err(|e| futures::io::Error::new(futures::io::ErrorKind::Other, e))
            .into_async_read()
            .compat();

        let header = LocalFileHeader::from_reader(&mut reader).await?;
        let data_offset = (header.file_name_length + header.extra_field_length) as u64;
        // If the data length in the CD don't equal the local header then we're going to trucate
        // the end of the file but it's too late at this point
        let mut skip: Vec<u8> = vec![0; data_offset as usize];
        reader.read_exact(&mut skip).await?;

        if entry.data_descriptor() {
            let delimiter = crate::spec::signature::DATA_DESCRIPTOR.to_le_bytes();
            let reader = OwnedReader::Owned(reader);
            let reader = PrependReader::Normal(reader);
            let reader = AsyncDelimiterReader::new(reader, &delimiter);
            let reader = CompressionReader::from_reader(entry.compression(), reader.take(u64::MAX));

            Ok(ZipEntryReader::with_data_descriptor(entry, reader, true))
        } else {
            let reader = OwnedReader::Owned(reader);
            let reader = PrependReader::Normal(reader);
            let reader = reader.take(entry.compressed_size.unwrap().into());
            let reader = CompressionReader::from_reader(entry.compression(), reader);

            Ok(ZipEntryReader::from_raw(entry, reader, false))
        }
    }
}
