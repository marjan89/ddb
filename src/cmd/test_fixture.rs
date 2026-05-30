pub fn load_fixtures_map() -> std::collections::HashMap<String, String> {
    let mut map = std::collections::HashMap::new();
    let Some(fp) = std::env::var("DDB_FIXTURES_PATH").ok()
        .filter(|p| !p.is_empty() && std::path::Path::new(p).exists())
    else { return map };
    if let Ok(content) = std::fs::read_to_string(&fp) {
        if let Ok(val) = serde_yaml::from_str::<serde_json::Value>(&content) {
            flatten_fixtures("fixtures", &val, &mut map);
        }
    }
    map
}

pub fn flatten_fixtures(prefix: &str, val: &serde_json::Value, map: &mut std::collections::HashMap<String, String>) {
    match val {
        serde_json::Value::Object(obj) => {
            for (k, v) in obj {
                flatten_fixtures(&format!("{prefix}.{k}"), v, map);
            }
        }
        serde_json::Value::String(s) => { map.insert(format!("{{{{{prefix}}}}}"), s.clone()); }
        serde_json::Value::Number(n) => { map.insert(format!("{{{{{prefix}}}}}"), n.to_string()); }
        serde_json::Value::Bool(b) => { map.insert(format!("{{{{{prefix}}}}}"), b.to_string()); }
        _ => {}
    }
}

pub fn interpolate_raw(content: &str, map: &std::collections::HashMap<String, String>) -> String {
    let mut result = content.to_string();
    for (pattern, replacement) in map {
        let quoted = format!("\"{}\"", pattern);
        if replacement.parse::<i64>().is_ok() {
            result = result.replace(&quoted, replacement);
        }
        result = result.replace(pattern, replacement);
    }
    result
}

pub struct FixtureResolver {
    file_fixtures: std::collections::HashMap<String, String>,
    api_responses: std::collections::HashMap<String, serde_json::Value>,
}

impl FixtureResolver {
    pub fn new(file_fixtures: std::collections::HashMap<String, String>) -> Self {
        Self { file_fixtures, api_responses: std::collections::HashMap::new() }
    }

    pub fn add_api_response(&mut self, key: &str, val: serde_json::Value) {
        self.api_responses.insert(key.to_string(), val);
    }

    pub fn get_var(&self, key: &str) -> Option<&serde_json::Value> {
        self.api_responses.get(key)
    }

    pub fn file_fixtures(&self) -> &std::collections::HashMap<String, String> {
        &self.file_fixtures
    }

    pub fn resolve(&self, template: &str) -> String {
        let mut result = template.to_string();
        // Built-in dynamic values
        if result.contains("{{timestamp}}") {
            let ts = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis();
            result = result.replace("{{timestamp}}", &ts.to_string());
        }
        // API responses take precedence (highest layer)
        for (key, val) in &self.api_responses {
            Self::apply_patterns(&mut result, key, val);
        }
        // File fixtures (lowest layer — only for patterns API didn't match)
        for (pattern, replacement) in &self.file_fixtures {
            let quoted = format!("\"{}\"", pattern);
            if replacement.parse::<i64>().is_ok() {
                result = result.replace(&quoted, replacement);
            }
            result = result.replace(pattern, replacement);
        }
        result
    }

    fn apply_patterns(result: &mut String, prefix: &str, val: &serde_json::Value) {
        match val {
            serde_json::Value::Object(map) => {
                for (k, v) in map { Self::apply_patterns(result, &format!("{prefix}.{k}"), v); }
            }
            serde_json::Value::String(s) => { *result = result.replace(&format!("{{{{{prefix}}}}}"), s); }
            serde_json::Value::Number(n) => { *result = result.replace(&format!("{{{{{prefix}}}}}"), &n.to_string()); }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolver_file_fixtures() {
        let mut map = std::collections::HashMap::new();
        map.insert("{{fixtures.user.name}}".to_string(), "Oscar".to_string());
        let resolver = FixtureResolver::new(map);
        assert_eq!(resolver.resolve("hello {{fixtures.user.name}}"), "hello Oscar");
    }

    #[test]
    fn test_resolver_api_overrides_file() {
        let mut map = std::collections::HashMap::new();
        map.insert("{{api_result.token}}".to_string(), "file_token".to_string());
        let mut resolver = FixtureResolver::new(map);
        resolver.add_api_response("api_result", serde_json::json!({"token": "api_token"}));
        let result = resolver.resolve("auth: {{api_result.token}}");
        assert_eq!(result, "auth: api_token");
    }

    #[test]
    fn test_resolver_missing_key_passthrough() {
        let resolver = FixtureResolver::new(std::collections::HashMap::new());
        assert_eq!(resolver.resolve("{{unknown.key}}"), "{{unknown.key}}");
    }
}
