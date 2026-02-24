use std::sync::{Arc, RwLock};
use log::warn;
use super::*;
use napi::bindgen_prelude::*;
use napi_derive::napi;
use pyo3::prelude::*;
use pyo3::types::PyBytes;
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

// Common implementation for both bindings
pub struct SkillsChangeEvent {
    pub skill_id: String,
    pub old_value: f64,
    pub new_value: f64,
}

// Thread-safe storage for listeners
// Using Arc<RwLock<>> instead of static mut or Mutex<Vec<Box<dyn Fn>>>
type Listener = Arc<dyn Fn(&SkillsChangeEvent) + Send + Sync>;
lazy_static::lazy_static! {
    static ref LISTENERS: RwLock<Vec<Listener>> = RwLock::new(Vec::new());
}

// Common emit implementation
fn emit_impl(event: &SkillsChangeEvent) {
    let listeners = match LISTENERS.read() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(), // Handle poisoned lock
    };
    
    for listener in listeners.iter() {
        // Using std::panic::catch_unwind for panic safety
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            listener(event);
        })) {
            Ok(_) => {},
            Err(err) => {
                let err_msg = if let Some(s) = err.downcast_ref::<String>() {
                    s.clone()
                } else if let Some(s) = err.downcast_ref::<&str>() {
                    s.to_string()
                } else {
                    "Unknown error".to_string()
                };
                // Using log::warn instead of eprintln! to match TypeScript
                warn!("skills change listener failed: {}", err_msg);
            }
        }
    }
}

// NAPI bindings
#[cfg(feature = "napi")]
mod napi_bindings {

    #[napi(object)]
    pub struct SkillsChangeEventNapi {
        pub skill_id: String,
        pub old_value: f64,
        pub new_value: f64,
    }

    impl From<&SkillsChangeEventNapi> for SkillsChangeEvent {
        fn from(event: &SkillsChangeEventNapi) -> Self {
            SkillsChangeEvent {
                skill_id: event.skill_id.clone(),
                old_value: event.old_value,
                new_value: event.new_value,
            }
        }
    }

    #[napi]
    pub fn emit(event: &SkillsChangeEventNapi) -> Result<()> {
        let event = SkillsChangeEvent::from(event);
        emit_impl(&event);
        Ok(())
    }

    #[napi]
    pub fn batch_emit(events: Float64Array) -> Result<()> {
        let data = events.as_ref();
        
        // Validate array length
        if data.len() % 3 != 0 {
            return Err(Error::from_reason("Invalid event array length"));
        }
        
        let event_count = data.len() / 3;
        
        for i in 0..event_count {
            let base_idx = i * 3;
            // First element is skill_id encoded as f64
            let skill_id_hash = data[base_idx].to_bits();
            let old_value = data[base_idx + 1];
            let new_value = data[base_idx + 2];
            
            let skill_id = format!("skill_{}", skill_id_hash);
            
            let event = SkillsChangeEvent {
                skill_id,
                old_value,
                new_value,
            };
            
            emit_impl(&event);
        }
        Ok(())
    }
}

// PyO3 bindings
#[cfg(feature = "pyo3")]
mod pyo3_bindings {

    #[pyclass]
    #[derive(Clone)]
    pub struct SkillsChangeEventPy {
        #[pyo3(get, set)]
        pub skill_id: String,
        #[pyo3(get, set)]
        pub old_value: f64,
        #[pyo3(get, set)]
        pub new_value: f64,
    }

    #[pymethods]
    impl SkillsChangeEventPy {
        #[new]
        fn new(skill_id: String, old_value: f64, new_value: f64) -> Self {
            Self {
                skill_id,
                old_value,
                new_value,
            }
        }
    }

    impl From<&SkillsChangeEventPy> for SkillsChangeEvent {
        fn from(event: &SkillsChangeEventPy) -> Self {
            SkillsChangeEvent {
                skill_id: event.skill_id.clone(),
                old_value: event.old_value,
                new_value: event.new_value,
            }
        }
    }

    #[pyfunction]
    pub fn emit(event: &SkillsChangeEventPy) -> PyResult<()> {
        let event = SkillsChangeEvent::from(event);
        emit_impl(&event);
        Ok(())
    }

    #[pyfunction]
    pub fn batch_emit(events: &PyBytes) -> PyResult<()> {
        let data = events.as_bytes();
        let float_size = std::mem::size_of::<f64>();
        
        // Validate data length
        if data.len() % (3 * float_size) != 0 {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "Invalid event data length"
            ));
        }
        
        let event_count = data.len() / (3 * float_size);
        
        for i in 0..event_count {
            let base_idx = i * 3 * float_size;
            
            // Safely extract bytes and convert to f64
            let skill_id_bytes = &data[base_idx..base_idx + float_size];
            let old_value_bytes = &data[base_idx + float_size..base_idx + 2 * float_size];
            let new_value_bytes = &data[base_idx + 2 * float_size..base_idx + 3 * float_size];
            
            // Use f64::from_bits instead of u64 for skill_id to match NAPI version
            let skill_id_bits = u64::from_le_bytes(skill_id_bytes.try_into()
                .map_err(|_| pyo3::exceptions::PyValueError::new_err("Invalid skill_id bytes"))?);
            let skill_id_hash = f64::from_bits(skill_id_bits);
            
            let old_value = f64::from_le_bytes(old_value_bytes.try_into()
                .map_err(|_| pyo3::exceptions::PyValueError::new_err("Invalid old_value bytes"))?);
            let new_value = f64::from_le_bytes(new_value_bytes.try_into()
                .map_err(|_| pyo3::exceptions::PyValueError::new_err("Invalid new_value bytes"))?);
            
            let skill_id = format!("skill_{}", skill_id_hash.to_bits());
            
            let event = SkillsChangeEvent {
                skill_id,
                old_value,
                new_value,
            };
            
            emit_impl(&event);
        }
        Ok(())
    }

    #[pymodule]
    fn shinso_emit(_py: Python, m: &PyModule) -> PyResult<()> {
        m.add_class::<SkillsChangeEventPy>()?;
        m.add_function(wrap_pyfunction!(emit, m)?)?;
        m.add_function(wrap_pyfunction!(batch_emit, m)?)?;
        Ok(())
    }
}

// Helper function to add a listener (implementation would be needed)
pub fn add_listener(listener: Listener) -> Result<(), String> {
    match LISTENERS.write() {
        Ok(mut guard) => {
            guard.push(listener);
            Ok(())
        }
        Err(_) => Err("Failed to acquire write lock".to_string())
    }
}

// NAPI binding

#[napi]
pub struct Cache {
    cache: HashMap<String, u64>,
    ttl_ms: u64,
}

#[napi]
impl Cache {
    #[napi(constructor)]
    pub fn new(ttl_ms: u64) -> Self {
        Cache {
            cache: HashMap::new(),
            ttl_ms,
        }
    }

    #[napi]
    pub fn cleanup(&mut self) -> Result<()> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| Error::new(Status::GenericFailure, e.to_string()))?
            .as_millis() as u64;
        
        self.cache.retain(|_, &mut timestamp| {
            now - timestamp <= self.ttl_ms
        });
        
        Ok(())
    }
}
