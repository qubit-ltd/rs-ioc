// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
use std::io::Error;
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::time::Duration;

use r#type::bean;
use r#type::ContainerBuilder;
use r#type::Managed;
use r#type::ShutdownMode;
use r#type::WaitPolicy;
use tokio::runtime;
use tokio::time;

static STOPS: AtomicUsize = AtomicUsize::new(0);

#[bean]
fn synchronous() -> Result<Managed<u32>, Error> {
    Ok(Managed::synchronous(Arc::new(7), |_| {
        STOPS.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }))
}

#[bean]
async fn asynchronous() -> Result<Managed<String>, Error> {
    Ok(Managed::synchronous(Arc::new(String::from("async")), |_| {
        STOPS.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }))
}

fn verify_managed_output_path() {
    let mut builder = ContainerBuilder::new().wait_policy(WaitPolicy::bounded(
        Duration::from_secs(30),
        Duration::from_secs(5),
        |duration| Box::pin(time::sleep(duration)),
    ));
    builder.install::<SynchronousBean>().unwrap();
    builder.install::<AsynchronousBean>().unwrap();
    let runtime = runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .unwrap();
    let application = runtime.block_on(builder.build_all_async()).unwrap();
    let context = application.context();
    assert_eq!(*context.get::<u32>().unwrap(), 7);
    assert_eq!(context.get::<String>().unwrap().as_str(), "async");
    let mut shutdown = application.begin_shutdown(ShutdownMode::Immediate);
    runtime.block_on(shutdown.wait()).unwrap();
    assert_eq!(STOPS.load(Ordering::SeqCst), 2);
}


fn main() {
    verify_managed_output_path();
}

#[cfg(test)]
mod tests {
    use super::verify_managed_output_path;

    #[test]
    fn test_managed_factory_paths_build_query_and_shutdown() {
        verify_managed_output_path();
    }
}
