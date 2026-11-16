use std::{
    cmp::Ordering,
    collections::BinaryHeap,
    path::PathBuf,
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicU64, Ordering as AtomicOrdering},
    },
    thread,
};

use futures::channel::oneshot;
use zpdf::TextSpan;

use crate::{
    document::{
        LoadedPdfDocument, PageAnnotation, PagePreview, PdfDocumentDetails, PdfDocumentSummary,
    },
    edit::{EditOutcome, PdfEdit},
};

/// Relative urgency of a render/text request. Foreground work (the pages the
/// user is looking at) always preempts background prefetch in the shared queue.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Priority {
    Background,
    Foreground,
}

enum Work {
    Render {
        index: usize,
        dpi: f32,
        respond: oneshot::Sender<anyhow::Result<PagePreview>>,
    },
    PageSpans {
        index: usize,
        respond: oneshot::Sender<anyhow::Result<Vec<TextSpan>>>,
    },
    PageText {
        index: usize,
        respond: oneshot::Sender<anyhow::Result<String>>,
    },
    PageAnnotations {
        index: usize,
        respond: oneshot::Sender<anyhow::Result<Vec<PageAnnotation>>>,
    },
    DocumentDetails {
        respond: oneshot::Sender<anyhow::Result<PdfDocumentDetails>>,
    },
    /// Apply edits and write them back. Carries the whole batch so a multi-field
    /// form fill lands as one incremental update.
    Edit {
        edits: Vec<PdfEdit>,
        respond: oneshot::Sender<anyhow::Result<EditOutcome>>,
    },
}

impl Work {
    /// Whether the awaiting receiver has been dropped. Such jobs are skipped at
    /// dequeue time so superseded background prefetch is discarded cheaply
    /// instead of occupying a worker thread.
    fn is_canceled(&self) -> bool {
        match self {
            Work::Render { respond, .. } => respond.is_canceled(),
            Work::PageSpans { respond, .. } => respond.is_canceled(),
            Work::PageText { respond, .. } => respond.is_canceled(),
            Work::PageAnnotations { respond, .. } => respond.is_canceled(),
            Work::DocumentDetails { respond } => respond.is_canceled(),
            // An edit already accepted from the user must run: dropping its
            // receiver cannot undo the write-back the user asked for.
            Work::Edit { .. } => false,
        }
    }
}

struct Job {
    priority: Priority,
    /// Tie-breaker so equal-priority jobs run in submission order (FIFO). A
    /// lower sequence is older and should run first.
    sequence: u64,
    work: Work,
}

impl PartialEq for Job {
    fn eq(&self, other: &Self) -> bool {
        self.priority == other.priority && self.sequence == other.sequence
    }
}
impl Eq for Job {}

impl Ord for Job {
    fn cmp(&self, other: &Self) -> Ordering {
        // `BinaryHeap` is a max-heap, so the greatest `Job` is popped first:
        // higher priority wins, then the smaller (older) sequence wins.
        self.priority
            .cmp(&other.priority)
            .then_with(|| other.sequence.cmp(&self.sequence))
    }
}
impl PartialOrd for Job {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Default)]
struct Queue {
    jobs: BinaryHeap<Job>,
    /// Set once when the worker is dropped so idle threads can exit.
    shutdown: bool,
}

/// The newest document bytes, shared so every pool thread reloads after an edit
/// without the viewer having to respawn the pool.
struct Published {
    data: Arc<[u8]>,
    generation: u64,
}

struct Shared {
    queue: Mutex<Queue>,
    available: Condvar,
    published: Mutex<Published>,
    /// Serializes edits so two threads can never build an incremental update
    /// from the same base bytes and lose one of them.
    edit_lock: Mutex<()>,
    ready: AtomicU64,
}

/// Owns a pool of dedicated OS threads, each holding its own
/// [`LoadedPdfDocument`].
///
/// `zpdf::PdfDocument` is neither `Send` nor `Sync` (it caches decoded objects
/// in `RefCell`/`OnceCell`), so it cannot be moved between threads or onto
/// GPUI's background executor. Instead each pool thread opens the document from
/// the same shared `Arc<[u8]>` — the raw bytes are shared, only the per-thread
/// parse caches are duplicated — and pulls jobs from a shared priority queue.
/// Every value that crosses the channel boundary (`PagePreview`,
/// `PdfDocumentSummary`, `PageAnnotation`, `Vec<u8>`) is `Send`.
pub struct PdfWorker {
    shared: Arc<Shared>,
    summary: Mutex<Arc<PdfDocumentSummary>>,
    next_sequence: Mutex<u64>,
}

/// Upper bound on pool threads. Each thread duplicates the document's parse
/// caches, so this is kept small to bound memory on large files.
const MAX_WORKER_THREADS: usize = 4;

impl PdfWorker {
    /// Spawn the pool, open the document on the first thread to obtain the
    /// summary, then bring the remaining threads online. Resolves once the
    /// summary (page count, dimensions, outline, metadata) is available.
    pub async fn open(path: PathBuf, data: Arc<[u8]>, password: Vec<u8>) -> anyhow::Result<Self> {
        let thread_count = thread::available_parallelism()
            .map_or(1, |count| count.get().saturating_sub(1))
            .clamp(1, MAX_WORKER_THREADS);

        let shared = Arc::new(Shared {
            queue: Mutex::new(Queue::default()),
            available: Condvar::new(),
            published: Mutex::new(Published {
                data: data.clone(),
                generation: 1,
            }),
            edit_lock: Mutex::new(()),
            ready: AtomicU64::new(0),
        });

        let (init_tx, init_rx) = oneshot::channel::<anyhow::Result<Arc<PdfDocumentSummary>>>();
        let mut init_tx = Some(init_tx);

        for thread_index in 0..thread_count {
            let shared = shared.clone();
            let path = path.clone();
            let data = data.clone();
            let mut password = password.clone();
            // Only the first thread reports the summary back to the opener.
            let init_tx = init_tx.take();
            thread::Builder::new()
                .name(format!("pdf-worker-{thread_index}"))
                .spawn(move || {
                    let document = match if password.is_empty() {
                        LoadedPdfDocument::open(path, data)
                    } else {
                        LoadedPdfDocument::open_with_password(path, data, &password)
                    } {
                        Ok(document) => {
                            shared.ready.fetch_add(1, AtomicOrdering::SeqCst);
                            if let Some(init_tx) = init_tx {
                                let summary = Arc::new(document.summary.clone());
                                if init_tx.send(Ok(summary)).is_err() {
                                    password.fill(0);
                                    return;
                                }
                            }
                            password.fill(0);
                            document
                        }
                        Err(error) => {
                            password.fill(0);
                            if let Some(init_tx) = init_tx {
                                init_tx.send(Err(error)).ok();
                            }
                            return;
                        }
                    };
                    worker_loop(document, shared);
                })?;
        }

        let mut password = password;
        password.fill(0);

        let summary = init_rx.await??;
        Ok(Self {
            shared,
            summary: Mutex::new(summary),
            next_sequence: Mutex::new(0),
        })
    }

    /// The document summary as of the last completed edit.
    pub fn summary(&self) -> Arc<PdfDocumentSummary> {
        self.summary
            .lock()
            .unwrap_or_else(|err| err.into_inner())
            .clone()
    }

    /// Queue a page render. The returned receiver resolves with the rendered
    /// bitmap, or an error if rendering failed or the worker is gone. Dropping
    /// the receiver before the job runs cancels it.
    pub fn render_page(
        &self,
        index: usize,
        dpi: f32,
        priority: Priority,
    ) -> oneshot::Receiver<anyhow::Result<PagePreview>> {
        let (respond, receiver) = oneshot::channel();
        self.submit(
            priority,
            Work::Render {
                index,
                dpi,
                respond,
            },
        );
        receiver
    }

    /// Queue text extraction for a page, used for search. Spans carry geometry so
    /// the view can draw a match instead of only scrolling to its page.
    pub fn page_spans(
        &self,
        index: usize,
        priority: Priority,
    ) -> oneshot::Receiver<anyhow::Result<Vec<TextSpan>>> {
        let (respond, receiver) = oneshot::channel();
        self.submit(priority, Work::PageSpans { index, respond });
        receiver
    }

    /// Queue plain-text extraction for a page, used by copy-to-clipboard.
    pub fn page_text(
        &self,
        index: usize,
        priority: Priority,
    ) -> oneshot::Receiver<anyhow::Result<String>> {
        let (respond, receiver) = oneshot::channel();
        self.submit(priority, Work::PageText { index, respond });
        receiver
    }

    /// Queue annotation extraction for a page, used by the annotation and link
    /// overlays.
    pub fn page_annotations(
        &self,
        index: usize,
        priority: Priority,
    ) -> oneshot::Receiver<anyhow::Result<Vec<PageAnnotation>>> {
        let (respond, receiver) = oneshot::channel();
        self.submit(priority, Work::PageAnnotations { index, respond });
        receiver
    }

    /// Queue the on-demand document details (metadata, signatures, attachments).
    pub fn document_details(&self) -> oneshot::Receiver<anyhow::Result<PdfDocumentDetails>> {
        let (respond, receiver) = oneshot::channel();
        self.submit(Priority::Foreground, Work::DocumentDetails { respond });
        receiver
    }

    /// Queue edits. They are applied as one incremental update and written back;
    /// the outcome carries the new bytes and the reopened summary.
    pub fn apply_edits(
        &self,
        edits: Vec<PdfEdit>,
    ) -> oneshot::Receiver<anyhow::Result<EditOutcome>> {
        let (respond, receiver) = oneshot::channel();
        self.submit(Priority::Foreground, Work::Edit { edits, respond });
        receiver
    }

    /// Adopt edited bytes: the edit's own thread is already current, and every
    /// other thread reloads before its next job.
    pub fn publish(&self, outcome: &EditOutcome) {
        self.published().data = outcome.bytes.clone();
        self.published().generation += 1;
        *self.summary.lock().unwrap_or_else(|err| err.into_inner()) = outcome.summary.clone();
    }

    fn published(&self) -> std::sync::MutexGuard<'_, Published> {
        self.shared
            .published
            .lock()
            .unwrap_or_else(|err| err.into_inner())
    }

    fn submit(&self, priority: Priority, work: Work) {
        let sequence = {
            let mut next = self
                .next_sequence
                .lock()
                .unwrap_or_else(|err| err.into_inner());
            let sequence = *next;
            *next = next.wrapping_add(1);
            sequence
        };
        let mut queue = self
            .shared
            .queue
            .lock()
            .unwrap_or_else(|err| err.into_inner());
        queue.jobs.push(Job {
            priority,
            sequence,
            work,
        });
        drop(queue);
        self.shared.available.notify_one();
    }
}

impl Drop for PdfWorker {
    fn drop(&mut self) {
        if let Ok(mut queue) = self.shared.queue.lock() {
            queue.shutdown = true;
        }
        self.shared.available.notify_all();
    }
}

/// Reload this thread's document when another thread published newer bytes.
///
/// Called before every job so a render never mixes an old page parse with a
/// bitmap the viewer expects to reflect the latest edit.
fn reload_if_stale(
    document: &mut LoadedPdfDocument,
    shared: &Shared,
    generation: &mut u64,
) -> anyhow::Result<()> {
    let (data, published) = {
        let published = shared
            .published
            .lock()
            .unwrap_or_else(|err| err.into_inner());
        (published.data.clone(), published.generation)
    };
    if published == *generation {
        return Ok(());
    }
    document.reload(data)?;
    *generation = published;
    Ok(())
}

fn worker_loop(mut document: LoadedPdfDocument, shared: Arc<Shared>) {
    let mut generation = {
        let published = shared
            .published
            .lock()
            .unwrap_or_else(|err| err.into_inner());
        published.generation
    };
    loop {
        let job = {
            let mut queue = shared.queue.lock().unwrap_or_else(|err| err.into_inner());
            loop {
                if queue.shutdown {
                    return;
                }
                if let Some(job) = queue.jobs.pop() {
                    break job;
                }
                queue = shared
                    .available
                    .wait(queue)
                    .unwrap_or_else(|err| err.into_inner());
            }
        };

        // The awaiting view may have dropped the receiver (e.g. the page was
        // evicted or a newer request superseded it) while the job waited.
        if job.work.is_canceled() {
            continue;
        }

        // Edits take the lock before reloading so the batch that runs is built
        // from the newest published bytes.
        let edits_guard = match &job.work {
            Work::Edit { .. } => Some(
                shared
                    .edit_lock
                    .lock()
                    .unwrap_or_else(|err| err.into_inner()),
            ),
            _ => None,
        };
        if let Err(error) = reload_if_stale(&mut document, &shared, &mut generation) {
            match job.work {
                Work::Edit { respond, .. } => {
                    respond.send(Err(error)).ok();
                    continue;
                }
                Work::Render { respond, .. } => {
                    respond.send(Err(error)).ok();
                    continue;
                }
                Work::PageSpans { respond, .. } => {
                    respond.send(Err(error)).ok();
                    continue;
                }
                Work::PageText { respond, .. } => {
                    respond.send(Err(error)).ok();
                    continue;
                }
                Work::PageAnnotations { respond, .. } => {
                    respond.send(Err(error)).ok();
                    continue;
                }
                Work::DocumentDetails { respond } => {
                    respond.send(Err(error)).ok();
                    continue;
                }
            }
        }

        match job.work {
            Work::Render {
                index,
                dpi,
                respond,
            } => {
                respond.send(document.render_page_preview(index, dpi)).ok();
            }
            Work::PageSpans { index, respond } => {
                respond.send(document.page_spans(index)).ok();
            }
            Work::PageText { index, respond } => {
                respond.send(document.page_text(index)).ok();
            }
            Work::PageAnnotations { index, respond } => {
                respond.send(document.page_annotations(index)).ok();
            }
            Work::DocumentDetails { respond } => {
                respond.send(document.document_details()).ok();
            }
            Work::Edit { edits, respond } => {
                let result = document.apply_edits(&edits).and_then(|bytes| {
                    let bytes = Arc::<[u8]>::from(bytes);
                    // Summarize the new bytes with a throwaway open: this thread
                    // keeps serving the old document until the viewer confirms
                    // the file was written and publishes the edit.
                    let summary = Arc::new(document.summarize(bytes.clone())?);
                    Ok(EditOutcome { bytes, summary })
                });
                respond.send(result).ok();
            }
        }
        drop(edits_guard);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a job, returning the receiver so the caller can keep it alive
    /// (dropping it would mark the job canceled, though ordering ignores that).
    fn make_job(
        priority: Priority,
        sequence: u64,
    ) -> (Job, oneshot::Receiver<anyhow::Result<Vec<TextSpan>>>) {
        let (respond, receiver) = oneshot::channel::<anyhow::Result<Vec<TextSpan>>>();
        let job = Job {
            priority,
            sequence,
            work: Work::PageSpans { index: 0, respond },
        };
        (job, receiver)
    }

    #[test]
    fn priority_queue_pops_foreground_before_background() {
        let mut heap = BinaryHeap::new();
        let mut keep_alive = Vec::new();
        for (priority, sequence) in [
            (Priority::Background, 0),
            (Priority::Background, 1),
            (Priority::Foreground, 2),
        ] {
            let (new_job, receiver) = make_job(priority, sequence);
            keep_alive.push(receiver);
            heap.push(new_job);
        }

        // Foreground first despite being submitted last.
        assert_eq!(heap.pop().unwrap().priority, Priority::Foreground);
        // Then background in submission (FIFO) order.
        let next = heap.pop().unwrap();
        assert_eq!(next.priority, Priority::Background);
        assert_eq!(next.sequence, 0);
        assert_eq!(heap.pop().unwrap().sequence, 1);
    }

    #[test]
    fn equal_priority_is_fifo_by_sequence() {
        let mut heap = BinaryHeap::new();
        let mut keep_alive = Vec::new();
        for sequence in [5, 2, 9] {
            let (new_job, receiver) = make_job(Priority::Foreground, sequence);
            keep_alive.push(receiver);
            heap.push(new_job);
        }
        assert_eq!(heap.pop().unwrap().sequence, 2);
        assert_eq!(heap.pop().unwrap().sequence, 5);
        assert_eq!(heap.pop().unwrap().sequence, 9);
    }
}
