/*!
 * SOURCE OF TRUTH KEYWORDS: RulePolisher tests, rule chain table tests, cloud code dictionary test, filler test, numbers to digits test, casing skip test
 * WHAT:  Table-driven tests of the whole rule polisher: every stage together, with the context switches (fillers
 *        off, engine casing, a named or detected language) and the TextPolisher contract.
 * WHY:   Each stage file tests its own rules; these prove the fixed order composes (a filler removed before spacing
 *        folds its comma, a stutter collapsed before casing, a doubled number word kept by repeats for the numbers
 *        stage) and that the context flags reach the right stage.
 * WHERE: `cargo test` (adapters::polish::rules::tests).
 */

use std::{sync::Arc, task::Poll};

use super::*;
use crate::{
    ports::fakes::poll_once,
    types::{Language, StaticList, StaticStr},
};

fn pairs(list: &[(&str, &str)]) -> StaticList<TextPair> {
    StaticList::from(
        list.iter()
            .map(|(from, to)| TextPair {
                from: StaticStr::from((*from).to_owned()),
                to: StaticStr::from((*to).to_owned()),
            })
            .collect::<Vec<_>>(),
    )
}

/// An engine that punctuates; `cased` says whether it also cases.
fn context(cased: bool) -> PolishContext {
    PolishContext {
        language: None,
        punctuated: true,
        cased,
        remove_fillers: true,
        format_lists: true,
        spoken_symbols: true,
        dictionary: pairs(&[("cloud code", "Claude Code")]),
    }
}

#[test]
fn every_stage_runs_in_order() {
    let polisher = RulePolisher::new();
    let uncased = context(false);
    let cased = context(true);
    let cases: &[(&PolishContext, &str, &str)] = &[
        (&uncased, "hello. world", "Hello. World"),
        (&uncased, "i think we should go", "I think we should go"),
        (&uncased, "the the cat sat", "The cat sat"),
        (&uncased, "um, so we start", "So we start"),
        (&cased, "Um, so we start.", "So we start."),
        (
            &uncased,
            "i use cloud code every day",
            "I use Claude Code every day",
        ),
        (&cased, "I use Cloud Code.", "I use Claude Code."),
        (&cased, "We should go, um.", "We should go."),
        (&cased, "You know, it works, you know.", "It works."),
        (
            &cased,
            "I- I think the the answer is, uh, forty two.",
            "I think the answer is 42.",
        ),
        (&cased, "Twenty, fifty.", "20, 50."),
        (
            &cased,
            "Um, it was built in fifteen twenty three, not one thousand five hundred.",
            "It was built in 1523, not 1500.",
        ),
        (
            &uncased,
            "i- i have twenty twenty five tickets at ten thirty",
            "I have 2025 tickets at 10:30",
        ),
        (
            &uncased,
            "twenty people came. five left",
            "20 people came. Five left",
        ),
        (
            &cased,
            "One of them had five apples.",
            "One of them had five apples.",
        ),
        (&cased, "Hello.  World , again", "Hello. World, again"),
        (&cased, "hello. world", "hello. world"),
        (&cased, "Um.", ""),
        (&cased, "", ""),
        (&uncased, "b-but i- i said so", "But I said so"),
    ];
    for (context, input, expected) in cases {
        assert_eq!(polisher.apply(input, context), *expected, "{input}");
    }
}

#[test]
fn fillers_stay_when_the_setting_is_off() {
    let polisher = RulePolisher::new();
    let context = PolishContext {
        remove_fillers: false,
        ..context(true)
    };
    assert_eq!(
        polisher.apply("Um, so we start.", &context),
        "Um, so we start."
    );
    assert_eq!(
        polisher.apply("the the end", &context),
        "the end",
        "repeats do not depend on the filler setting"
    );
}

#[test]
fn spoken_lists_compose_with_the_other_stages() {
    let polisher = RulePolisher::new();
    let cases: &[(&PolishContext, &str, &str)] = &[
        (
            &context(true),
            "The list of grocery items are as follows. First apple, second oranges. Third mangoes, Fourth watermelon.",
            "The list of grocery items are as follows\n1. apple\n2. oranges\n3. mangoes\n4. watermelon",
        ),
        (
            &context(true),
            "Um, the plan is. First, uh, twenty five chairs, second the the tables. Then we eat.",
            "The plan is\n1. 25 chairs\n2. the tables\nThen we eat.",
        ),
        (
            &context(false),
            "we need. first milk, second bread",
            "We need\n1. Milk\n2. Bread",
        ),
    ];
    for (context, input, expected) in cases {
        assert_eq!(polisher.apply(input, context), *expected, "{input}");
    }
}

#[test]
fn lists_stay_prose_when_the_setting_is_off() {
    let polisher = RulePolisher::new();
    let context = PolishContext {
        format_lists: false,
        ..context(true)
    };
    let spoken = "We need. First milk, second bread.";
    assert_eq!(polisher.apply(spoken, &context), spoken);
}

#[test]
fn addresses_amounts_and_symbols_compose_with_the_other_stages() {
    let polisher = RulePolisher::new();
    let cases: &[(&PolishContext, &str, &str)] = &[
        (
            &context(true),
            "Send it to john at gmail dot com.",
            "Send it to john@gmail.com.",
        ),
        (
            &context(true),
            "Mail harshav dot shah at outlook dot co dot in",
            "Mail harshav.shah@outlook.co.in",
        ),
        (
            &context(true),
            "Go to google dot com slash docs",
            "Go to google.com/docs",
        ),
        (
            &context(true),
            "Open w w w dot example dot org",
            "Open www.example.org",
        ),
        (&context(true), "It costs five dollars", "It costs $5"),
        (
            &context(true),
            "twenty five dollars and fifty cents",
            "$25.50",
        ),
        (&context(true), "two hundred rupees", "\u{20b9}200"),
        (&context(true), "ten euros", "\u{20ac}10"),
        (
            &context(true),
            "Wrap it in open bracket x close bracket",
            "Wrap it in (x)",
        ),
        (
            &context(true),
            "Um, email john underscore doe at gmail dot com.",
            "Email john_doe@gmail.com.",
        ),
        (
            &context(false),
            "send it to john at sign gmail dot com",
            "Send it to john@gmail.com",
        ),
        (
            &context(true),
            "Tag it hashtag launch and ping at sign harshav.",
            "Tag it #launch and ping @harshav.",
        ),
        (
            &context(true),
            "Use snake underscore case for open square bracket i close square bracket.",
            "Use snake_case for [i].",
        ),
        (
            &context(true),
            "Tom ampersand Jerry, two asterisk three, and forward slash or, C colon backslash temp.",
            "Tom & Jerry, two * three, and/or, C colon\\temp.",
        ),
        (
            &context(true),
            "The plan is. First visit google dot com, second pay ten dollars.",
            "The plan is\n1. visit google.com\n2. pay $10",
        ),
    ];
    for (context, input, expected) in cases {
        assert_eq!(polisher.apply(input, context), *expected, "{input}");
    }
}

#[test]
fn prose_that_looks_like_an_address_or_amount_stays_as_spoken() {
    let polisher = RulePolisher::new();
    let unchanged = [
        "I'm at home.",
        "Meet me at the office.",
        "The dot on the map.",
        "Connect the dots.",
        "A trial period.",
        "Five pounds of flour.",
        "Five apples.",
        "Send it to john@gmail.com.",
        "It costs $5.",
        "Go to google.com.",
        "Visit www.example.org/docs today.",
        "Press the underscore key.",
        "The hashtag is trending.",
        "We closed the dot com era.",
    ];
    for input in unchanged {
        assert_eq!(polisher.apply(input, &context(true)), input, "{input}");
    }
}

#[test]
fn polishing_twice_changes_nothing_more() {
    let polisher = RulePolisher::new();
    let context = context(true);
    let inputs = [
        "Send it to john at gmail dot com.",
        "Mail harshav dot shah at outlook dot co dot in",
        "Go to google dot com slash docs",
        "Open w w w dot example dot org",
        "twenty five dollars and fifty cents",
        "Wrap it in open bracket x close bracket",
        "Use snake underscore case, hashtag launch.",
        "The plan is. First visit google dot com, second pay ten dollars.",
    ];
    for input in inputs {
        let once = polisher.apply(input, &context);
        assert_eq!(polisher.apply(&once, &context), once, "{input}");
    }
}

#[test]
fn symbols_stay_words_when_the_setting_is_off() {
    let polisher = RulePolisher::new();
    let context = PolishContext {
        spoken_symbols: false,
        ..context(true)
    };
    assert_eq!(
        polisher.apply(
            "Use snake underscore case and open bracket x close bracket.",
            &context
        ),
        "Use snake underscore case and open bracket x close bracket."
    );
    assert_eq!(
        polisher.apply("Send it to john at gmail dot com.", &context),
        "Send it to john@gmail.com.",
        "addresses do not depend on the symbol setting"
    );
    assert_eq!(
        polisher.apply("john underscore doe at gmail dot com", &context),
        "john_doe@gmail.com",
        "a spoken underscore inside an address is part of the address"
    );
}

#[test]
fn other_languages_keep_their_words() {
    let polisher = RulePolisher::new();
    let detected = context(true);
    assert_eq!(
        polisher.apply("Er ist um drei Uhr nicht da.", &detected),
        "Er ist um drei Uhr nicht da."
    );
    assert_eq!(
        polisher.apply("Um dia muito bom, obrigado.", &detected),
        "Um dia muito bom, obrigado."
    );
    assert_eq!(
        polisher.apply("Ich habe zwanzig Euro und twenty.", &detected),
        "Ich habe zwanzig Euro und twenty.",
        "number words are written as digits only for a language with number data"
    );
    let german = PolishContext {
        language: Some(Language::from_static("de")),
        ..context(true)
    };
    assert_eq!(polisher.apply("Um, äh, so.", &german), "Um, äh, so.");
    let italian = PolishContext {
        language: Some(Language::from_static("it")),
        ..context(false)
    };
    assert_eq!(polisher.apply("vedo i ragazzi", &italian), "Vedo i ragazzi");
}

#[test]
fn the_dictionary_is_compiled_once_per_pair_list() {
    let polisher = RulePolisher::new();
    let first = polisher.dictionary(&context(true).dictionary);
    let again = polisher.dictionary(&context(true).dictionary);
    assert!(Arc::ptr_eq(&first, &again));
    let changed = polisher.dictionary(&pairs(&[("echo", "Echo")]));
    assert!(!Arc::ptr_eq(&first, &changed));
    assert_eq!(changed.apply("echo"), "Echo");
}

#[test]
fn the_port_is_instant_ready_and_never_fails() {
    let polisher = RulePolisher::new();
    assert_eq!(polisher.caps(), RulePolisher::CAPS);
    assert!(!polisher.caps().needs_model);
    assert!(matches!(poll_once(polisher.prepare()), Poll::Ready(Ok(()))));
    let context = context(true);
    assert!(matches!(
        poll_once(polisher.polish("Um, the the plan.", &context)),
        Poll::Ready(Ok(text)) if text == "The plan."
    ));
}
