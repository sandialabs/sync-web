use hex;
use journal_sdk::{Word, JOURNAL, SIZE};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::fs;

const PROFILE_PATH: &str = "tests/profiles/arrival-utilities-v1.json";
const EXCLUSION_PROBE_PATH: &str = "tests/profiles/arrival-utilities-v1-exclusions.scm";
const EXCLUSION_EXPECTED_PATH: &str = "tests/profiles/arrival-utilities-v1-exclusions.expected.scm";
const C_ORACLE_PATH: &str = "external/s7/s7.c";
const EVALUATOR_PATH: &str = "lisp/evaluator.scm";
const UTILITIES_PATH: &str = "lisp/utils.scm";

#[derive(Deserialize)]
struct Profile {
    schema: String,
    profile: String,
    c_oracle_source_sha256: String,
    evaluator_source_sha256: String,
    utilities_source_sha256: String,
    c_exclusion_probe_sha256: String,
    c_exclusion_expected_sha256: String,
    upstream_keep_count: usize,
    retained_binding_count: usize,
    excluded_capabilities: Vec<ExcludedCapability>,
    excluded: Vec<Exclusion>,
}

#[derive(Deserialize)]
struct ExcludedCapability {
    name: String,
    classification: String,
    reason: String,
}

#[derive(Deserialize)]
struct Exclusion {
    name: String,
    c_arity: String,
    c_signature: String,
    classification: String,
    reason: String,
}

fn sha256(text: &str) -> String {
    format!("{:x}", Sha256::digest(text.as_bytes()))
}

fn keep_list_range(source: &str) -> (usize, usize) {
    let binding = source.find("(keep-list").expect("evaluator keep-list binding");
    let start = binding
        + source[binding..]
            .find("'(")
            .expect("quoted evaluator keep-list")
        + 2;
    let end = start
        + source[start..]
            .find(")))\n\t    (sync-map-set")
            .expect("end of evaluator keep-list");
    (start, end)
}

fn setup(fill: u8) -> impl Fn(&str, &str) {
    let record: Word = [fill; SIZE];
    let record = hex::encode(record);
    assert_eq!(
        JOURNAL.evaluate(&format!(
            "(sync-create (hex-string->byte-vector \"{record}\"))"
        )),
        "#t"
    );
    move |expression, expected| {
        let result = JOURNAL.evaluate(&format!(
            "(sync-call '{expression} #t (hex-string->byte-vector \"{record}\"))"
        ));
        assert_eq!(result, expected, "scoped Utilities expression: {expression}");
    }
}

#[test]
fn deterministic_authority_safe_utilities_profile_v1() {
    let profile_text = fs::read_to_string(PROFILE_PATH).expect("read Utilities profile");
    let profile: Profile = serde_json::from_str(&profile_text).expect("parse Utilities profile");
    assert_eq!(profile.schema, "sync-web/arrival-utilities-profile/v1");
    assert_eq!(profile.profile, "deterministic-authority-safe-v1");

    let c_oracle_source = fs::read_to_string(C_ORACLE_PATH).expect("read frozen C oracle");
    assert_eq!(sha256(&c_oracle_source), profile.c_oracle_source_sha256);
    let evaluator_source = fs::read_to_string(EVALUATOR_PATH).expect("read archived evaluator");
    let utilities_source = fs::read_to_string(UTILITIES_PATH).expect("read archived utilities");
    assert_eq!(sha256(&evaluator_source), profile.evaluator_source_sha256);
    assert_eq!(sha256(&utilities_source), profile.utilities_source_sha256);
    let exclusion_probe = fs::read_to_string(EXCLUSION_PROBE_PATH).expect("read C exclusion probe");
    let exclusion_expected = fs::read_to_string(EXCLUSION_EXPECTED_PATH).expect("read C exclusion snapshot");
    assert_eq!(sha256(&exclusion_probe), profile.c_exclusion_probe_sha256);
    assert_eq!(sha256(&exclusion_expected), profile.c_exclusion_expected_sha256);

    let (start, end) = keep_list_range(&evaluator_source);
    let upstream: Vec<&str> = evaluator_source[start..end].split_whitespace().collect();
    assert_eq!(upstream.len(), profile.upstream_keep_count);
    assert_eq!(profile.excluded.len(), 52);
    assert_eq!(profile.retained_binding_count + profile.excluded.len(), upstream.len());

    let upstream_names: HashSet<&str> = upstream.iter().copied().collect();
    let mut excluded_names = HashSet::new();
    let expected_probe = format!(
        "(list {})\n",
        profile.excluded.iter().map(|x| format!("(list '{} (arity {}) (signature {}))", x.name, x.name, x.name)).collect::<Vec<_>>().join(" ")
    );
    assert_eq!(exclusion_probe, expected_probe);
    for exclusion in &profile.excluded {
        assert!(upstream_names.contains(exclusion.name.as_str()), "unknown excluded root {}", exclusion.name);
        assert!(exclusion_expected.contains(&format!("({} (", exclusion.name)), "C snapshot missing {}", exclusion.name);
        assert!(excluded_names.insert(exclusion.name.as_str()), "duplicate excluded root {}", exclusion.name);
        assert!(!exclusion.c_arity.is_empty());
        assert!(!exclusion.c_signature.is_empty());
        assert!(!exclusion.reason.is_empty());
    }
    assert_eq!(profile.excluded.iter().filter(|x| x.classification == "forbidden-nondeterminism").count(), 3);
    assert_eq!(profile.excluded.iter().filter(|x| x.classification == "forbidden-authority").count(), 1);
    assert_eq!(profile.excluded.iter().filter(|x| x.classification == "deterministic-kernel-unavailable").count(), 3);
    for required in ["asinh", "random-state", "random-state->list", "random-state?", "symbol-table"] {
        assert!(excluded_names.contains(required), "required policy exclusion {required}");
    }

    let included: Vec<&str> = upstream
        .iter()
        .copied()
        .filter(|name| !excluded_names.contains(name))
        .collect();
    assert_eq!(included.len(), profile.retained_binding_count);

    assert_eq!(profile.excluded_capabilities.len(), 1);
    let capability = &profile.excluded_capabilities[0];
    assert_eq!(capability.name, "metacircular-utilities-composition");
    assert_eq!(capability.classification, "unsupported-runtime-semantics");
    assert!(capability.reason.contains("<list*>"));

    let assert = setup(0x5a);
    assert("(list (defined? 'random) random (procedure? random))", "(#t *removed* #f)");
    if std::env::var("SYNC_WEB_EVALUATOR").as_deref() == Ok("unified") {
        let inventory = format!(
            "(let loop ((xs '({})) (missing '())) (if (null? xs) (reverse missing) (loop (cdr xs) (if (defined? (car xs)) missing (cons (car xs) missing)))))",
            upstream.join(" ")
        );
        let expected_missing: Vec<&str> = upstream
            .iter()
            .copied()
            .filter(|name| excluded_names.contains(name))
            .collect();
        assert(&inventory, &format!("({})", expected_missing.join(" ")));
    }

    let mut scoped_evaluator = String::with_capacity(evaluator_source.len());
    scoped_evaluator.push_str(&evaluator_source[..start]);
    scoped_evaluator.push_str(&included.join(" "));
    scoped_evaluator.push_str(&evaluator_source[end..]);

    assert(&scoped_evaluator, "\"Installed metacircular interface\"");

    // The profile explicitly does not claim the archived composition of
    // utils.scm through the metacircular evaluator. Validate the unchanged
    // Utilities source and expected behavior on a separate fresh record.
    let assert_utilities = setup(0x5b);
    assert_utilities(&utilities_source, "\"Installed metacircular utilities\"");
    assert_utilities(
        &format!("(begin {utilities_source} (filter positive? '(1 -1 0)))"),
        "(1)",
    );
    assert_utilities(
        &format!("(begin {utilities_source} (reduce (lambda (x y) (+ x y)) 0 '(1 2 3 4)))"),
        "10",
    );

    assert_eq!(sha256(&fs::read_to_string(EVALUATOR_PATH).unwrap()), profile.evaluator_source_sha256);
    assert_eq!(sha256(&fs::read_to_string(UTILITIES_PATH).unwrap()), profile.utilities_source_sha256);
    println!(
        "arrival-utilities-profile: upstream={} retained-bindings={} excluded={}",
        upstream.len(), included.len(), profile.excluded.len()
    );
}
