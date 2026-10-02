//! Fail-closed JSON Schema subset validator for MCP tool arguments (DEV-63).
//!
//! Every advertised tool `inputSchema` is validated with this module before
//! its handler runs, for both direct method dispatch and `tools/call`. The
//! validator implements exactly the keyword set the tool catalog ships
//! (`type`, `properties`, `required`, `additionalProperties`, `items`,
//! `enum`, `oneOf`, plus numeric `minimum`/`maximum`) and treats annotation
//! keywords (`description`, `title`, `default`, `examples`) as non-validating.
//!
//! Fail-closed contract: any validation keyword outside the supported set
//! (for example `$ref`, `allOf`, `pattern`, `const`) is an error, so a future
//! schema edit can never silently skip enforcement. Global enum *hints*
//! (see `super::hints`) are annotations and are never applied as constraints;
//! project-aware enum validation stays inside the domain handlers.

use serde_json::Value;

/// Annotation keywords that carry no validation semantics.
const ANNOTATION_KEYWORDS: &[&str] = &["description", "title", "default", "examples"];
/// Validation keywords this module enforces.
const SUPPORTED_KEYWORDS: &[&str] = &[
    "type",
    "properties",
    "required",
    "additionalProperties",
    "items",
    "enum",
    "oneOf",
    "minimum",
    "maximum",
];

/// Recursively verify that `schema` only uses keywords this validator
/// enforces (or annotations). Returns every unsupported keyword location.
pub(crate) fn unsupported_keywords(schema: &Value) -> Vec<String> {
    let mut issues = Vec::new();
    check_supported(schema, "", &mut issues);
    issues
}

fn check_supported(schema: &Value, pointer: &str, issues: &mut Vec<String>) {
    let Some(object) = schema.as_object() else {
        return;
    };
    for (key, value) in object {
        let child_pointer = format!("{pointer}/{key}");
        if ANNOTATION_KEYWORDS.contains(&key.as_str()) {
            continue;
        }
        if !SUPPORTED_KEYWORDS.contains(&key.as_str()) {
            issues.push(format!(
                "{pointer}: unsupported schema keyword '{key}' (validator fails closed)"
            ));
            continue;
        }
        match key.as_str() {
            "properties" => {
                if let Some(properties) = value.as_object() {
                    for (name, property) in properties {
                        check_supported(property, &format!("{child_pointer}/{name}"), issues);
                    }
                }
            }
            "additionalProperties" => {
                if value.is_object() {
                    check_supported(value, &child_pointer, issues);
                }
            }
            "items" => {
                check_supported(value, &child_pointer, issues);
            }
            "oneOf" => {
                if let Some(branches) = value.as_array() {
                    for (index, branch) in branches.iter().enumerate() {
                        check_supported(branch, &format!("{child_pointer}/{index}"), issues);
                    }
                }
            }
            _ => {}
        }
    }
}

/// Validate `instance` against `schema`, returning human-readable issues for
/// every violation (empty when valid).
pub(crate) fn validate_instance(schema: &Value, instance: &Value) -> Vec<String> {
    let mut issues = Vec::new();
    validate_value(schema, instance, "", &mut issues);
    issues
}

fn validate_value(schema: &Value, instance: &Value, pointer: &str, issues: &mut Vec<String>) {
    let Some(object) = schema.as_object() else {
        issues.push(format!("{pointer}: schema must be an object"));
        return;
    };

    if let Some(types) = object.get("type")
        && !type_matches(types, instance)
    {
        issues.push(format!(
            "{pointer}: expected type {}, found {}",
            describe_types(types),
            json_type_name(instance)
        ));
    }

    if let Some(enum_value) = object.get("enum") {
        match enum_value.as_array() {
            Some(allowed) if allowed.contains(instance) => {}
            Some(_) => issues.push(format!(
                "{pointer}: value {} is not one of the allowed enum values",
                preview(instance)
            )),
            None => {
                issues.push(format!("{pointer}: schema enum must be an array"));
                return;
            }
        }
    }

    if let Some(minimum) = object.get("minimum").and_then(Value::as_f64)
        && let Some(number) = instance.as_f64()
        && number < minimum
    {
        issues.push(format!(
            "{pointer}: value {number} is below the minimum {minimum}"
        ));
    }
    if let Some(maximum) = object.get("maximum").and_then(Value::as_f64)
        && let Some(number) = instance.as_f64()
        && number > maximum
    {
        issues.push(format!(
            "{pointer}: value {number} is above the maximum {maximum}"
        ));
    }

    if let Some(branches) = object.get("oneOf").and_then(Value::as_array) {
        let matches = branches
            .iter()
            .filter(|branch| validate_instance(branch, instance).is_empty())
            .count();
        if matches != 1 {
            issues.push(format!(
                "{pointer}: value must match exactly one allowed variant (matched {matches})"
            ));
        }
    }

    if let Value::Object(fields) = instance {
        if let Some(properties) = object.get("properties").and_then(Value::as_object) {
            for (name, field) in fields {
                if let Some(property_schema) = properties.get(name) {
                    validate_value(property_schema, field, &format!("{pointer}/{name}"), issues);
                }
            }
        }
        if let Some(required) = object.get("required").and_then(Value::as_array) {
            for name in required {
                if let Some(name) = name.as_str()
                    && !fields.contains_key(name)
                {
                    issues.push(format!("{pointer}: missing required property '{name}'"));
                }
            }
        }
        if let Some(additional) = object.get("additionalProperties") {
            let declared: Vec<&String> = object
                .get("properties")
                .and_then(Value::as_object)
                .map(|properties| properties.keys().collect())
                .unwrap_or_default();
            for (name, field) in fields {
                if declared
                    .iter()
                    .any(|declared| declared.as_str() == name.as_str())
                {
                    continue;
                }
                match additional {
                    Value::Bool(false) => issues.push(format!(
                        "{pointer}: unknown property '{name}' is not allowed"
                    )),
                    Value::Bool(true) => {}
                    schema @ Value::Object(_) => {
                        validate_value(schema, field, &format!("{pointer}/{name}"), issues)
                    }
                    _ => issues.push(format!(
                        "{pointer}: unsupported additionalProperties form for '{name}'"
                    )),
                }
            }
        }
    }

    if let Value::Array(items) = instance
        && let Some(item_schema) = object.get("items")
    {
        for (index, item) in items.iter().enumerate() {
            validate_value(item_schema, item, &format!("{pointer}/{index}"), issues);
        }
    }
}

fn type_matches(types: &Value, instance: &Value) -> bool {
    match types {
        Value::String(name) => single_type_matches(name, instance),
        Value::Array(names) => names
            .iter()
            .filter_map(Value::as_str)
            .any(|name| single_type_matches(name, instance)),
        _ => false,
    }
}

fn single_type_matches(name: &str, instance: &Value) -> bool {
    match name {
        "object" => instance.is_object(),
        "array" => instance.is_array(),
        "string" => instance.is_string(),
        "number" => instance.is_number(),
        "integer" => instance.as_i64().is_some() || instance.as_u64().is_some(),
        "boolean" => instance.is_boolean(),
        "null" => instance.is_null(),
        _ => false,
    }
}

fn describe_types(types: &Value) -> String {
    match types {
        Value::String(name) => name.clone(),
        Value::Array(names) => {
            let rendered: Vec<String> = names
                .iter()
                .filter_map(Value::as_str)
                .map(|name| name.to_string())
                .collect();
            format!("one of [{}]", rendered.join(", "))
        }
        _ => "valid JSON type".to_string(),
    }
}

fn json_type_name(instance: &Value) -> &'static str {
    match instance {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

/// Bound rejected-value previews without splitting a UTF-8 character.
fn preview(instance: &Value) -> String {
    let text = instance.to_string();
    const PREVIEW_LIMIT: usize = 80;
    if text.len() <= PREVIEW_LIMIT {
        return text;
    }
    let mut cut = PREVIEW_LIMIT;
    while !text.is_char_boundary(cut) {
        cut -= 1;
    }
    let mut truncated = text[..cut].to_string();
    truncated.push_str("...");
    truncated
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn enforces_required_type_and_nullability() {
        let schema = json!({
            "type": "object",
            "properties": {
                "id": {"type": "string"},
                "project": {"type": ["string", "null"]}
            },
            "required": ["id"],
            "additionalProperties": false
        });

        assert!(validate_instance(&schema, &json!({"id": "MCP-1"})).is_empty());
        assert!(validate_instance(&schema, &json!({"id": "MCP-1", "project": null})).is_empty());

        let missing = validate_instance(&schema, &json!({"project": "MCP"}));
        assert_eq!(missing.len(), 1);
        assert!(missing[0].contains("missing required property 'id'"));

        let wrong_type = validate_instance(&schema, &json!({"id": 7}));
        assert_eq!(wrong_type.len(), 1);
        assert!(wrong_type[0].contains("expected type string, found number"));

        let null_rejected = validate_instance(&schema, &json!({"id": "MCP-1", "project": 3}));
        assert!(null_rejected[0].contains("/project"));
    }

    #[test]
    fn rejects_unknown_properties_and_validates_nested_values() {
        let schema = json!({
            "type": "object",
            "properties": {
                "values": {"type": "object", "additionalProperties": {"type": "string"}}
            },
            "additionalProperties": false
        });

        assert!(validate_instance(&schema, &json!({"values": {"a": "b"}})).is_empty());

        let unknown = validate_instance(&schema, &json!({"dryrun": true}));
        assert_eq!(unknown.len(), 1);
        assert!(unknown[0].contains("unknown property 'dryrun'"));

        let nested = validate_instance(&schema, &json!({"values": {"a": 5}}));
        assert_eq!(nested.len(), 1);
        assert!(nested[0].contains("/values/a"));
        assert!(nested[0].contains("expected type string, found number"));
    }

    #[test]
    fn enforces_nested_arrays_enums_and_one_of() {
        let schema = json!({
            "type": "object",
            "properties": {
                "tags": {"type": "array", "items": {"type": "string"}},
                "order": {"type": ["string", "null"], "enum": ["asc", "desc", null]},
                "filter": {"oneOf": [
                    {"type": "string"},
                    {"type": "array", "items": {"type": "string"}}
                ]}
            },
            "additionalProperties": false
        });

        let valid = json!({"tags": ["a", "b"], "order": "asc", "filter": ["x"]});
        assert!(validate_instance(&schema, &valid).is_empty());

        let bad_item = validate_instance(&schema, &json!({"tags": ["a", 5]}));
        assert!(bad_item[0].contains("/tags/1"));

        let bad_enum = validate_instance(&schema, &json!({"order": "diagonal"}));
        assert!(bad_enum[0].contains("not one of the allowed enum values"));

        let null_enum_ok = validate_instance(&schema, &json!({"order": null}));
        assert!(null_enum_ok.is_empty());

        let one_of_mismatch = validate_instance(&schema, &json!({"filter": 9}));
        assert!(one_of_mismatch[0].contains("exactly one allowed variant"));

        let one_of_number_in_items = validate_instance(&schema, &json!({"filter": ["x", 4]}));
        assert!(one_of_number_in_items[0].contains("exactly one allowed variant"));
    }

    #[test]
    fn enforces_numeric_bounds() {
        let schema = json!({
            "type": "object",
            "properties": {
                "limit": {"type": ["number", "null"], "minimum": 1, "maximum": 200}
            },
            "additionalProperties": false
        });
        assert!(validate_instance(&schema, &json!({"limit": 50})).is_empty());
        assert!(validate_instance(&schema, &json!({"limit": 0}))[0].contains("below the minimum"));
        assert!(
            validate_instance(&schema, &json!({"limit": 201}))[0].contains("above the maximum")
        );
        assert!(validate_instance(&schema, &json!({"limit": null})).is_empty());
    }

    #[test]
    fn fails_closed_on_unsupported_keywords() {
        let unsupported = json!({"type": "object", "pattern": "^a"});
        let issues = unsupported_keywords(&unsupported);
        assert_eq!(issues.len(), 1);
        assert!(issues[0].contains("unsupported schema keyword 'pattern'"));

        let ref_schema = json!({"$ref": "#/definitions/x"});
        assert!(unsupported_keywords(&ref_schema)[0].contains("unsupported schema keyword '$ref'"));

        let nested = json!({
            "type": "object",
            "properties": {
                "patch": {"type": "object", "properties": {"title": {"type": "string", "const": 1}}}
            }
        });
        let issues = unsupported_keywords(&nested);
        assert!(issues[0].contains("/properties/patch/properties/title"));

        let supported = json!({
            "type": "object",
            "description": "annotated",
            "properties": {
                "tags": {"type": "array", "items": {"type": "string"}, "title": "Tags"},
                "order": {"type": ["string", "null"], "enum": ["asc", "desc", null]}
            },
            "required": ["tags"],
            "additionalProperties": false
        });
        assert!(unsupported_keywords(&supported).is_empty());
    }

    #[test]
    fn instance_must_be_an_object_at_the_root() {
        let schema = json!({"type": "object", "properties": {}, "additionalProperties": false});
        assert!(validate_instance(&schema, &json!({})).is_empty());
        let issues = validate_instance(&schema, &json!([1, 2]));
        assert!(issues[0].contains("expected type object, found array"));
    }

    #[test]
    fn preview_never_panics_on_multibyte_boundaries() {
        // Exactly at the limit: untouched.
        let exact = Value::String("a".repeat(78));
        assert_eq!(exact.to_string().len(), 80);
        assert_eq!(preview(&exact), exact.to_string());

        // Long ASCII: truncated without panicking.
        let long_ascii = Value::String("a".repeat(200));
        let previewed = preview(&long_ascii);
        assert_eq!(previewed, format!("\"{}...", "a".repeat(79)));

        // 81-byte multi-byte value whose byte 80 sits mid-character: the
        // reviewer's repro (task_list due filter of 27x 3-byte chars).
        let multibyte = Value::String("研".repeat(27));
        assert_eq!(multibyte.to_string().len(), 83);
        let previewed = preview(&multibyte);
        assert!(!previewed.is_empty());
        // JSON's opening quote plus 26 full characters fit before byte 80.
        assert_eq!(previewed, format!("\"{}...", "研".repeat(26)));

        // Every prefix length over every offset must be boundary-safe.
        for count in 1..=40 {
            let value = Value::String("✓".repeat(count));
            let _ = preview(&value);
            let mixed = Value::String(format!("{}{}", "ab".repeat(45), "日".repeat(count)));
            let _ = preview(&mixed);
        }
    }

    #[test]
    fn enum_rejection_of_long_multibyte_value_reports_issue_without_panic() {
        // The exact wire shape from the review: task_list {"due": "研"*27}.
        let schema = json!({
            "type": "object",
            "properties": {
                "due": {"type": ["string", "null"], "enum": ["today", "soon", "later", "overdue", null]}
            },
            "additionalProperties": false
        });
        let issues = validate_instance(&schema, &json!({"due": "研".repeat(27)}));
        assert_eq!(issues.len(), 1);
        assert!(issues[0].contains("not one of the allowed enum values"));
        assert!(issues[0].contains("研"));
    }
}
