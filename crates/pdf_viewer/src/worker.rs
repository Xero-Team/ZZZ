use std::{
    cmp::Ordering,
    collections::BinaryHeap,
    path::PathBuf,
    sync::{Arc, Condvar, Mutex},
    thread,
};

use futures::channel::oneshot;

use crate::document::{LoadedPdfDocument, PagePreview, PdfDocumentSummary};

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
    PageText {
        index: usize,
        respond: oneshot::Sender<anyhow::Result<String>>,
    },
}

impl Work {
    /// Whether the awaiting receiver has been dropped. Such jobs are skipped at
    /// dequeue time so superseded background prefetch is discarded cheaply
    /// instead of occupying a worker thread.
    fn is_canceled(&self) -> bool {
        match self {
            Work::Render { respond, .. } => respond.is_canceled(),
            Work::PageText { respond, .. } => respond.is_canceled(),
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

struct Shared {
    queue: Mutex<Queue>,
    available: Condvar,
}

/// Owns a pool of dedicated OS threads, each holding its own
/// [`LoadedPdfDocument`].
///
/// `zpdf::PdfDocument` is neither `Send` nor `Sync` (it caches decoded objects
/// in `RefCell`/`OnceCell`), so it cannot be moved between threads or onto
/// GPUI's background executor. Instead each pool thread opens the document from
/// the same shared `Arc<[u8]>` — the raw bytes are shared, only the per-thread
/// parse caches are duplicated — and pulls jobs from a shared priority queue.
/// Every value that crosses the channel boundary (`PagePreview`, `String`,
/// `PdfDocumentSummary`) is `Send`.
pub struct PdfWorker {
    shared: Arc<Shared>,
    summary: Arc<PdfDocumentSummary>,
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
            .map(|count| count.get().saturating_sub(1))
            .unwrap_or(1)
            .clamp(1, MAX_WORKER_THREADS);

        let shared = Arc::new(Shared {
            queue: Mutex::new(Queue::default()),
            available: Condvar::new(),
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
            summary,
            next_sequence: Mutex::new(0),
        })
    }

    pub fn summary(&self) -> &Arc<PdfDocumentSummary> {
        &self.summary
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

    /// Queue text extraction for a page, used by search.
    pub fn page_text(
        &self,
        index: usize,
        priority: Priority,
    ) -> oneshot::Receiver<anyhow::Result<String>> {
        let (respond, receiver) = oneshot::channel();
        self.submit(priority, Work::PageText { index, respond });
        receiver
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

fn worker_loop(document: LoadedPdfDocument, shared: Arc<Shared>) {
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

        match job.work {
            Work::Render {
                index,
                dpi,
                respond,
            } => {
                respond.send(document.render_page_preview(index, dpi)).ok();
            }
            Work::PageText { index, respond } => {
                respond.send(document.page_text(index)).ok();
            }
        }
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
    ) -> (Job, oneshot::Receiver<anyhow::Result<String>>) {
        let (respond, receiver) = oneshot::channel::<anyhow::Result<String>>();
        let job = Job {
            priority,
            sequence,
            work: Work::PageText { index: 0, respond },
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
