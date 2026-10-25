//! Reading each provider's answer, without a network.

use ai_ships::provider::{parse_reply, Family, ModelSpec};
use serde_json::json;

#[test]
fn deepseek_names_the_partner_model() {
  let spec = ModelSpec::parse("deepseek").unwrap();
  assert_eq!(spec.family, Family::Maas);
  assert_eq!(spec.model, "deepseek-ai/deepseek-v4.1-flash-maas");
  assert_eq!(ModelSpec::parse(&spec.name()), Some(spec));
}

/// A chat completions answer as Vertex returns it for a partner model: the
/// tool call's arguments are a JSON string, not an object.
#[test]
fn a_partner_models_tool_call_is_read() {
  let answer = json!({
    "choices": [{
      "finish_reason": "tool_calls",
      "message": {
        "role": "assistant",
        "content": null,
        "tool_calls": [{
          "type": "function",
          "function": {
            "name": "plan_round",
            "arguments": "{\"plan\": \"Close and fire.\", \"ships\": [{\"ship\": \"Kestrel\", \"rationale\": \"Close.\", \"helm\": {\"verb\": \"intercept\", \"target\": \"Red Morrow\", \"band\": \"Long\"}}]}"
          }
        }]
      }
    }],
    "usage": {"prompt_tokens": 8500, "completion_tokens": 300, "prompt_tokens_details": null}
  });
  let reply = parse_reply(Family::Maas, &answer).unwrap();
  assert_eq!(reply.decision.plan, "Close and fire.");
  assert_eq!(reply.decision.ships[0].ship, "Kestrel");
  assert_eq!((reply.input_tokens, reply.cached_tokens, reply.output_tokens), (8500, 0, 300));
  let spec = ModelSpec::parse("deepseek").unwrap();
  let cost = reply.cost(&spec).unwrap();
  assert!((cost - (8500.0 * 0.30 + 300.0 * 1.20) / 1e6).abs() < 1e-12, "{cost}");
}

#[test]
fn an_answer_without_the_tool_call_is_refused() {
  let answer = json!({"choices": [{"message": {"content": "I would close."}}], "usage": {}});
  assert!(parse_reply(Family::Maas, &answer).is_err());
}

#[test]
fn a_prompt_file_is_named_after_the_model() {
  let spec = ModelSpec::parse("deepseek@prompts/deepseek.md").unwrap();
  assert_eq!(spec.model, "deepseek-ai/deepseek-v4.1-flash-maas");
  assert_eq!(ModelSpec::parse("gemini@x.md").unwrap().model, "gemini-3.8-flash");
}

#[test]
fn a_prompt_file_replaces_the_built_in_prompt() {
  let path = std::env::temp_dir().join(format!("callisto-prompt-{}.md", std::process::id()));
  std::fs::write(&path, "You command ships. Lock everything.").unwrap();
  let name = format!("deepseek@{}", path.display());
  assert_eq!(ai_ships::llm::system_for(&name).unwrap(), "You command ships. Lock everything.");
  assert_eq!(ai_ships::llm::system_for("deepseek").unwrap(), ai_ships::prompt::system());
  assert!(ai_ships::llm::system_for("deepseek@/no/such/prompt.md").is_err());
  let _ = std::fs::remove_file(&path);
}

#[test]
fn haiku_names_claude_on_anthropics_own_api() {
  let spec = ModelSpec::parse("haiku").unwrap();
  assert_eq!(spec.family, Family::Anthropic);
  assert_eq!(spec.model, "claude-haiku-5-5");
  assert_eq!(ModelSpec::parse(&spec.name()), Some(spec));
}

/// Anthropic counts tokens read from and written to the cache apart from
/// `input_tokens`; a reply's input is all three, of which the reads were
/// cached.
#[test]
fn a_claude_reply_counts_cached_input_in_its_input() {
  let answer = json!({
    "content": [{"type": "tool_use", "name": "plan_round", "input": {"plan": "Hold.", "ships": []}}],
    "usage": {"input_tokens": 2700, "cache_read_input_tokens": 5800, "cache_creation_input_tokens": 0, "output_tokens": 300}
  });
  let reply = parse_reply(Family::Anthropic, &answer).unwrap();
  assert_eq!(
    (reply.input_tokens, reply.cached_tokens, reply.output_tokens),
    (8500, 5800, 300)
  );
  let cost = reply.cost(&ModelSpec::parse("haiku").unwrap()).unwrap();
  assert!(
    (cost - (2700.0 * 0.10 + 5800.0 * 0.01 + 300.0 * 0.50) / 1e6).abs() < 1e-12,
    "{cost}"
  );
}

#[test]
fn a_thinking_effort_is_part_of_the_name() {
  let spec = ModelSpec::parse("haiku+think").unwrap();
  assert_eq!(
    (spec.model.as_str(), spec.thinking.as_deref()),
    ("claude-haiku-5-5", Some("medium"))
  );
  assert_eq!(ModelSpec::parse("haiku+thinkhigh").unwrap().thinking.as_deref(), Some("high"));
  assert_eq!(ModelSpec::parse(&spec.name()), Some(spec));
  assert_eq!(ModelSpec::parse("haiku").unwrap().thinking, None);
  assert!(ModelSpec::parse("haiku+thinkhard").is_none());
}
