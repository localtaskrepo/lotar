#[cfg(feature = "schema")]
pub(crate) fn custom_value_key(v: &crate::types::CustomFieldValue) -> String {
    match v {
        serde_json::Value::Null => "null".to_string(),
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Array(_) => "[array]".to_string(),
        serde_json::Value::Object(_) => "{object}".to_string(),
    }
}

#[cfg(not(feature = "schema"))]
pub(crate) fn custom_value_key(v: &crate::types::CustomFieldValue) -> String {
    match v {
        serde_yaml::Value::Null => "null".to_string(),
        serde_yaml::Value::Bool(b) => b.to_string(),
        serde_yaml::Value::Number(n) => n.to_string(),
        serde_yaml::Value::String(s) => s.clone(),
        serde_yaml::Value::Sequence(_) => "[array]".to_string(),
        serde_yaml::Value::Mapping(_) => "{object}".to_string(),
        _ => "other".to_string(),
    }
}
