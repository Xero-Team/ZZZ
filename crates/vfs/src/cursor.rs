use std::{
    future::Future,
    io::{Error as IoError, ErrorKind, SeekFrom},
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};

use futures::io::{AsyncRead, AsyncSeek};

use crate::{OperationContext, VfsFile, VfsResult};

type ReadFuture = Pin<Box<dyn Future<Output = (u64, Vec<u8>, VfsResult<usize>)> + Send>>;
type LengthFuture = Pin<Box<dyn Future<Output = VfsResult<u64>> + Send>>;

/// Sequential `AsyncRead`/`AsyncSeek` adapter over positioned `VfsFile` I/O.
///
/// Each cursor owns only its logical position, so multiple cursors over one
/// file never share or race a backend cursor.
pub struct VfsFileCursor {
    file: Arc<dyn VfsFile>,
    position: u64,
    operation_context: OperationContext,
    pending_read: Option<ReadFuture>,
    pending_length: Option<LengthFuture>,
}

impl VfsFileCursor {
    /// Creates a cursor at offset zero using the supplied cancellation context.
    pub fn new(file: Arc<dyn VfsFile>, operation_context: OperationContext) -> Self {
        Self {
            file,
            position: 0,
            operation_context,
            pending_read: None,
            pending_length: None,
        }
    }

    /// Returns the cursor's next read offset.
    pub fn position(&self) -> u64 {
        self.position
    }

    fn seek_from_base(&mut self, base: u64, offset: i64) -> std::io::Result<u64> {
        let position = i128::from(base) + i128::from(offset);
        let position = u64::try_from(position).map_err(|_| {
            IoError::new(
                ErrorKind::InvalidInput,
                "VFS cursor seek would move outside the file address space",
            )
        })?;
        self.position = position;
        Ok(position)
    }
}

impl AsyncRead for VfsFileCursor {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buffer: &mut [u8],
    ) -> Poll<std::io::Result<usize>> {
        if self.pending_length.is_some() {
            return Poll::Ready(Err(IoError::other(
                "cannot read while a VFS cursor seek is pending",
            )));
        }
        if buffer.is_empty() {
            return Poll::Ready(Ok(0));
        }
        if self.pending_read.is_none() {
            let file = self.file.clone();
            let position = self.position;
            let operation_context = self.operation_context.clone();
            let length = buffer.len();
            self.pending_read = Some(Box::pin(async move {
                let mut bytes = vec![0; length];
                let result = file.read_at(position, &mut bytes, operation_context).await;
                (position, bytes, result)
            }));
        }

        let Some(future) = self.pending_read.as_mut() else {
            return Poll::Ready(Err(IoError::other("VFS cursor read future disappeared")));
        };
        match future.as_mut().poll(cx) {
            Poll::Pending => Poll::Pending,
            Poll::Ready((position, bytes, result)) => {
                self.pending_read = None;
                let bytes_read = result.map_err(IoError::other)?;
                if bytes_read > bytes.len() || bytes_read > buffer.len() {
                    return Poll::Ready(Err(IoError::new(
                        ErrorKind::InvalidData,
                        "VFS file returned more bytes than the cursor requested",
                    )));
                }
                buffer[..bytes_read].copy_from_slice(&bytes[..bytes_read]);
                let bytes_read_u64 = u64::try_from(bytes_read).map_err(|_| {
                    IoError::new(ErrorKind::InvalidData, "VFS cursor byte count overflowed")
                })?;
                self.position = position.checked_add(bytes_read_u64).ok_or_else(|| {
                    IoError::new(ErrorKind::InvalidData, "VFS cursor position overflowed")
                })?;
                Poll::Ready(Ok(bytes_read))
            }
        }
    }
}

impl AsyncSeek for VfsFileCursor {
    fn poll_seek(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        position: SeekFrom,
    ) -> Poll<std::io::Result<u64>> {
        if self.pending_read.is_some() {
            return Poll::Ready(Err(IoError::other(
                "cannot seek while a VFS cursor read is pending",
            )));
        }
        match position {
            SeekFrom::Start(position) => {
                self.pending_length = None;
                self.position = position;
                Poll::Ready(Ok(position))
            }
            SeekFrom::Current(offset) => {
                self.pending_length = None;
                let current = self.position;
                Poll::Ready(self.seek_from_base(current, offset))
            }
            SeekFrom::End(offset) => {
                if self.pending_length.is_none() {
                    let file = self.file.clone();
                    let operation_context = self.operation_context.clone();
                    self.pending_length =
                        Some(Box::pin(async move { file.len(operation_context).await }));
                }
                let Some(future) = self.pending_length.as_mut() else {
                    return Poll::Ready(Err(IoError::other(
                        "VFS cursor length future disappeared",
                    )));
                };
                match future.as_mut().poll(cx) {
                    Poll::Pending => Poll::Pending,
                    Poll::Ready(result) => {
                        self.pending_length = None;
                        let length = result.map_err(IoError::other)?;
                        Poll::Ready(self.seek_from_base(length, offset))
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use futures::io::{AsyncReadExt as _, AsyncSeekExt as _};

    use crate::{
        CreateDisposition, FileAccess, MemoryProvider, OpenOptions, PathEncoding, ProviderPath,
        VfsProvider as _, WriteAtOptions,
    };

    use super::*;

    #[test]
    fn positioned_file_cursor_reads_and_seeks_without_shared_cursor_state() {
        futures::executor::block_on(async {
            let provider = MemoryProvider::new("cursor-test", PathEncoding::PortableUtf8);
            let path = ProviderPath::from_byte_components(
                PathEncoding::PortableUtf8,
                [b"archive.zip".as_slice()],
            )
            .expect("cursor fixture path should be valid");
            let file = provider
                .open(
                    &path,
                    OpenOptions {
                        access: FileAccess::ReadWrite,
                        create: CreateDisposition::CreateNew,
                        ..OpenOptions::default()
                    },
                )
                .await
                .expect("cursor fixture should open");
            file.write_at(0, b"0123456789", WriteAtOptions::default())
                .await
                .expect("cursor fixture should write");

            let mut first = VfsFileCursor::new(file.clone(), OperationContext::default());
            let mut second = VfsFileCursor::new(file, OperationContext::default());
            let mut bytes = [0; 4];
            first
                .read_exact(&mut bytes)
                .await
                .expect("first cursor should read");
            assert_eq!(&bytes, b"0123");
            first
                .seek(SeekFrom::End(-3))
                .await
                .expect("first cursor should seek from end");
            let mut tail = Vec::new();
            first
                .read_to_end(&mut tail)
                .await
                .expect("first cursor should read tail");
            assert_eq!(tail, b"789");

            second
                .seek(SeekFrom::Start(4))
                .await
                .expect("second cursor should seek independently");
            let mut middle = [0; 2];
            second
                .read_exact(&mut middle)
                .await
                .expect("second cursor should read independently");
            assert_eq!(&middle, b"45");
        });
    }
}
