// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Starts a managed worker during construction and shuts it down explicitly.

use std::error::Error;
use std::io;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::time::Duration;

use qubit_ioc::CleanupError;
use qubit_ioc::ContainerBuilder;
use qubit_ioc::Managed;
use qubit_ioc::ShutdownMode;
use qubit_ioc::WaitPolicy;
use tokio::runtime::Builder;
use tokio::spawn;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;
use tokio::time::sleep;

/// Worker task state shared with its explicit shutdown callbacks.
struct Worker {
    /// Set after the task receives the stop signal and exits.
    finished: Arc<AtomicBool>,
    /// One-shot sender consumed by the synchronous stop callback.
    stop: Mutex<Option<oneshot::Sender<()>>>,
    /// Task handle consumed by the asynchronous wait callback.
    task: Mutex<Option<JoinHandle<()>>>,
}

/// Requests stop without waiting; the wait callback observes task completion.
fn request_stop(worker: Arc<Worker>) -> Result<(), CleanupError> {
    let stop = worker
        .stop
        .lock()
        .map_err(|_| CleanupError::new(io::Error::other("stop lock poisoned")))?
        .take();
    if let Some(stop) = stop {
        let _ = stop.send(());
    }
    Ok(())
}

/// Builds the managed worker and observes cleanup on success and failure.
fn main() -> Result<(), Box<dyn Error>> {
    Builder::new_current_thread().enable_time().build()?.block_on(async {
        let mut builder = ContainerBuilder::new().wait_policy(WaitPolicy::bounded(
            Duration::from_secs(30),
            Duration::from_secs(5),
            |duration| Box::pin(sleep(duration)),
        ));
        builder.register_managed_factory::<Worker, _>(&[], |_| {
            let (stop_sender, stop_receiver) = oneshot::channel();
            let finished = Arc::new(AtomicBool::new(false));
            let task_finished = Arc::clone(&finished);
            let task = spawn(async move {
                let _ = stop_receiver.await;
                task_finished.store(true, Ordering::Release);
            });
            let worker = Arc::new(Worker {
                finished,
                stop: Mutex::new(Some(stop_sender)),
                task: Mutex::new(Some(task)),
            });
            Ok(Managed::asynchronous(Arc::clone(&worker), request_stop, |worker| {
                Box::pin(async move {
                    let task = worker
                        .task
                        .lock()
                        .map_err(|_| CleanupError::new(io::Error::other("task lock poisoned")))?
                        .take();
                    if let Some(task) = task {
                        task.await.map_err(CleanupError::new)?;
                    }
                    Ok(())
                })
            })
            .with_graceful_stop(request_stop))
        })?;
        builder.root::<Worker>();
        let application = match builder.build_async().await {
            Ok(application) => application,
            Err(failure) => {
                let (cause, cleanup) = failure.into_parts();
                eprintln!("Application construction failed: {cause}");
                if let Some(mut cleanup) = cleanup
                    && let Err(error) = cleanup.wait().await
                {
                    eprintln!("Application cleanup failed: {error}");
                }
                return Err(cause.into());
            }
        };
        let worker = application.context().get::<Worker>();
        let mode = if worker.is_ok() {
            ShutdownMode::Graceful
        } else {
            ShutdownMode::Immediate
        };
        let mut shutdown = application.begin_shutdown(mode);
        let cleanup = shutdown.wait().await;
        if let Err(error) = &cleanup {
            eprintln!("Application shutdown failed: {error}; {:?}", error.report());
        }
        let worker = worker?;
        cleanup?;
        assert!(worker.finished.load(Ordering::Acquire));
        Ok::<(), Box<dyn Error>>(())
    })?;
    Ok(())
}
