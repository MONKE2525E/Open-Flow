use serde::Deserialize;

use super::get_cleanup_prompt_with_extras;

#[derive(Deserialize)]
struct FixtureFile {
    deterministic_contracts: Vec<PromptContract>,
}

#[derive(Deserialize)]
struct PromptContract {
    id: String,
    input: String,
    profile: String,
    intensity: String,
    must_contain: Vec<String>,
    must_not_contain: Vec<String>,
}

#[test]
fn prompt_regression_fixtures_hold() {
    let fixtures: FixtureFile = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/prompt-regressions.json"
    ))
    .expect("prompt regression fixtures must be valid JSON");

    for case in fixtures.deterministic_contracts {
        let prompt = get_cleanup_prompt_with_extras(
            "openai",
            "gpt-4o-mini",
            &case.profile,
            &case.intensity,
            "",
            None,
            &case.input,
            None,
        );
        for required in case.must_contain {
            assert!(
                prompt.contains(&required),
                "fixture {}: prompt lost required contract fragment {:?}",
                case.id,
                required
            );
        }
        for forbidden in case.must_not_contain {
            assert!(
                !prompt.contains(&forbidden),
                "fixture {}: prompt contains forbidden fragment {:?}",
                case.id,
                forbidden
            );
        }
    }
}

