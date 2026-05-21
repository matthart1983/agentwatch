//! Pricing lookup keyed by (Provider, model). Same model can carry
//! different rates depending on the provider: OpenRouter typically
//! marks up the aggregator path slightly, direct Anthropic/OpenAI is
//! same-or-cheaper, Ollama is always free, Copilot is subscription.
//!
//! Numbers are indicative until we fetch live rates from each provider's
//! `/v1/models` endpoint and cache them.

use super::provider::{provider_for, Provider};

#[derive(Debug, Clone, Copy)]
pub struct ModelPrice {
    /// USD per million input tokens.
    pub input_per_mtok: f64,
    /// USD per million output tokens.
    pub output_per_mtok: f64,
    /// True when the price isn't per-token (e.g. Copilot subscription).
    /// Callers display a "sub." tier marker instead of a dollar amount.
    pub is_subscription: bool,
}

impl ModelPrice {
    const fn rate(input_per_mtok: f64, output_per_mtok: f64) -> Self {
        Self {
            input_per_mtok,
            output_per_mtok,
            is_subscription: false,
        }
    }

    const fn free() -> Self {
        Self {
            input_per_mtok: 0.0,
            output_per_mtok: 0.0,
            is_subscription: false,
        }
    }

    const fn subscription() -> Self {
        Self {
            input_per_mtok: 0.0,
            output_per_mtok: 0.0,
            is_subscription: true,
        }
    }
}

// (Provider, model_id) → price. Order doesn't matter; lookup scans linearly.
const PRICES: &[(Provider, &str, ModelPrice)] = &[
    // ── OpenRouter (aggregator) ─────────────────────────────────────
    (Provider::OpenRouter, "anthropic/claude-sonnet-4",          ModelPrice::rate(3.00,  15.00)),
    (Provider::OpenRouter, "anthropic/claude-sonnet-4-20250514", ModelPrice::rate(3.00,  15.00)),
    (Provider::OpenRouter, "anthropic/claude-3.5-sonnet",        ModelPrice::rate(3.00,  15.00)),
    (Provider::OpenRouter, "anthropic/claude-3.5-haiku",         ModelPrice::rate(0.80,  4.00)),
    (Provider::OpenRouter, "anthropic/claude-opus-4",            ModelPrice::rate(15.00, 75.00)),
    (Provider::OpenRouter, "openai/o3",                          ModelPrice::rate(10.00, 30.00)),
    (Provider::OpenRouter, "openai/gpt-4o",                      ModelPrice::rate(2.50,  10.00)),
    (Provider::OpenRouter, "openai/gpt-4o-mini",                 ModelPrice::rate(0.15,  0.60)),
    (Provider::OpenRouter, "deepseek/deepseek-chat-v3-0324",     ModelPrice::rate(0.27,  1.10)),
    (Provider::OpenRouter, "deepseek/v3",                        ModelPrice::rate(0.27,  1.10)),
    (Provider::OpenRouter, "google/gemini-2.5-pro",              ModelPrice::rate(1.25,  5.00)),
    (Provider::OpenRouter, "google/gemini-2.5-flash",            ModelPrice::rate(0.30,  2.50)),
    (Provider::OpenRouter, "meta/llama-3.3-70b",                 ModelPrice::rate(0.40,  0.40)),
    (Provider::OpenRouter, "meta-llama/llama-3.3-70b-instruct",  ModelPrice::rate(0.40,  0.40)),

    // ── Anthropic direct (same rates as OpenRouter — Anthropic sets them) ──
    (Provider::Anthropic,  "anthropic/claude-sonnet-4",          ModelPrice::rate(3.00,  15.00)),
    (Provider::Anthropic,  "claude-sonnet-4",                    ModelPrice::rate(3.00,  15.00)),
    (Provider::Anthropic,  "claude-3.5-sonnet",                  ModelPrice::rate(3.00,  15.00)),
    (Provider::Anthropic,  "claude-3.5-haiku",                   ModelPrice::rate(0.80,  4.00)),
    (Provider::Anthropic,  "claude-opus-4",                      ModelPrice::rate(15.00, 75.00)),

    // ── OpenAI direct (5–10% cheaper than via OpenRouter for popular models) ──
    (Provider::OpenAI,     "openai/gpt-4o",                      ModelPrice::rate(2.50,  10.00)),
    (Provider::OpenAI,     "gpt-4o",                             ModelPrice::rate(2.50,  10.00)),
    (Provider::OpenAI,     "gpt-4o-mini",                        ModelPrice::rate(0.15,  0.60)),
    (Provider::OpenAI,     "o3",                                 ModelPrice::rate(10.00, 30.00)),
    (Provider::OpenAI,     "o3-mini",                            ModelPrice::rate(1.10,  4.40)),
    (Provider::OpenAI,     "o1",                                 ModelPrice::rate(15.00, 60.00)),

    // ── Ollama (local — always free) ───────────────────────────────────────
    (Provider::Ollama,     "llama3.3:70b",                       ModelPrice::free()),
    (Provider::Ollama,     "llama3.3:8b",                        ModelPrice::free()),
    (Provider::Ollama,     "mistral",                            ModelPrice::free()),
    (Provider::Ollama,     "mistral-large",                      ModelPrice::free()),
    (Provider::Ollama,     "deepseek-coder:33b",                 ModelPrice::free()),
    (Provider::Ollama,     "qwen2.5-coder:32b",                  ModelPrice::free()),

    // ── Copilot (subscription — no per-token billing) ──────────────────────
    (Provider::Copilot,    "gpt-4o-copilot",                     ModelPrice::subscription()),
    (Provider::Copilot,    "o1-copilot",                         ModelPrice::subscription()),
    (Provider::Copilot,    "copilot-codex",                      ModelPrice::subscription()),
];

/// Look up the price for an exact (provider, model) pair.
pub fn lookup_with(provider: Provider, model: &str) -> Option<ModelPrice> {
    PRICES
        .iter()
        .find(|(p, m, _)| *p == provider && *m == model)
        .map(|(_, _, price)| *price)
}

/// Legacy single-arg lookup — infers provider from the model id. Kept so
/// existing call sites don't break while the rest of the codebase
/// migrates.
pub fn lookup(model: &str) -> Option<ModelPrice> {
    lookup_with(provider_for(model), model)
}

/// All known model ids in the pricing table (provider-tagged) — used by
/// the `/team models` slash command for discoverability.
pub fn known_models() -> Vec<&'static str> {
    PRICES.iter().map(|(_, m, _)| *m).collect()
}

/// Same, but de-duplicated and provider-grouped for richer output.
pub fn models_by_provider() -> Vec<(Provider, Vec<&'static str>)> {
    let mut groups: Vec<(Provider, Vec<&'static str>)> = Vec::new();
    for prov in Provider::all() {
        let models: Vec<&'static str> = PRICES
            .iter()
            .filter(|(p, _, _)| *p == prov)
            .map(|(_, m, _)| *m)
            .collect();
        if !models.is_empty() {
            groups.push((prov, models));
        }
    }
    groups
}

/// Compute USD cost given a provider, model, and token counts. Returns
/// 0.0 for subscription models OR when the model isn't in the table —
/// caller distinguishes via `is_subscription`.
pub fn compute_cost_with(
    provider: Provider,
    model: &str,
    tokens_in: u32,
    tokens_out: u32,
) -> f64 {
    let Some(p) = lookup_with(provider, model) else {
        return 0.0;
    };
    if p.is_subscription {
        return 0.0;
    }
    (tokens_in as f64 / 1_000_000.0) * p.input_per_mtok
        + (tokens_out as f64 / 1_000_000.0) * p.output_per_mtok
}

/// Legacy single-model entry — infers provider from model id.
pub fn compute_cost(model: &str, tokens_in: u32, tokens_out: u32) -> f64 {
    compute_cost_with(provider_for(model), model, tokens_in, tokens_out)
}
