use pyo3::prelude::*;

/// Loaded as `nautilus_pyo3.shioaji`.
#[pymodule]
pub fn shioaji(_py: Python<'_>, _m: &Bound<'_, PyModule>) -> PyResult<()> {
    // Enums, clients, configs will be registered here as they're built
    Ok(())
}
