// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Adam Hooper

use nvml_wrapper::{
    enums::device::SampleValue, struct_wrappers::device::FieldValueSample, structs::device::FieldId,
};

use super::NvmlContext;

#[derive(Debug)]
pub enum FieldQueryStatus {
    /// The whole API call succeeded and this field returned a success status.
    Ok,
    /// The whole API call failed before returning per-field results.
    CallError(String),
    /// The call succeeded but this individual field returned a non-success status.
    FieldError(String),
}

#[derive(Debug)]
pub struct NvmlFieldValue {
    pub field_id: u32,
    pub query: FieldQueryStatus,
    pub value_type: String,
    pub value: String,
}

pub fn query_field_values_for_gpu(
    ctx: &NvmlContext,
    gpu_index: u32,
    field_ids: &[u32],
) -> Vec<NvmlFieldValue> {
    let dev = match ctx.nvml.device_by_index(gpu_index) {
        Ok(dev) => dev,
        Err(e) => return call_error_values(field_ids, e.to_string()),
    };

    let wrapped_ids: Vec<FieldId> = field_ids.iter().copied().map(FieldId).collect();
    let samples = match dev.field_values_for(&wrapped_ids) {
        Ok(samples) => samples,
        Err(e) => return call_error_values(field_ids, e.to_string()),
    };

    field_ids
        .iter()
        .copied()
        .enumerate()
        .map(|(idx, requested_id)| match samples.get(idx) {
            Some(Ok(sample)) => field_value_from_sample(requested_id, sample),
            Some(Err(e)) => field_error_value(requested_id, e.to_string()),
            None => field_error_value(requested_id, "missing field result".to_string()),
        })
        .collect()
}

fn call_error_values(field_ids: &[u32], error: String) -> Vec<NvmlFieldValue> {
    field_ids
        .iter()
        .copied()
        .map(|field_id| NvmlFieldValue {
            field_id,
            query: FieldQueryStatus::CallError(error.clone()),
            value_type: "unavailable".to_string(),
            value: "unavailable".to_string(),
        })
        .collect()
}

fn field_error_value(field_id: u32, error: String) -> NvmlFieldValue {
    NvmlFieldValue {
        field_id,
        query: FieldQueryStatus::FieldError(error),
        value_type: "unavailable".to_string(),
        value: "unavailable".to_string(),
    }
}

fn field_value_from_sample(field_id: u32, sample: &FieldValueSample) -> NvmlFieldValue {
    match &sample.value {
        Ok(value) => {
            let (value_type, value) = sample_value_strings(value);
            NvmlFieldValue {
                field_id,
                query: FieldQueryStatus::Ok,
                value_type,
                value,
            }
        }
        Err(e) => field_error_value(field_id, e.to_string()),
    }
}

fn sample_value_strings(value: &SampleValue) -> (String, String) {
    match value {
        SampleValue::F64(value) => ("f64".to_string(), value.to_string()),
        SampleValue::U32(value) => ("u32".to_string(), value.to_string()),
        SampleValue::U64(value) => ("u64".to_string(), value.to_string()),
        SampleValue::I64(value) => ("i64".to_string(), value.to_string()),
    }
}
