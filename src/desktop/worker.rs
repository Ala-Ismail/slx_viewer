use std::{
    io,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, Sender},
    },
    thread,
};

use eframe::egui::Context;
use slx_viewer::{Limits, load_cancellable, model::Model};

struct Request {
    id: u64,
    path: PathBuf,
    cancelled: Arc<AtomicBool>,
}

type Completion = (u64, Result<Model, String>);

pub struct Loader {
    requests: Sender<Request>,
    results: Receiver<Completion>,
    active: Option<Request>,
    id: u64,
}

impl Loader {
    pub fn new(ctx: Context) -> io::Result<Self> {
        let (requests, queue) = mpsc::channel::<Request>();
        let (completed, results) = mpsc::channel();
        thread::Builder::new()
            .name("slx-loader".into())
            .spawn(move || {
                let limits = Limits::default();
                while let Ok(request) = queue.recv() {
                    if request.cancelled.load(Ordering::Relaxed) {
                        continue;
                    }
                    let result = load_cancellable(&request.path, &limits, &request.cancelled)
                        .map_err(|error| {
                            format!("Could not open {}. {error}", request.path.display())
                        });
                    if !request.cancelled.load(Ordering::Relaxed) {
                        if completed.send((request.id, result)).is_err() {
                            break;
                        }
                        ctx.request_repaint();
                    }
                }
            })?;
        Ok(Self {
            requests,
            results,
            active: None,
            id: 0,
        })
    }

    pub fn open(&mut self, path: PathBuf) -> Result<(), &'static str> {
        self.cancel();
        self.id += 1;
        let cancelled = Arc::new(AtomicBool::new(false));
        self.requests
            .send(Request {
                id: self.id,
                path: path.clone(),
                cancelled: cancelled.clone(),
            })
            .map_err(|_| "The file loader stopped. Restart SLX Viewer and try again.")?;
        self.active = Some(Request {
            id: self.id,
            path,
            cancelled,
        });
        Ok(())
    }

    pub fn pending_path(&self) -> Option<&Path> {
        self.active.as_ref().map(|request| request.path.as_path())
    }

    pub fn cancel(&mut self) {
        if let Some(request) = self.active.take() {
            request.cancelled.store(true, Ordering::Relaxed);
        }
    }

    pub fn poll(&mut self) -> Option<Result<(PathBuf, Model), String>> {
        while let Ok((id, result)) = self.results.try_recv() {
            if self.active.as_ref().is_some_and(|request| request.id == id) {
                let path = self.active.take().expect("matching active request").path;
                return Some(result.map(|model| (path, model)));
            }
        }
        None
    }
}

impl Drop for Loader {
    fn drop(&mut self) {
        self.cancel();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn switching_and_cancelling_discard_queued_completions() {
        let (requests, queued) = mpsc::channel();
        let (completed, results) = mpsc::channel();
        let mut loader = Loader {
            requests,
            results,
            active: None,
            id: 0,
        };
        loader.open("old.slx".into()).unwrap();
        let old = queued.try_recv().unwrap();
        loader.open("new.slx".into()).unwrap();
        let new = queued.try_recv().unwrap();
        assert!(old.cancelled.load(Ordering::Relaxed));
        completed.send((old.id, Err("stale".into()))).unwrap();
        completed.send((new.id, Err("latest".into()))).unwrap();
        assert_eq!(loader.poll().unwrap().unwrap_err(), "latest");
        loader.open("cancelled.slx".into()).unwrap();
        let cancelled = queued.try_recv().unwrap();
        loader.cancel();
        assert!(cancelled.cancelled.load(Ordering::Relaxed));
        completed.send((cancelled.id, Err("stale".into()))).unwrap();
        assert!(loader.poll().is_none());
    }
}
