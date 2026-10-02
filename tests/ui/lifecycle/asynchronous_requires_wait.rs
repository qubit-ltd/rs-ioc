use std::sync::Arc;
use qubit_ioc::Managed;

fn main() {
    let _resource = Managed::asynchronous(Arc::new(1_u8), |_| Ok(()));
}
