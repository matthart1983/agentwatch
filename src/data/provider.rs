//! Provider abstraction — which LLM backend serves a given model.
//!
//! Today every invocation neo writes is `provider: "openrouter"` because
//! that's neo's only configured provider. The plan in PROVIDERS.md adds
//! Anthropic / OpenAI / Ollama / Copilot direct paths; once those land
//! upstream the `provider` field on `InvocationRecord` will start
//! carrying real values and the rest of AgentWatch will surface them.
//!
//! The `provider_for()` heuristic infers a provider from a model id
//! string when nothing else is available (e.g. for team-panel previews).

use ratatui::style::Color;

use crate::theme;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Provider {
    OpenRouter,
    Anthropic,
    OpenAI,
    Ollama,
    Copilot,
    Unknown,
}

impl Provider {
    /// Short two-letter tag for UI badges.
    pub fn badge(&self) -> &'static str {
        match self {
            Provider::OpenRouter => "or",
            Provider::Anthropic => "an",
            Provider::OpenAI => "op",
            Provider::Ollama => "ol",
            Provider::Copilot => "co",
            Provider::Unknown => "·",
        }
    }

    /// Display name in toasts / panels.
    pub fn name(&self) -> &'static str {
        match self {
            Provider::OpenRouter => "openrouter",
            Provider::Anthropic => "anthropic",
            Provider::OpenAI => "openai",
            Provider::Ollama => "ollama",
            Provider::Copilot => "copilot",
            Provider::Unknown => "unknown",
        }
    }

    /// Foreground colour for the badge.
    pub fn color(&self) -> Color {
        match self {
            Provider::OpenRouter => theme::CYAN,
            Provider::Anthropic => theme::MAGENTA,
            Provider::OpenAI => theme::GREEN,
            Provider::Ollama => theme::YELLOW,
            Provider::Copilot => theme::FG,
            Provider::Unknown => theme::DIM,
        }
    }

    /// Environment variable that signals this provider is available.
    pub fn env_key(&self) -> Option<&'static str> {
        match self {
            Provider::OpenRouter => Some("OPENROUTER_API_KEY"),
            Provider::Anthropic => Some("ANTHROPIC_API_KEY"),
            Provider::OpenAI => Some("OPENAI_API_KEY"),
            Provider::Ollama => Some("OLLAMA_HOST"), // optional — see is_configured
            Provider::Copilot => Some("GH_COPILOT_TOKEN"),
            Provider::Unknown => None,
        }
    }

    /// Convert a stored provider string back to the enum. Used when
    /// parsing `InvocationRecord.provider`.
    pub fn from_str(s: &str) -> Provider {
        match s.to_ascii_lowercase().as_str() {
            "openrouter" => Provider::OpenRouter,
            "anthropic" => Provider::Anthropic,
            "openai" => Provider::OpenAI,
            "ollama" => Provider::Ollama,
            "copilot" => Provider::Copilot,
            _ => Provider::Unknown,
        }
    }

    pub fn all() -> [Provider; 5] {
        [
            Provider::OpenRouter,
            Provider::Anthropic,
            Provider::OpenAI,
            Provider::Ollama,
            Provider::Copilot,
        ]
    }
}

/// Best-guess provider for a model id when we don't have an invocation
/// record. Prefix-based — once direct providers land we'll get a more
/// accurate answer from each invocation's `provider` field.
///
/// Note: today every neo call is routed through OpenRouter regardless of
/// the model's natural home. Once `auth.toml` ships in neo, an
/// `anthropic/claude-sonnet-4` call with `ANTHROPIC_API_KEY` set will
/// actually take the direct path. Until then, this heuristic is what the
/// UI shows as a "predicted" provider for the team-panel preview.
pub fn provider_for(model: &str) -> Provider {
    if model.is_empty() || model == "auto" {
        return Provider::OpenRouter;
    }
    let lower = model.to_ascii_lowercase();
    if lower.starts_with("ollama:") || lower.contains(":latest") {
        return Provider::Ollama;
    }
    if lower.ends_with("-copilot") || lower.contains("copilot") {
        return Provider::Copilot;
    }
    // Naked ids without a provider prefix — likely Ollama local
    // (llama3.3:70b, mistral, deepseek-coder:33b).
    if !lower.contains('/') {
        if lower.starts_with("claude-") {
            return Provider::Anthropic;
        }
        if lower.starts_with("gpt-") || lower.starts_with("o3") || lower.starts_with("o1") {
            return Provider::OpenAI;
        }
        return Provider::Ollama;
    }
    if lower.starts_with("anthropic/") {
        return Provider::Anthropic;
    }
    if lower.starts_with("openai/") {
        return Provider::OpenAI;
    }
    // Everything else — google/, meta/, deepseek/, mistral/ — has no
    // direct path of its own that we support, so OpenRouter is the
    // sensible default.
    Provider::OpenRouter
}

/// Is the provider's env-var configured in the current process? `Ollama`
/// is treated as available if either OLLAMA_HOST is set OR localhost
/// reachable — we only check the env var here, the network probe is
/// deferred to the provider impl in neo.
pub fn is_configured(p: Provider) -> bool {
    match p {
        Provider::Ollama => std::env::var("OLLAMA_HOST").is_ok() || cfg!(test),
        _ => p
            .env_key()
            .map(|k| std::env::var(k).is_ok())
            .unwrap_or(false),
    }
}
