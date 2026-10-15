/*!
 * SOURCE OF TRUTH KEYWORDS: written forms guard, keeps_written_forms, WrittenFormLost, protect emails urls amounts symbols, model stage rewrote an address
 * WHAT:  `keeps_written_forms(before, after)`: false when `after` holds fewer copies of any written form `before` held:
 *        a word carrying a symbol ("john@gmail.com", "$25.50", "snake_case", "(x)", "#launch") or a domain dot
 *        ("google.com").
 * WHY:   The rule stages write spoken emails, web addresses, amounts and symbols (02 §8.3 stages 3, 4 and 7); a model
 *        stage after them (grammar polish) may spell one back out ("john at gmail.com") or "fix" it ("Google.com",
 *        "$25.5"). Like a dictionary spelling, such a form must survive, so a model answer that loses one is not used
 *        and the chain keeps the rule output. A form is a whitespace-separated word with the sentence punctuation
 *        around it trimmed, so the model may still add a full stop or a comma after it. A domain dot needs two
 *        letters after it, so decimals ("3.5") and abbreviations ("e.g.") are not forms the model has to keep
 *        verbatim. Counting (not just presence) catches a rewrite of one copy among several. Pure, so it is
 *        table-tested alone.
 * WHERE: PolishChain::run_stage (chain.rs), on the output of every stage that runs a model (`caps.needs_model`),
 *        next to the dictionary guard.
 */

use std::collections::HashMap;

/// Characters that make a word a written form.
const SYMBOLS: &[char] = &[
    '@', '/', '\\', '_', '#', '&', '*', '$', '\u{20ac}', '\u{20b9}', '\u{a5}', '(', ')', '[', ']',
    '{', '}',
];

/// Whether `after` keeps every written form `before` held, as often as `before` held it.
pub(super) fn keeps_written_forms(before: &str, after: &str) -> bool {
    let kept = counts(after);
    counts(before)
        .into_iter()
        .all(|(form, count)| kept.get(form).copied().unwrap_or(0) >= count)
}

/// How often each written form appears in `text`.
fn counts(text: &str) -> HashMap<&str, usize> {
    let mut counts = HashMap::new();
    for form in text
        .split_whitespace()
        .map(|word| word.trim_matches(is_sentence_mark))
        .filter(|word| is_written_form(word))
    {
        *counts.entry(form).or_insert(0) += 1;
    }
    counts
}

/// Punctuation a sentence puts around a word ("john@gmail.com." or "“$5,”").
fn is_sentence_mark(c: char) -> bool {
    matches!(
        c,
        '.' | ','
            | ';'
            | ':'
            | '!'
            | '?'
            | '"'
            | '\''
            | '\u{201c}'
            | '\u{201d}'
            | '\u{2018}'
            | '\u{2019}'
    )
}

fn is_written_form(word: &str) -> bool {
    word.contains(SYMBOLS) || has_domain_dot(word)
}

/// A dot between a letter or digit and two letters ("example.com"), never a decimal or "e.g".
fn has_domain_dot(word: &str) -> bool {
    let chars: Vec<char> = word.chars().collect();
    chars.windows(4).any(|window| {
        window[0].is_alphanumeric()
            && window[1] == '.'
            && window[2].is_alphabetic()
            && window[3].is_alphabetic()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_model_answer_must_keep_every_written_form() {
        // (before, after, kept)
        let cases: &[(&str, &str, bool)] = &[
            (
                "Send it to john@gmail.com",
                "Send it to john@gmail.com.",
                true,
            ),
            (
                "Send it to john@gmail.com",
                "Send it to john at gmail.com.",
                false,
            ),
            ("Go to google.com/docs", "Go to Google.com/docs.", false),
            ("It costs $25.50", "It costs $25.5.", false),
            ("It costs $5", "It only costs $5,", true),
            ("Wrap it in (x)", "Wrap it in (x).", true),
            ("use snake_case here", "Use snake case here.", false),
            ("google.com and google.com", "google.com and Google.", false),
            ("it is 3.5 e.g. now", "It is 3.4, e.g., now.", true),
            ("no forms here", "No forms here.", true),
            ("", "Anything.", true),
        ];
        for (before, after, kept) in cases {
            assert_eq!(
                keeps_written_forms(before, after),
                *kept,
                "{before:?} → {after:?}"
            );
        }
    }
}
