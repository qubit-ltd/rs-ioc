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

use qubit_ioc::CleanupError;
use qubit_ioc::ContainerBuilder;
use qubit_ioc::Managed;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;

/// Worker task state shared with its explicit shutdown callbacks.
struct Worker {
    /// Set after the task receives the stop signal and exits.
    finished: Arc<AtomicBool>,
    /// One-shot sender consumed by the synchronous stop callback.
    stop: Mutex<Option<oneshot::Sender<()>>>,
    /// Task handle consumed by the asynchronous wait callback.
    task: Mutex<Option<JoinHandle<()>>>,
}

fn main() -> Result<(), Box<dyn Error>> {
    tokio::runtime::Builder::new_current_thread().build()?.block_on(async {
        let mut builder = ContainerBuilder::new();
        builder.register_managed_factory::<Worker, _>(&[], |_| {
            let (stop_sender, stop_receiver) = oneshot::channel();
            let finished = Arc::new(AtomicBool::new(false));
            let task_finished = Arc::clone(&finished);
            let task = tokio::spawn(async move {
                let _ = stop_receiver.await;
                task_finished.store(true, Ordering::Release);
            });
            let worker = Arc::new(Worker {
                finished,
                stop: Mutex::new(Some(stop_sender)),
                task: Mutex::new(Some(task)),
            });
            Ok(Managed::new(Arc::clone(&worker), |worker| {
                let stop = worker
                    .stop
                    .lock()
                    .map_err(|_| CleanupError::new(io::Error::other("stop lock poisoned")))?
                    .take();
                if let Some(stop) = stop {
                    let _ = stop.send(());
                }
                Ok(())
            })
            .with_wait(|worker| {
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
            }))
        })?;
        builder.root::<Worker>();
        let context = builder.build()?;
        let worker = context.get::<Worker>()?;
        let mut shutdown = context.begin_shutdown();
        shutdown.wait().await?;
        assert!(worker.finished.load(Ordering::Acquire));
        Ok::<(), Box<dyn Error>>(())
    })?;
    Ok(())
}
