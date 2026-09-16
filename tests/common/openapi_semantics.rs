//! Minimal, strict OpenAPI 3.1 schema validator for semantic contract tests.
//!
//! This is deliberately NOT a full JSON Schema implementation. It validates
//! the documented subset against `docs/openapi.json` with 3.1 semantics:
//!
//! - `$ref` composes with sibling keywords (both apply).
//! - `type` accepts the string form and the 3.1 array form (including
//!   `"null"` unions); `properties`/`required`/`additionalProperties` apply
//!   to object instances and `items` to array instances regardless of how
//!   the `type` keyword is spelled.
//! - `enum`, `minimum`, `anyOf` (>= 1 branch), `oneOf` (exactly 1 branch),
//!   and `allOf` (every branch) are enforced.
//! - The 3.0 `nullable` keyword is inert in 3.1; encountering it is an
//!   error rather than a silent pass, so the dialect debt stays visible.
//! - Any *unsupported* validation keyword is an error instead of being
//!   silently ignored; annotations (description, default, examples, title,
//!   format, ...) are ignored by design.
//! - Dangling `$ref` targets surface as validation errors.
//!
//! If this subset ever becomes a maintenance burden, replace it with a
//! standards validator crate rather than growing it by accretion.

use serde_json::Value;

/// Keywords this validator enforces or intentionally treats as annotations.
const SUPPORTED_KEYWORDS: &[&str] = &[
    // validation
    "$ref",
    "type",
    "enum",
    "required",
    "properties",
    "items",
    "additionalProperties",
    "anyOf",
    "oneOf",
    "allOf",
    "minimum",
    // annotations (do not constrain instances)
    "description",
    "default",
    "examples",
    "title",
    "format",
    "$comment",
    "deprecated",
    "externalDocs",
    "readOnly",
    "writeOnly",
    "discriminator",
    "xml",
];

pub struct SpecValidator {
    root: Value,
}

impl SpecValidator {
    /// Load the published spec relative to the crate root.
    pub fn load_published() -> Self {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("docs/openapi.json");
        let raw = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        let root: Value =
            serde_json::from_str(&raw).unwrap_or_else(|e| panic!("parse {}: {e}", path.display()));
        Self { root }
    }

    pub fn root(&self) -> &Value {
        &self.root
    }

    /// Resolve a component schema by name (e.g. `EnvelopeTaskList`).
    pub fn component(&self, name: &str) -> &Value {
        self.root
            .pointer(&format!("/components/schemas/{name}"))
            .unwrap_or_else(|| panic!("component {name} missing from published spec"))
    }

    /// Validate `value` against a named component schema.
    pub fn validate_component(&self, name: &str, value: &Value) -> Result<(), String> {
        self.validate(self.component(name), value)
            .map_err(|e| format!("{name}: {e}"))
    }

    /// Resolve a local JSON pointer reference (`#/a/b`) against the spec.
    fn resolve_ref(&self, reference: &Value) -> Result<Value, String> {
        let Some(pointer) = reference.as_str().and_then(|r| r.strip_prefix('#')) else {
            return Err(format!("only local refs are supported, got {reference}"));
        };
        let decoded = pointer.replace("~1", "/").replace("~0", "~");
        self.root
            .pointer(&decoded)
            .cloned()
            .ok_or_else(|| format!("dangling ref {pointer} in published spec"))
    }

    pub fn validate(&self, schema: &Value, value: &Value) -> Result<(), String> {
        self.validate_at(schema, value, "$")
    }

    fn validate_at(&self, schema: &Value, value: &Value, path: &str) -> Result<(), String> {
        let Some(obj) = schema.as_object() else {
            return Err(format!("{path}: schema must be an object, got {schema}"));
        };

        let unsupported: Vec<&String> = obj
            .keys()
            .filter(|key| !SUPPORTED_KEYWORDS.contains(&key.as_str()))
            .collect();
        if let Some((first, rest)) = unsupported.split_first() {
            let etc = if rest.is_empty() {
                String::new()
            } else {
                format!(" (+{} more)", rest.len())
            };
            return Err(format!(
                "{path}: unsupported validation keyword '{first}'{etc}; extend the validator or fix the spec (3.0 'nullable' is inert in 3.1)"
            ));
        }

        // $ref composes with siblings: resolve, validate against the target,
        // then fall through to the sibling keywords below.
        if let Some(reference) = obj.get("$ref") {
            let resolved = self
                .resolve_ref(reference)
                .map_err(|e| format!("{path}: {e}"))?;
            self.validate_at(&resolved, value, path)?;
        }

        if let Some(all) = obj.get("allOf").and_then(Value::as_array) {
            for sub in all {
                self.validate_at(sub, value, path)?;
            }
        }

        if let Some(expected) = obj.get("type") {
            self.check_type(expected, value, path)?;
        }

        if let Some(allowed) = obj.get("enum").and_then(Value::as_array)
            && !allowed.contains(value)
        {
            return Err(format!(
                "{path}: {value} not in enum [{}]",
                allowed
                    .iter()
                    .map(Value::to_string)
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }

        if let Some(minimum) = obj.get("minimum").and_then(Value::as_f64)
            && value.as_f64().is_some_and(|v| v < minimum)
        {
            return Err(format!("{path}: {value} below minimum {minimum}"));
        }

        if value.is_object() {
            let object = value.as_object().unwrap();
            if let Some(required) = obj.get("required").and_then(Value::as_array) {
                let missing: Vec<&str> = required
                    .iter()
                    .filter_map(Value::as_str)
                    .filter(|field| !object.contains_key(*field))
                    .collect();
                if !missing.is_empty() {
                    return Err(format!(
                        "{path}: missing required field(s) {}",
                        missing.join(", ")
                    ));
                }
            }
            if let Some(properties) = obj.get("properties").and_then(Value::as_object) {
                for (field, sub) in properties {
                    if let Some(member) = object.get(field) {
                        self.validate_at(sub, member, &format!("{path}.{field}"))?;
                    }
                }
            }
            if let Some(additional) = obj.get("additionalProperties") {
                let declared: Vec<&str> = obj
                    .get("properties")
                    .and_then(Value::as_object)
                    .map(|props| props.keys().map(String::as_str).collect())
                    .unwrap_or_default();
                for (field, member) in object {
                    if declared.contains(&field.as_str()) {
                        continue;
                    }
                    match additional {
                        Value::Bool(true) | Value::Null => {}
                        Value::Bool(false) => {
                            return Err(format!("{path}: unexpected field {field}"));
                        }
                        sub => self.validate_at(sub, member, &format!("{path}.{field}"))?,
                    }
                }
            }
        }

        if value.is_array()
            && let Some(items) = obj.get("items")
        {
            for (index, element) in value.as_array().unwrap().iter().enumerate() {
                self.validate_at(items, element, &format!("{path}[{index}]"))?;
            }
        }

        for key in ["anyOf", "oneOf"] {
            let Some(options) = obj.get(key).and_then(Value::as_array) else {
                continue;
            };
            let branches: Vec<Result<(), String>> = options
                .iter()
                .map(|sub| self.validate_at(sub, value, path))
                .collect();
            let hit = branches.iter().filter(|r| r.is_ok()).count();
            let satisfied = if key == "anyOf" { hit >= 1 } else { hit == 1 };
            if !satisfied {
                return Err(format!(
                    "{path}: {key} not satisfied ({hit}/{} branches matched: {})",
                    options.len(),
                    branches
                        .into_iter()
                        .filter_map(|r| r.err())
                        .collect::<Vec<_>>()
                        .join("; ")
                ));
            }
        }

        Ok(())
    }

    fn check_type(&self, expected: &Value, value: &Value, path: &str) -> Result<(), String> {
        let names: Vec<&str> = match expected {
            Value::String(single) => vec![single.as_str()],
            Value::Array(many) => many.iter().filter_map(Value::as_str).collect(),
            other => return Err(format!("{path}: unsupported type declaration {other}")),
        };
        if names.is_empty() {
            return Err(format!("{path}: empty type declaration {expected}"));
        }
        if names
            .iter()
            .any(|allowed| json_type_matches(allowed, value))
        {
            Ok(())
        } else {
            Err(format!(
                "{path}: expected type {}, got {} ({value})",
                names.join("|"),
                json_type_name(value)
            ))
        }
    }
}

fn json_type_name(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(n) if n.is_u64() || n.is_i64() => "integer",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

fn json_type_matches(allowed: &str, value: &Value) -> bool {
    match allowed {
        "null" => value.is_null(),
        "boolean" => value.is_boolean(),
        // JSON booleans are not integers, unlike serde_json's as_u64.
        "integer" => value.is_number() && (value.as_u64().is_some() || value.as_i64().is_some()),
        "number" => value.is_number(),
        "string" => value.is_string(),
        "array" => value.is_array(),
        "object" => value.is_object(),
        other => panic!("unsupported JSON Schema type {other}"),
    }
}
