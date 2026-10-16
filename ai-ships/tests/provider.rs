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
