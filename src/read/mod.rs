// Copyright (c) 2021 Harry [Majored] [hello@majored.pw]
// MIT License (https://github.com/Majored/rs-async-zip/blob/main/LICENSE)

//! A module which supports reading ZIP files using various approaches.

pub mod fs;
pub mod mem;
pub(crate) mod seek;
pub(crate) mod stream;
pub mod sync;
pub(crate) mod entry;

pub use entry::{ZipEntry, ZipEntryReader};
pub use seek::SeekMethod;
pub use stream::StreamMethod;
pub use fs::FileMethod;
use crate::error::Result;

use tokio::io::{AsyncRead, AsyncSeek};

/// A ZIP archive reader generic over a reading method implementation.
pub struct ZipFileReader<M> {
    pub(crate) inner: M,
}

impl<M> ZipFileReader<M> {
    /// Constructs a new ZIP archive reader using the seeking method.
    /// 
    /// An alias of [`ZipFileReader::<SeekMethod::<R>>::new()`].
    pub async fn with_seek_method<R: AsyncRead + AsyncSeek + Unpin>(reader: R) -> Result<ZipFileReader<SeekMethod<R>>> {
        ZipFileReader::<SeekMethod::<_>>::new(reader).await
    }

    /// Constructs a new ZIP archive reader using the file method.
    /// 
    /// An alias of [`ZipFileReader::<FileMethod>::new()`].
    pub async fn with_file_method(filename: String) -> Result<ZipFileReader<FileMethod>> {
        ZipFileReader::<FileMethod>::new(filename).await
    }

    /// Constructs a new ZIP archive reader using the streaming method.
    /// 
    /// An alias of [`ZipFileReader::<StreamMethod::<R>>::new()`].
    pub fn with_stream_method<R: AsyncRead + Unpin>(reader: R) -> ZipFileReader<StreamMethod<R>> {
        ZipFileReader::<StreamMethod::<_>>::new(reader)
    }
}