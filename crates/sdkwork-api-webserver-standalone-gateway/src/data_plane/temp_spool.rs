//! Bounded temp-spool subsystem for CGI-family upstream responses (Wave 14A).
//!
//! nginx switches an upstream response that no longer fits in its response
//! buffers to a temp file under the configured `*_temp_path`
//! (`ngx_event_pipe` → `ngx_create_temp_file`), bounded per file by
//! `*_max_temp_file_size` and written in `*_temp_file_write_size` chunks. This
//! module owns the runtime equivalent:
//!
//! - a **per-generation spool directory** under the operator-configured
//!   `[http] tempRoot` (or the system temp directory when unset), created
//!   lazily; every spool file is created `0600` with a unique name and removed
//!   on completion, error or drop;
//! - a **global budget** (total in-flight bytes and open files), so a burst of
//!   oversized upstreams can never fill the disk unboundedly. Exceeding the
//!   budget degrades that one response to the in-memory tier, which is still
//!   bounded by the runtime response-body ceiling;
//! - nginx's per-file cap (`0` = spool tier disabled, so the response is
//!   delivered without a temp file, exactly like nginx);
//! - the **in-memory bound** is the resource's configured response buffer bank
//!   (`*_buffer_size + *_buffers * size`), so a body only reaches the disk once
//!   nginx would have exhausted its buffers too.
//!
//! Bounded difference (documented, not silent): when one spool file would
//! exceed `*_max_temp_file_size`, nginx stops spooling and passes the rest of
//! the response to the client synchronously. The runtime has no unbuffered
//! passthrough seam (the exchange always buffers before replying), so the
//! remainder is completed in memory and the response is served identically
//! (status, `Content-Length` and bytes). Only the storage tier differs.
//!
//! Every spool file is a self-contained guard: dropping [`SpoolFile`] removes
//! the file and releases its budget reservation, so a cancelled request, a
//! client abort or an upstream failure cannot leak a temp file.

use std::{
    io,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, AtomicUsize, Ordering},
        Arc,
    },
    time::{SystemTime, UNIX_EPOCH},
};

use axum::body::Body;
use bytes::Bytes;
use futures_util::stream;
use tokio::{
    fs::OpenOptions,
    io::{AsyncReadExt, AsyncWriteExt},
    time::Instant,
};

/// nginx's default `*_temp_file_write_size` for the default 4 KiB buffers.
pub(crate) const DEFAULT_WRITE_CHUNK: usize = 8 * 1024;

/// Bound on a range assembled across the spool file/tail boundary; above it a
/// cross-boundary range declines to the full representation (a documented
/// bounded difference, never a wrong body).
const MAX_ASSEMBLED_RANGE_BYTES: u64 = 8 * 1024 * 1024;

/// Read/serve chunk used when streaming a spool file back to the client.
const SERVE_CHUNK_MIN: usize = 512;

/// Typed temp-spool failure. Every variant is recoverable by degrading to the
/// in-memory tier; none of them may surface as a wrong response.
#[derive(Debug, thiserror::Error)]
pub(crate) enum SpoolError {
    #[error("temp-spool byte budget exhausted")]
    ByteBudget,
    #[error("temp-spool open-file budget exhausted")]
    FileBudget,
    #[error("temp-spool file would exceed `*_max_temp_file_size`")]
    FileLimit,
    #[error("temp-spool I/O failed: {0}")]
    Io(#[from] io::Error),
}

/// One per-runtime-generation spool root plus its shared budget counters.
pub(crate) struct TempSpool {
    generation_dir: PathBuf,
    budget_bytes: u64,
    max_files: usize,
    used_bytes: AtomicU64,
    used_files: AtomicUsize,
    sequence: AtomicU64,
}

impl TempSpool {
    /// Build the spool for one runtime generation.
    ///
    /// `root` is the operator-configured `[http] tempRoot`; `None` places the
    /// per-generation directory under the system temp directory. The directory
    /// is created lazily on the first spooled response, so a deployment that
    /// never overflows never touches the disk.
    pub(crate) fn new(
        root: Option<&str>,
        generation: u64,
        budget_bytes: u64,
        max_files: usize,
    ) -> Self {
        let base = root
            .filter(|root| !root.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        Self {
            generation_dir: base.join(format!(
                "sdkwork-webserver-{}-gen{generation}",
                std::process::id()
            )),
            budget_bytes: budget_bytes.max(1),
            max_files: max_files.max(1),
            used_bytes: AtomicU64::new(0),
            used_files: AtomicUsize::new(0),
            sequence: AtomicU64::new(0),
        }
    }

    /// The per-generation directory used when the resource declares no
    /// `*_temp_path`.
    pub(crate) fn generation_dir(&self) -> &Path {
        &self.generation_dir
    }

    /// Current in-flight spool bytes.
    pub(crate) fn used_bytes(&self) -> u64 {
        self.used_bytes.load(Ordering::Relaxed)
    }

    /// Current open spool files.
    pub(crate) fn used_files(&self) -> usize {
        self.used_files.load(Ordering::Relaxed)
    }

    fn reserve_bytes(&self, bytes: u64) -> Result<(), SpoolError> {
        let mut current = self.used_bytes.load(Ordering::Relaxed);
        loop {
            let next = current.checked_add(bytes).ok_or(SpoolError::ByteBudget)?;
            if next > self.budget_bytes {
                return Err(SpoolError::ByteBudget);
            }
            match self.used_bytes.compare_exchange_weak(
                current,
                next,
                Ordering::AcqRel,
                Ordering::Relaxed,
            ) {
                Ok(_) => return Ok(()),
                Err(actual) => current = actual,
            }
        }
    }

    fn release_bytes(&self, bytes: u64) {
        self.used_bytes
            .fetch_sub(bytes.min(self.used_bytes()), Ordering::AcqRel);
    }

    fn reserve_file(&self) -> Result<(), SpoolError> {
        let mut current = self.used_files.load(Ordering::Relaxed);
        loop {
            if current >= self.max_files {
                return Err(SpoolError::FileBudget);
            }
            match self.used_files.compare_exchange_weak(
                current,
                current + 1,
                Ordering::AcqRel,
                Ordering::Relaxed,
            ) {
                Ok(_) => return Ok(()),
                Err(actual) => current = actual,
            }
        }
    }

    fn release_file(&self) {
        self.used_files.fetch_sub(1, Ordering::AcqRel);
    }

    /// Create one unique `0600` spool file in `dir` (or the per-generation
    /// directory when the resource declares no `*_temp_path`).
    pub(crate) async fn create(
        self: &Arc<Self>,
        dir: Option<&Path>,
    ) -> Result<SpoolFile, SpoolError> {
        let dir = dir.unwrap_or(&self.generation_dir);
        tokio::fs::create_dir_all(dir).await?;
        // A spool directory must be a real directory this process created (or
        // an operator-owned one), never a symlink planted in a shared temp
        // directory: following it would redirect spool writes outside the
        // confinement root. A symlink degrades the response to the in-memory
        // tier instead.
        let metadata = tokio::fs::symlink_metadata(dir).await?;
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err(SpoolError::Io(io::Error::other(
                "temp-spool directory is not a plain directory",
            )));
        }
        #[cfg(unix)]
        if dir == self.generation_dir {
            // The per-generation directory is private to this process; files
            // themselves are always created 0600.
            use std::os::unix::fs::PermissionsExt;
            let _ = tokio::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700)).await;
        }
        self.reserve_file()?;
        let sequence = self.sequence.fetch_add(1, Ordering::Relaxed);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default();
        let path = dir.join(format!("{nanos:032x}-{sequence:016x}.tmp"));
        let file = match open_private(&path).await {
            Ok(file) => file,
            Err(error) => {
                self.release_file();
                return Err(SpoolError::Io(error));
            }
        };
        Ok(SpoolFile {
            spool: Arc::clone(self),
            path,
            file: Some(file),
            bytes: 0,
            released: false,
        })
    }
}

impl Drop for TempSpool {
    fn drop(&mut self) {
        // Every file in the per-generation directory is owned by a live or
        // dropped `SpoolFile`; once the generation (and therefore every
        // response body built from it) is gone the directory is empty and can
        // be removed. A configured `*_temp_path` is shared operator state and
        // is never removed here.
        tracing::debug!(
            directory = %self.generation_dir().display(),
            spool_files = self.used_files(),
            spool_bytes = self.used_bytes(),
            "releasing the CGI temp-spool generation"
        );
        if let Err(error) = std::fs::remove_dir(self.generation_dir()) {
            if error.kind() != io::ErrorKind::NotFound {
                tracing::debug!(
                    directory = %self.generation_dir().display(),
                    error = %error,
                    "temp-spool generation directory could not be removed"
                );
            }
        }
    }
}

/// Open one spool file write-only with mode `0600` (nginx's temp-file mode).
async fn open_private(path: &Path) -> io::Result<tokio::fs::File> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    options.mode(0o600);
    options.open(path).await
}

/// One open spool file. Dropping it removes the file and releases the budget
/// reservation.
pub(crate) struct SpoolFile {
    spool: Arc<TempSpool>,
    path: PathBuf,
    file: Option<tokio::fs::File>,
    bytes: u64,
    released: bool,
}

impl SpoolFile {
    /// Bytes written so far.
    pub(crate) fn bytes(&self) -> u64 {
        self.bytes
    }

    /// Filesystem path (the file is removed on drop).
    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    /// Append one upstream chunk, split into `write_chunk`-sized writes and
    /// bounded by `max_file_bytes` (0 = no per-file cap).
    pub(crate) async fn append(
        &mut self,
        data: &[u8],
        write_chunk: usize,
        max_file_bytes: u64,
    ) -> Result<(), SpoolError> {
        if data.is_empty() {
            return Ok(());
        }
        if max_file_bytes != 0 && self.bytes.saturating_add(data.len() as u64) > max_file_bytes {
            return Err(SpoolError::FileLimit);
        }
        let chunk = write_chunk.max(1);
        for piece in data.chunks(chunk) {
            self.spool.reserve_bytes(piece.len() as u64)?;
            let Some(file) = self.file.as_mut() else {
                self.spool.release_bytes(piece.len() as u64);
                return Err(SpoolError::Io(io::Error::new(
                    io::ErrorKind::BrokenPipe,
                    "spool file is closed",
                )));
            };
            if let Err(error) = file.write_all(piece).await {
                self.spool.release_bytes(piece.len() as u64);
                return Err(SpoolError::Io(error));
            }
            self.bytes += piece.len() as u64;
        }
        Ok(())
    }

    /// Flush and close the write handle so the file can be served.
    pub(crate) async fn finish(&mut self) -> io::Result<()> {
        if let Some(mut file) = self.file.take() {
            file.flush().await?;
        }
        Ok(())
    }

    /// Stream the whole spooled file into `writer` (used by `*_store` and the
    /// flush-failure fallback so the body is never lost).
    pub(crate) async fn copy_to<W>(&self, writer: &mut W) -> io::Result<u64>
    where
        W: AsyncWriteExt + Unpin + ?Sized,
    {
        let mut file = tokio::fs::File::open(&self.path).await?;
        let mut buffer = vec![0u8; DEFAULT_WRITE_CHUNK];
        let mut written = 0u64;
        loop {
            let read = file.read(&mut buffer).await?;
            if read == 0 {
                break;
            }
            writer.write_all(&buffer[..read]).await?;
            written += read as u64;
        }
        Ok(written)
    }

    /// Read one bounded slice from the file.
    pub(crate) async fn read_slice(&self, start: u64, end: u64) -> io::Result<Vec<u8>> {
        use tokio::io::AsyncSeekExt;
        let length = end.saturating_sub(start);
        let mut file = tokio::fs::File::open(&self.path).await?;
        file.seek(io::SeekFrom::Start(start)).await?;
        let mut buffer = vec![
            0u8;
            usize::try_from(length).map_err(|_| io::Error::new(
                io::ErrorKind::InvalidInput,
                "spool range is too large"
            ))?
        ];
        file.read_exact(&mut buffer).await?;
        Ok(buffer)
    }
}

impl Drop for SpoolFile {
    fn drop(&mut self) {
        self.file.take();
        if let Err(error) = std::fs::remove_file(&self.path) {
            if error.kind() != io::ErrorKind::NotFound {
                tracing::debug!(
                    path = %self.path.display(),
                    error = %error,
                    "temp-spool file could not be removed"
                );
            }
        }
        if !self.released {
            self.released = true;
            self.spool.release_bytes(self.bytes);
            self.spool.release_file();
        }
    }
}

/// The buffered upstream body once the exchange completed.
///
/// `Spooled` keeps the file (until served or dropped) plus an in-memory tail
/// for the bytes past `*_max_temp_file_size`.
pub(crate) enum BufferedResponseBody {
    Memory(Vec<u8>),
    Spooled {
        file: SpoolFile,
        tail: Vec<u8>,
    },
    SpoolRange {
        file: SpoolFile,
        start: u64,
        end: u64,
    },
}

impl BufferedResponseBody {
    /// Exact body length in bytes.
    pub(crate) fn len(&self) -> u64 {
        match self {
            Self::Memory(body) => body.len() as u64,
            Self::Spooled { file, tail } => file.bytes().saturating_add(tail.len() as u64),
            Self::SpoolRange { start, end, .. } => end.saturating_sub(*start),
        }
    }

    /// Whether the body is persisted on disk.
    pub(crate) fn is_spooled(&self) -> bool {
        !matches!(self, Self::Memory(_))
    }

    /// Borrow the in-memory representation when the body never spooled.
    #[cfg(test)]
    pub(crate) fn memory(&self) -> Option<&[u8]> {
        match self {
            Self::Memory(body) => Some(body),
            _ => None,
        }
    }

    /// nginx's single-range semantics: return the requested slice.
    ///
    /// A slice that lives in the spool file streams from disk (never
    /// materialized); one that lives in the in-memory tail, or crosses the
    /// file/tail boundary under the assembly bound, is copied into memory.
    /// `Err(body)` means "decline the range and serve the full
    /// representation" (the same bounded behaviour as a multi-range request),
    /// and returns the untouched body so the full representation is intact.
    pub(crate) async fn slice(self, start: u64, end: u64) -> Result<Self, Self> {
        match self {
            Self::Memory(body) => {
                let start = usize::try_from(start).unwrap_or(usize::MAX).min(body.len());
                let end = usize::try_from(end).unwrap_or(usize::MAX).min(body.len());
                if start >= end {
                    return Ok(Self::Memory(Vec::new()));
                }
                Ok(Self::Memory(body[start..end].to_vec()))
            }
            Self::Spooled { file, tail } => {
                let file_bytes = file.bytes();
                if end <= file_bytes {
                    return Ok(Self::SpoolRange { file, start, end });
                }
                if start >= file_bytes {
                    let start = usize::try_from(start - file_bytes)
                        .unwrap_or(usize::MAX)
                        .min(tail.len());
                    let end = usize::try_from(end - file_bytes)
                        .unwrap_or(usize::MAX)
                        .min(tail.len());
                    return Ok(Self::Memory(tail[start..end].to_vec()));
                }
                if end.saturating_sub(start) > MAX_ASSEMBLED_RANGE_BYTES {
                    return Err(Self::Spooled { file, tail });
                }
                let mut bytes = match file.read_slice(start, file_bytes).await {
                    Ok(bytes) => bytes,
                    Err(error) => {
                        tracing::debug!(
                            error = %error,
                            "spooled range could not be read; declining the range"
                        );
                        return Err(Self::Spooled { file, tail });
                    }
                };
                let tail_end = usize::try_from(end - file_bytes)
                    .unwrap_or(usize::MAX)
                    .min(tail.len());
                bytes.extend_from_slice(&tail[..tail_end]);
                Ok(Self::Memory(bytes))
            }
            Self::SpoolRange { .. } => Err(Self::Memory(Vec::new())),
        }
    }

    /// Stream the whole body into `writer` (the `*_store` path).
    pub(crate) async fn copy_to<W>(&mut self, writer: &mut W) -> io::Result<u64>
    where
        W: AsyncWriteExt + Unpin + ?Sized,
    {
        match self {
            Self::Memory(body) => {
                writer.write_all(body).await?;
                Ok(body.len() as u64)
            }
            Self::Spooled { file, tail } => {
                let mut written = file.copy_to(writer).await?;
                writer.write_all(tail).await?;
                written += tail.len() as u64;
                Ok(written)
            }
            Self::SpoolRange { file, start, end } => {
                let bytes = file.read_slice(*start, *end).await?;
                writer.write_all(&bytes).await?;
                Ok(bytes.len() as u64)
            }
        }
    }

    /// Turn the body into an axum response body, optionally paced
    /// (`*_limit_rate`) at `rate` bytes/second.
    pub(crate) fn into_body(self, rate: Option<u64>, write_chunk: usize) -> Body {
        let chunk = write_chunk.max(SERVE_CHUNK_MIN);
        match self {
            Self::Memory(body) => paced_memory_body(body, rate),
            Self::Spooled { file, tail } => spool_stream(file, 0, None, tail, rate, chunk),
            Self::SpoolRange { file, start, end } => {
                spool_stream(file, start, Some(end), Vec::new(), rate, chunk)
            }
        }
    }
}

/// Build a response body stream over a spool file window plus an in-memory
/// tail. The [`SpoolFile`] guard travels inside the stream state, so dropping
/// the response body early removes the file.
fn spool_stream(
    file: SpoolFile,
    start: u64,
    end: Option<u64>,
    tail: Vec<u8>,
    rate: Option<u64>,
    chunk: usize,
) -> Body {
    let file_end = end.unwrap_or_else(|| file.bytes());
    let stream = stream::unfold(
        SpoolStreamState {
            file,
            tail,
            offset: start,
            file_end,
            served: 0,
            rate,
            bucket: Instant::now(),
            chunk,
            read: None,
        },
        |mut state| async move {
            if let Some(rate) = state.rate.filter(|rate| *rate > 0) {
                let target = state.bucket
                    + std::time::Duration::from_secs_f64(state.served as f64 / rate as f64);
                let now = Instant::now();
                if target > now {
                    tokio::time::sleep(target - now).await;
                }
            }
            if state.offset >= state.file_end {
                if state.tail.is_empty() {
                    return None;
                }
                let piece = std::mem::take(&mut state.tail);
                state.served += piece.len() as u64;
                return Some((Ok::<Bytes, io::Error>(Bytes::from(piece)), state));
            }
            if state.read.is_none() {
                match tokio::fs::File::open(state.file.path()).await {
                    Ok(file) => {
                        state.read = Some(file);
                        if state.offset > 0 {
                            use tokio::io::AsyncSeekExt;
                            if let Some(read) = state.read.as_mut() {
                                if let Err(error) =
                                    read.seek(io::SeekFrom::Start(state.offset)).await
                                {
                                    return Some((Err::<Bytes, io::Error>(error), state));
                                }
                            }
                        }
                    }
                    Err(error) => return Some((Err::<Bytes, io::Error>(error), state)),
                }
            }
            let remaining = state.file_end - state.offset;
            let want = state
                .chunk
                .min(usize::try_from(remaining).unwrap_or(usize::MAX));
            let mut buffer = vec![0u8; want];
            let read = state.read.as_mut()?;
            match read.read_exact(&mut buffer).await {
                Ok(_) => {
                    state.offset += want as u64;
                    state.served += want as u64;
                    Some((Ok::<Bytes, io::Error>(Bytes::from(buffer)), state))
                }
                Err(error) => Some((Err::<Bytes, io::Error>(error), state)),
            }
        },
    );
    Body::from_stream(stream)
}

struct SpoolStreamState {
    file: SpoolFile,
    tail: Vec<u8>,
    /// Absolute file offset already served (or the range start).
    offset: u64,
    /// Absolute file offset of the end of the served window.
    file_end: u64,
    /// Bytes already handed to the client (pacing clock).
    served: u64,
    chunk: usize,
    rate: Option<u64>,
    bucket: Instant,
    read: Option<tokio::fs::File>,
}

/// A fully buffered in-memory body, optionally paced by `*_limit_rate`.
pub(crate) fn paced_memory_body(body: Vec<u8>, rate: Option<u64>) -> Body {
    let Some(rate) = rate.filter(|rate| *rate > 0) else {
        return Body::from(body);
    };
    // ~10 pacing steps per second, bounded by the response chunk window.
    let chunk = usize::try_from((rate / 10).clamp(1024, 64 * 1024)).unwrap_or(1024);
    let length = body.len();
    let stream = stream::unfold(
        (body, 0usize, Instant::now(), rate, chunk),
        move |(body, offset, started, rate, chunk)| async move {
            if offset >= body.len() {
                return None;
            }
            let end = (offset + chunk).min(length);
            let target = started + std::time::Duration::from_secs_f64(end as f64 / rate as f64);
            let now = Instant::now();
            if target > now {
                tokio::time::sleep(target - now).await;
            }
            let piece = Bytes::copy_from_slice(&body[offset..end]);
            Some((
                Ok::<Bytes, std::io::Error>(piece),
                (body, end, started, rate, chunk),
            ))
        },
    );
    Body::from_stream(stream)
}

/// Resolved spool tier of one resource, cheap to clone into each exchange.
#[derive(Clone)]
pub(crate) struct BodyBufferPolicy {
    pub spool: Arc<TempSpool>,
    pub spool_dir: Option<PathBuf>,
    pub memory_bound: usize,
    pub write_chunk: usize,
    pub max_file_bytes: u64,
}

impl BodyBufferPolicy {
    /// Build the per-response body sink.
    pub(crate) fn buffer(&self) -> SpoolingBodyBuffer {
        SpoolingBodyBuffer::new(
            Arc::clone(&self.spool),
            self.spool_dir.clone(),
            self.memory_bound,
            self.write_chunk,
            self.max_file_bytes,
        )
    }
}

/// The buffer tier of one response body: in memory up to `memory_bound`, then
/// the spool file, then (past `max_file_bytes`) an in-memory tail.
pub(crate) struct SpoolingBodyBuffer {
    spool: Arc<TempSpool>,
    spool_dir: Option<PathBuf>,
    memory_bound: usize,
    write_chunk: usize,
    max_file_bytes: u64,
    memory: Vec<u8>,
    file: Option<SpoolFile>,
    /// Spool tier disabled for the rest of this response (budget, I/O error or
    /// past `max_file_bytes`).
    spool_disabled: bool,
    total: u64,
}

impl SpoolingBodyBuffer {
    /// Build a response buffer for one resource.
    ///
    /// `max_file_bytes == 0` disables the spool tier exactly like nginx.
    pub(crate) fn new(
        spool: Arc<TempSpool>,
        spool_dir: Option<PathBuf>,
        memory_bound: usize,
        write_chunk: usize,
        max_file_bytes: u64,
    ) -> Self {
        Self {
            spool,
            spool_dir,
            memory_bound: memory_bound.max(1),
            write_chunk: write_chunk.max(1),
            max_file_bytes,
            memory: Vec::new(),
            file: None,
            spool_disabled: max_file_bytes == 0,
            total: 0,
        }
    }

    /// Bytes accepted so far.
    pub(crate) fn total(&self) -> u64 {
        self.total
    }

    /// Whether the body currently lives on disk.
    pub(crate) fn is_spooled(&self) -> bool {
        self.file.is_some()
    }

    /// Append one decoded upstream chunk. `hard_limit` is the runtime's
    /// response-body ceiling; exceeding it is a protocol failure exactly like
    /// before the spool tier existed.
    pub(crate) async fn append(
        &mut self,
        chunk: &[u8],
        hard_limit: u64,
    ) -> Result<(), SpoolLimitExceeded> {
        let next = self.total.saturating_add(chunk.len() as u64);
        if next > hard_limit {
            return Err(SpoolLimitExceeded);
        }
        self.total = next;
        if let Some(file) = self.file.as_mut() {
            match file
                .append(chunk, self.write_chunk, self.max_file_bytes)
                .await
            {
                Ok(()) => return Ok(()),
                Err(SpoolError::FileLimit) => {
                    self.spool_disabled = true;
                }
                Err(error) => {
                    tracing::debug!(error = %error, "temp-spool write failed; buffering in memory");
                    self.spool_disabled = true;
                }
            }
            self.memory.extend_from_slice(chunk);
            return Ok(());
        }
        if self.spool_disabled || self.memory.len().saturating_add(chunk.len()) <= self.memory_bound
        {
            self.memory.extend_from_slice(chunk);
            return Ok(());
        }
        // The configured in-memory response buffer bank is exhausted: nginx
        // starts a temp file here.
        match self.spool.create(self.spool_dir.as_deref()).await {
            Ok(mut file) => {
                let prefix = std::mem::take(&mut self.memory);
                if let Err(error) = file
                    .append(&prefix, self.write_chunk, self.max_file_bytes)
                    .await
                {
                    tracing::debug!(error = %error, "temp-spool prefix write failed; buffering in memory");
                    self.memory = prefix;
                    self.spool_disabled = true;
                    self.memory.extend_from_slice(chunk);
                    return Ok(());
                }
                match file
                    .append(chunk, self.write_chunk, self.max_file_bytes)
                    .await
                {
                    Ok(()) => {
                        self.file = Some(file);
                        Ok(())
                    }
                    Err(SpoolError::FileLimit) => {
                        // The prefix spooled but this chunk does not fit: keep
                        // the file and complete the response in memory.
                        self.spool_disabled = true;
                        self.memory.extend_from_slice(chunk);
                        self.file = Some(file);
                        Ok(())
                    }
                    Err(error) => {
                        tracing::debug!(error = %error, "temp-spool write failed; buffering in memory");
                        self.spool_disabled = true;
                        self.memory = prefix;
                        self.memory.extend_from_slice(chunk);
                        Ok(())
                    }
                }
            }
            Err(error) => {
                tracing::debug!(error = %error, "temp-spool file unavailable; buffering in memory");
                self.spool_disabled = true;
                self.memory.extend_from_slice(chunk);
                Ok(())
            }
        }
    }

    /// Close the spool (if any) and produce the servable body.
    pub(crate) async fn finish(mut self) -> Result<BufferedResponseBody, SpoolError> {
        tracing::debug!(
            spooled = self.is_spooled(),
            bytes = self.total(),
            "CGI upstream body buffer finalized"
        );
        let Some(mut file) = self.file.take() else {
            return Ok(BufferedResponseBody::Memory(self.memory));
        };
        if let Err(error) = file.finish().await {
            // The spooled prefix must survive a flush failure: copy it back
            // into memory so the body is not truncated.
            let mut bytes = Vec::new();
            if let Err(copy_error) = file.copy_to(&mut bytes).await {
                tracing::debug!(
                    error = %copy_error,
                    "temp-spool flush and recovery both failed"
                );
                return Err(SpoolError::Io(copy_error));
            }
            bytes.extend_from_slice(&self.memory);
            tracing::debug!(error = %error, "temp-spool flush failed; body recovered from disk");
            return Ok(BufferedResponseBody::Memory(bytes));
        }
        Ok(BufferedResponseBody::Spooled {
            file,
            tail: self.memory,
        })
    }
}

/// Hard response-body ceiling exceeded (`limits.max_response_body_bytes`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SpoolLimitExceeded;

#[cfg(test)]
mod tests {
    use super::*;

    fn spool() -> Arc<TempSpool> {
        Arc::new(TempSpool::new(None, 1, 1024 * 1024, 4))
    }

    #[tokio::test]
    async fn spool_files_are_private_unique_and_removed_on_drop() {
        let spool = spool();
        let dir = spool.generation_dir().to_path_buf();
        let first = spool.create(None).await.expect("spool file");
        let second = spool.create(None).await.expect("spool file");
        assert_ne!(first.path(), second.path());
        assert!(first.path().exists());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(first.path())
                .expect("metadata")
                .permissions()
                .mode()
                & 0o777;
            assert_eq!(mode, 0o600, "spool file must be created 0600");
        }
        assert_eq!(spool.used_files(), 2);
        let path = first.path().to_path_buf();
        drop(first);
        assert!(!path.exists(), "dropping a spool file removes it");
        assert_eq!(spool.used_files(), 1);
        assert!(dir.exists());
    }

    #[tokio::test]
    async fn spool_budget_bounds_total_bytes_and_files() {
        let spool = Arc::new(TempSpool::new(None, 2, 8, 1));
        let mut file = spool.create(None).await.expect("spool file");
        assert_eq!(spool.used_files(), 1);
        assert!(matches!(
            spool.create(None).await,
            Err(SpoolError::FileBudget)
        ));
        file.append(b"12345678", 4, 0).await.expect("first write");
        assert_eq!(spool.used_bytes(), 8);
        assert!(matches!(
            file.append(b"9", 4, 0).await,
            Err(SpoolError::ByteBudget)
        ));
        drop(file);
        assert_eq!(spool.used_bytes(), 0);
        assert_eq!(spool.used_files(), 0);
    }

    #[tokio::test]
    async fn spool_file_limit_is_reported_without_partial_write() {
        let spool = spool();
        let mut file = spool.create(None).await.expect("spool file");
        file.append(b"1234", 4, 8).await.expect("within limit");
        assert!(matches!(
            file.append(b"56789", 4, 8).await,
            Err(SpoolError::FileLimit)
        ));
        assert_eq!(file.bytes(), 4);
    }

    #[tokio::test]
    async fn body_buffer_spools_only_past_the_in_memory_bound() {
        let spool = spool();
        let mut buffer = SpoolingBodyBuffer::new(Arc::clone(&spool), None, 8, 4, 1024 * 1024);
        buffer.append(b"1234", 1024).await.expect("append");
        assert!(!buffer.is_spooled());
        assert_eq!(spool.used_files(), 0);
        buffer.append(b"5678", 1024).await.expect("append");
        assert!(!buffer.is_spooled(), "exactly the bound stays in memory");
        buffer.append(b"9", 1024).await.expect("append");
        assert!(buffer.is_spooled());
        assert_eq!(spool.used_files(), 1);
        let body = buffer.finish().await.expect("body");
        assert_eq!(body.len(), 9);
        assert!(body.is_spooled());
        let bytes = body.into_body(None, 4);
        let collected = http_body_util::BodyExt::collect(bytes)
            .await
            .expect("body")
            .to_bytes();
        assert_eq!(&collected[..], b"123456789");
        assert_eq!(spool.used_files(), 0, "serving drops the spool file");
    }

    #[tokio::test]
    async fn body_buffer_zero_file_size_disables_spooling() {
        let spool = spool();
        let mut buffer = SpoolingBodyBuffer::new(Arc::clone(&spool), None, 4, 4, 0);
        buffer.append(b"12345678", 1024).await.expect("append");
        assert!(!buffer.is_spooled());
        let body = buffer.finish().await.expect("body");
        assert_eq!(body.memory(), Some(&b"12345678"[..]));
    }

    #[tokio::test]
    async fn body_buffer_completes_in_memory_past_max_file_size() {
        let spool = spool();
        let mut buffer = SpoolingBodyBuffer::new(Arc::clone(&spool), None, 4, 4, 4);
        buffer.append(b"1234", 1024).await.expect("append");
        buffer.append(b"5678", 1024).await.expect("append");
        let body = buffer.finish().await.expect("body");
        assert_eq!(body.len(), 8);
        let bytes = http_body_util::BodyExt::collect(body.into_body(None, 4))
            .await
            .expect("body")
            .to_bytes();
        assert_eq!(&bytes[..], b"12345678");
    }

    #[tokio::test]
    async fn spooled_body_ranges_read_from_disk() {
        let spool = spool();
        let mut buffer = SpoolingBodyBuffer::new(Arc::clone(&spool), None, 4, 4, 1 << 20);
        buffer.append(b"0123456789", 1024).await.expect("append");
        let body = buffer.finish().await.expect("body");
        assert_eq!(body.len(), 10);
        let slice = match body.slice(2, 5).await {
            Ok(slice) => slice,
            Err(_) => panic!("range must be served from the spool"),
        };
        assert_eq!(slice.len(), 3);
        let bytes = http_body_util::BodyExt::collect(slice.into_body(None, 4))
            .await
            .expect("range body")
            .to_bytes();
        assert_eq!(&bytes[..], b"234");
    }

    #[tokio::test]
    async fn configured_spool_directory_is_used_and_removed() {
        let spool = spool();
        let configured = spool.generation_dir().join("per-protocol");
        let mut file = spool
            .create(Some(configured.as_path()))
            .await
            .expect("spool file");
        assert!(file.path().starts_with(&configured));
        file.append(b"body", 4, 0).await.expect("append");
        let path = file.path().to_path_buf();
        file.finish().await.expect("finish");
        assert!(path.exists());
        drop(file);
        assert!(!path.exists());
    }
}
