use crate::{RenderProfile, RenderRequest};
use regex::Regex;
use std::sync::OnceLock;

#[derive(Debug, Clone, Copy)]
pub(crate) struct ThemePolicy {
    sanitize_temp_objects: bool,
    profile: RenderProfile,
}

impl ThemePolicy {
    pub(crate) fn for_request(request: &RenderRequest) -> Self {
        Self {
            sanitize_temp_objects: !matches!(request.profile, RenderProfile::RawFallback),
            profile: request.profile,
        }
    }

    pub(crate) fn inline(&self, text: &str) -> String {
        sanitize_display_line(text, self.sanitize_temp_objects)
    }

    pub(crate) fn raw(&self, raw_message: &str) -> String {
        let line = raw_message
            .lines()
            .next()
            .unwrap_or(raw_message)
            .to_string();
        sanitize_display_line(&line, !matches!(self.profile, RenderProfile::RawFallback))
    }
}

pub(crate) fn sanitize_display_line(text: &str, sanitize_temp_objects: bool) -> String {
    let sanitized = if sanitize_temp_objects {
        sanitize_transient_object_paths(text)
    } else {
        text.to_string()
    };
    escape_control_chars(&sanitized)
}

fn sanitize_transient_object_paths(text: &str) -> String {
    transient_object_path_pattern()
        .replace_all(text, "<temp-object>")
        .into_owned()
}

fn escape_control_chars(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for character in text.chars() {
        if character.is_control() && character != '\n' && character != '\t' {
            escaped.push_str(&format!("\\x{:02x}", character as u32));
        } else {
            escaped.push(character);
        }
    }
    escaped
}

fn transient_object_path_pattern() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| {
        Regex::new(r#"(?:(?:/private)?/tmp|/var/folders/[^:\s]+/T)/cc[^:\s'"`]+\.o"#)
            .expect("valid transient object path regex")
    })
}
