//! Pricing lookup for OpenRouter models so AgentWatch can compute cost
//! from `tokens_in`/`tokens_out` when neo records `cost: 0.0`.
//!
//! Numbers are baseline OpenRouter rates for the models neo's default
//! capability table ships with. Real billed price can drift — this is
//! indicative until we fetch `/api/v1/models` and cache the live rates.

#[derive(Debug, Clone, Copy)]
pub struct ModelPrice {
    /// USD per million input tokens.
    pub input_per_mtok: f64,
    /// USD per million output tokens.
    pub output_per_mtok: f64,
}

const PRICES: &[(&str, ModelPrice)] = &[
    // Anthropic
    ("anthropic/claude-sonnet-4",           ModelPrice { input_per_mtok: 3.00,  output_per_mtok: 15.00 }),
    ("anthropic/claude-sonnet-4-20250514",  ModelPrice { input_per_mtok: 3.00,  output_per_mtok: 15.00 }),
    ("anthropic/claude-3.5-sonnet",         ModelPrice { input_per_mtok: 3.00,  output_per_mtok: 15.00 }),
    ("anthropic/claude-3.5-haiku",          ModelPrice { input_per_mtok: 0.80,  output_per_mtok: 4.00  }),
    ("anthropic/claude-opus-4",             ModelPrice { input_per_mtok: 15.00, output_per_mtok: 75.00 }),

    // OpenAI
    ("openai/o3",                           ModelPrice { input_per_mtok: 10.00, output_per_mtok: 30.00 }),
    ("openai/gpt-4o",                       ModelPrice { input_per_mtok: 2.50,  output_per_mtok: 10.00 }),
    ("openai/gpt-4o-mini",                  ModelPrice { input_per_mtok: 0.15,  output_per_mtok: 0.60  }),

    // DeepSeek
    ("deepseek/deepseek-chat-v3-0324",      ModelPrice { input_per_mtok: 0.27,  output_per_mtok: 1.10  }),
    ("deepseek/v3",                         ModelPrice { input_per_mtok: 0.27,  output_per_mtok: 1.10  }),

    // Google
    ("google/gemini-2.5-pro",               ModelPrice { input_per_mtok: 1.25,  output_per_mtok: 5.00  }),
    ("google/gemini-2.5-flash",             ModelPrice { input_per_mtok: 0.30,  output_per_mtok: 2.50  }),

    // Meta (Llama via OpenRouter — varies by provider, typical free-tier)
    ("meta/llama-3.3-70b",                  ModelPrice { input_per_mtok: 0.40,  output_per_mtok: 0.40  }),
    ("meta-llama/llama-3.3-70b-instruct",   ModelPrice { input_per_mtok: 0.40,  output_per_mtok: 0.40  }),
];

/// Lookup price for an exact model id. Returns `None` if unknown so
/// callers can fall back to whatever neo stored (typically 0).
pub fn lookup(model: &str) -> Option<ModelPrice> {
    PRICES
        .iter()
        .find(|(id, _)| *id == model)
        .map(|(_, p)| *p)
}

/// Compute the USD cost of one invocation. Returns 0.0 if the model
/// isn't in our table — caller decides whether that means "free" or
/// "unknown".
pub fn compute_cost(model: &str, tokens_in: u32, tokens_out: u32) -> f64 {
    let Some(p) = lookup(model) else {
        return 0.0;
    };
    (tokens_in as f64 / 1_000_000.0) * p.input_per_mtok
        + (tokens_out as f64 / 1_000_000.0) * p.output_per_mtok
}
