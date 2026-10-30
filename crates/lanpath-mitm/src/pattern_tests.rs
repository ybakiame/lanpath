use crate::{Pattern, PatternError, pattern::*};

#[test]
fn lookbehind_rewrites_only_the_selected_query_value() {
    let pattern = Pattern::new(r"(?<=_region=)CN(?=&)").unwrap();
    let input = "https://owned.invalid/?_region=CN&sentinel=CN";
    assert_eq!(
        pattern.replace(input, "AT").unwrap(),
        "https://owned.invalid/?_region=AT&sentinel=CN"
    );
    assert!(
        !pattern
            .is_match("https://owned.invalid/?_region=CN")
            .unwrap()
    );
}

#[test]
fn negative_lookahead_excludes_already_processed_requests() {
    let pattern = Pattern::new(r"^https://owned\.invalid/subtitles\?(?!.*_done=1)").unwrap();
    assert!(
        pattern
            .is_match("https://owned.invalid/subtitles?lang=en")
            .unwrap()
    );
    assert!(
        !pattern
            .is_match("https://owned.invalid/subtitles?lang=en&_done=1")
            .unwrap()
    );
    assert!(
        !pattern
            .is_match("https://other.invalid/subtitles?lang=en")
            .unwrap()
    );
}

#[test]
fn both_engines_preserve_named_numeric_and_literal_dollar_replacements() {
    for source in [r"(?P<first>ab)", r"(?P<first>ab)(?=c)"] {
        let pattern = Pattern::new(source).unwrap();
        assert_eq!(
            pattern
                .replace_all("abc abc", "${first}-$0-$$-$missing")
                .unwrap(),
            "ab-ab-$-c ab-ab-$-c"
        );
        assert_eq!(
            pattern.replace("abc abc", "$1suffix/${1}suffix").unwrap(),
            "/absuffixc abc"
        );
        assert_eq!(
            pattern.replace("abc", "${}/$/${unclosed").unwrap(),
            "/$/${unclosedc"
        );
    }
}

#[test]
fn zero_width_replacement_preserves_unicode_and_iteration_boundaries() {
    assert_eq!(
        Pattern::new(r"(?=.)")
            .unwrap()
            .replace_all("éa", "|")
            .unwrap(),
        "|é|a"
    );
    assert_eq!(
        Pattern::new(r"(?<=.)")
            .unwrap()
            .replace_all("éa", "|")
            .unwrap(),
        "é|a|"
    );
    assert_eq!(
        Pattern::new(r"a*(?=.)")
            .unwrap()
            .replace_all("aé", "|")
            .unwrap(),
        "|é"
    );
}

#[test]
fn source_and_fallback_input_limits_return_errors_not_no_match() {
    assert!(matches!(
        Pattern::new(&"a".repeat(MAX_PATTERN_BYTES + 1)),
        Err(PatternError::SourceLimit)
    ));
    let input = "x".repeat(MAX_FALLBACK_INPUT_BYTES + 1);
    assert_eq!(
        Pattern::new(r"x(?=x)").unwrap().is_match(&input),
        Err(PatternError::InputLimit)
    );
    assert!(!Pattern::new("z").unwrap().is_match(&input).unwrap());
    assert_eq!(
        Pattern::new(r"x(?=x)").unwrap().replace_all(&input, "y"),
        Err(PatternError::InputLimit)
    );
}

#[test]
fn linear_replacements_remain_available_and_fallback_work_is_bounded() {
    let linear = Pattern::new("x").unwrap();
    assert_eq!(
        linear.replace_all(&"x".repeat(16_384), "y").unwrap(),
        "y".repeat(16_384)
    );
    let fallback = Pattern::new(r"x(?=x)").unwrap();
    assert_eq!(
        fallback
            .replace_all(&"x".repeat(MAX_FALLBACK_REPLACEMENTS + 1), "y")
            .unwrap(),
        format!("{}x", "y".repeat(MAX_FALLBACK_REPLACEMENTS))
    );
    assert_eq!(
        fallback.replace_all(&"x".repeat(MAX_FALLBACK_REPLACEMENTS + 2), "y"),
        Err(PatternError::ReplacementLimit)
    );
}

#[test]
fn replacement_growth_and_template_limits_reject_before_returning_partial_output() {
    let pattern = Pattern::new("x").unwrap();
    assert_eq!(
        pattern.replace("x", &"y".repeat(MAX_REPLACEMENT_BYTES + 1)),
        Err(PatternError::ReplacementLimit)
    );
    assert_eq!(
        pattern.replace_all(&"x".repeat(2048), &"y".repeat(MAX_REPLACEMENT_BYTES)),
        Err(PatternError::OutputLimit)
    );
}
