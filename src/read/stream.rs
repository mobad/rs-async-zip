// Copyright (c) 2021 Harry [Majored] [hello@majored.pw]
// MIT License (https://github.com/Majored/rs-async-zip/blob/main/LICENSE)

//! A module for reading ZIP file from a non-seekable source.
//!
//! # Example
//! ```
//! ```

use crate::read::ZipFileReader;

use crate::error::{Result, ZipError};
use crate::read::entry::{OwnedReader, PrependReader, CompressionReader};
use crate::read::{ZipEntry, ZipEntryReader};
use crate::spec::compression::Compression;
use crate::spec::header::LocalFileHeader;

use tokio::io::{AsyncRead, AsyncReadExt};
use async_io_utilities::{AsyncPrependReader, AsyncDelimiterReader};

/// A method which allows ZIP entries to be read both: out-of-order and multiple times.
/// 
/// As a result, this method requries the source to implement both [`AsyncRead`] and [`AsyncSeek`].
pub struct StreamMethod<R: AsyncRead + Unpin> {
    pub(crate) reader: AsyncPrependReader<R>,
    pub(crate) entry: Option<ZipEntry>,
    pub(crate) finished: bool,
}

impl<R: AsyncRead + Unpin> ZipFileReader<StreamMethod<R>> {
    /// Constructs a new ZIP file reader from a mutable reference to a reader.
    pub fn new(reader: R) -> Self {
        let reader = AsyncPrependReader::new(reader);
        ZipFileReader { inner: StreamMethod { reader, entry: None, finished: false } }
    }

    /// Returns whether or not `entry_reader()` will yield more entries.
    pub fn finished(&self) -> bool {
        self.inner.finished
    }

    /// Opens the next entry for reading if the central directory hasn't already been reached.
    pub async fn entry_reader(&mut self) -> Result<Option<ZipEntryReader<'_, R>>> {
        // TODO: Ensure the previous entry has been fully read.

        if self.inner.finished {
            return Ok(None);
        } else if let Some(inner) = read_lfh(&mut self.inner.reader).await? {
            self.inner.entry = Some(inner);
        } else {
            self.inner.finished = true;
            return Ok(None);
        }

        let entry_borrow = self.inner.entry.as_ref().unwrap();

        if entry_borrow.data_descriptor() {
            let delimiter = crate::spec::signature::DATA_DESCRIPTOR.to_le_bytes();
            let reader = OwnedReader::Borrow(&mut self.inner.reader);
            let reader = PrependReader::Prepend(reader);
            let reader = AsyncDelimiterReader::new(reader, &delimiter);
            let reader = CompressionReader::from_reader(entry_borrow.compression(), reader.take(u64::MAX));

            Ok(Some(ZipEntryReader::with_data_descriptor(entry_borrow, reader, true)))
        } else {
            let reader = OwnedReader::Borrow(&mut self.inner.reader);
            let reader = PrependReader::Prepend(reader);
            let reader = reader.take(entry_borrow.compressed_size.unwrap().into());
            let reader = CompressionReader::from_reader(entry_borrow.compression(), reader);

            Ok(Some(ZipEntryReader::from_raw(entry_borrow, reader, true)))
        }
    }
}

pub(crate) async fn read_lfh<R: AsyncRead + Unpin>(reader: &mut R) -> Result<Option<ZipEntry>> {
    match reader.read_u32_le().await? {
        crate::spec::signature::LOCAL_FILE_HEADER => {}
        crate::spec::signature::CENTRAL_DIRECTORY_FILE_HEADER => return Ok(None),
        actual => return Err(ZipError::UnexpectedHeaderError(actual, crate::spec::signature::LOCAL_FILE_HEADER)),
    };

    let header = LocalFileHeader::from_reader(reader).await?;
    let filename = crate::utils::read_string(reader, header.file_name_length.into()).await?;
    let extra = crate::utils::read_bytes(reader, header.extra_field_length.into()).await?;

    let entry = ZipEntry {
        name: filename,
        comment: None,
        data_descriptor: header.flags.data_descriptor,
        crc32: Some(header.crc),
        uncompressed_size: Some(header.uncompressed_size),
        compressed_size: Some(header.compressed_size),
        last_modified: crate::spec::date::zip_date_to_chrono(header.mod_date, header.mod_time),
        extra: Some(extra),
        compression: Compression::from_u16(header.compression)?,
        offset: None,
    };

    Ok(Some(entry))
}
