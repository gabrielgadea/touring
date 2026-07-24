//! PyO3 bindings (W7.2 placeholder).

#[cfg(feature = "bind-py")]
use pyo3::prelude::*;

#[cfg(feature = "bind-py")]
use crate::common::Greeting;

#[cfg(feature = "bind-py")]
#[pyfunction]
fn hello(message: String) -> PyResult<String> {
    let g = Greeting::new(message);
    Ok(format!("{} (touring {})", g.message, g.touring_version))
}

#[cfg(feature = "bind-py")]
#[pymodule]
fn touring(_py: Python, m: &PyModule) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(hello, m)?)?;
    Ok(())
}
