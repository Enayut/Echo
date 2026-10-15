/*!
 * SOURCE OF TRUTH KEYWORDS: lists stage, format_lists, spoken list, numbered list, first second third, list markers, list items, dictated enumeration
 * WHAT:  `format_lists` rewrites a spoken enumeration as a numbered list: "The list is as follows. First apple,
 *        second oranges, third mangoes." → "The list is as follows\n1. apple\n2. oranges\n3. mangoes". The lead-in
 *        keeps its own line, and text after the list's last sentence starts a new line.
 * WHY:   Stage 4 of 02 §8.3, behind `polish.format_lists`. Plain rules, no model, so the same words always give the
 *        same list. A wrong list is worse than a missed one, so an enumeration only counts when: its first marker
 *        opens a clause (text start, line break, pause or sentence mark before it), the markers count 1, 2, 3 …
 *        with no gap, each later marker opens a clause or follows a connector ("and fourth"), every item holds
 *        1..=`max_item_words` words, and there are at least `min_items` items. A sentence end closes the list
 *        unless the next item starts right after it, so prose after the list stays prose. That keeps "wait a
 *        second", "at first", "first time I went" and "first, second and third place" as spoken. Markers come from
 *        the lexicon: the NumberWords ordinals, ListWords phrases ("firstly", "first of all") and an item noun with
 *        a number ("number one", "step 2"). It runs before numbers, so "first" is still a word here and an item's
 *        own numbers are written in digits afterwards ("first twenty apples" → "1. 20 apples"); spacing and casing
 *        then treat each item like a line of their own.
 * WHERE: RulePolisher::apply (rules/mod.rs), after repeats and before numbers, when the context asks for it and the
 *        take's lexicon has ListWords.
 */

use std::ops::Range;

use super::{
    lexicon::{ListWords, NumberWords},
    text::{Token, TokenKind, is_dash, is_pause, is_terminal, next_significant, phrase_end},
};

/// `tokens` with every spoken enumeration written as a numbered list.
pub fn format_lists(
    tokens: &[Token],
    words: &ListWords,
    numbers: Option<&NumberWords>,
) -> Vec<Token> {
    let markers = Markers { words, numbers };
    let mut out: Vec<Token> = Vec::with_capacity(tokens.len());
    let mut copied = 0;
    let mut index = 0;
    while index < tokens.len() {
        let Some(list) = markers.list_at(tokens, index) else {
            index += 1;
            continue;
        };
        out.extend_from_slice(&tokens[copied..index]);
        write_list(&mut out, tokens, &list, words.number_mark);
        copied = match next_significant(tokens, list.end) {
            // Text after the list starts its own line.
            Some(rest) if !tokens[list.end..rest].iter().any(Token::is_line_break) => {
                out.push(line_break());
                rest
            }
            _ => list.end,
        };
        index = copied;
    }
    out.extend_from_slice(&tokens[copied..]);
    out
}

/// The take language's list vocabulary.
#[derive(Clone, Copy)]
struct Markers<'a> {
    words: &'a ListWords,
    numbers: Option<&'a NumberWords>,
}

/// A recognised list: each item's content tokens and the index right after the list.
struct List {
    items: Vec<Range<usize>>,
    end: usize,
}

/// Where an item ends.
enum ItemEnd {
    /// The next item follows: this content ends at `content_end`, the next item's content starts at `next_content`.
    Next {
        content_end: usize,
        next_content: usize,
    },
    /// The list ends with this item, at `end` (exclusive).
    Last { end: usize },
}

impl Markers<'_> {
    /// The list whose first marker starts at `start`, when the enumeration there passes every rule.
    fn list_at(&self, tokens: &[Token], start: usize) -> Option<List> {
        if !opens_clause(tokens, start) {
            return None;
        }
        let mut content_start = self.marker_end(tokens, start, 1)?;
        let mut place = 1;
        let mut items = Vec::new();
        let end = loop {
            match self.item_end(tokens, content_start, place + 1) {
                ItemEnd::Next {
                    content_end,
                    next_content,
                } => {
                    items.push(content_start..content_end);
                    content_start = next_content;
                    place += 1;
                }
                ItemEnd::Last { end } => {
                    items.push(content_start..end);
                    break end;
                }
            }
        };
        let fits = |item: &Range<usize>| {
            let words = tokens[item.clone()]
                .iter()
                .filter(|token| token.is_word())
                .count();
            (1..=self.words.max_item_words).contains(&words)
        };
        (items.len() >= self.words.min_items && items.iter().all(fits))
            .then_some(List { items, end })
    }

    /// Where the item whose content starts at `from` ends, `next_place` being the number the next item would have.
    fn item_end(&self, tokens: &[Token], from: usize, next_place: u64) -> ItemEnd {
        let mut index = from;
        while let Some(token) = tokens.get(index) {
            if let Some(next_content) = self.next_item_at(tokens, index, next_place) {
                return ItemEnd::Next {
                    content_end: index,
                    next_content,
                };
            }
            if token.is_line_break() {
                return ItemEnd::Last { end: index };
            }
            if token.punct().is_some_and(is_terminal) {
                let after = index
                    + tokens[index..]
                        .iter()
                        .take_while(|token| token.punct().is_some_and(is_terminal))
                        .count();
                // A sentence end closes the list unless the next item starts right after it.
                match next_significant(tokens, after) {
                    Some(next) if self.next_item_at(tokens, next, next_place).is_some() => {
                        index = next;
                    }
                    _ => return ItemEnd::Last { end: after },
                }
                continue;
            }
            index += 1;
        }
        ItemEnd::Last { end: tokens.len() }
    }

    /// The content start of the item numbered `place` when its marker, or a connector and its marker, starts at
    /// `index`.
    fn next_item_at(&self, tokens: &[Token], index: usize, place: u64) -> Option<usize> {
        if opens_clause(tokens, index)
            && let Some(content) = self.marker_end(tokens, index, place)
        {
            return Some(content);
        }
        let connector = tokens.get(index).filter(|token| token.is_word())?;
        if !self.words.connectors.contains(&connector.folded().as_str()) {
            return None;
        }
        tokens
            .get(index + 1)
            .filter(|token| token.is_space() && !token.is_line_break())?;
        self.marker_end(tokens, index + 2, place)
    }

    /// The end of the marker numbered `place` starting at `index`; a marker glued to more ("first-class") is part
    /// of a word, not a marker.
    fn marker_end(&self, tokens: &[Token], index: usize, place: u64) -> Option<usize> {
        let end = self
            .phrase_marker(tokens, index, place)
            .or_else(|| self.ordinal_marker(tokens, index, place))
            .or_else(|| self.numbered_marker(tokens, index, place))?;
        tokens
            .get(end)
            .is_none_or(|token| token.is_space() || token.punct().is_some_and(is_pause))
            .then_some(end)
    }

    /// The longest ListWords phrase for `place` ("first of all" over "first").
    fn phrase_marker(&self, tokens: &[Token], index: usize, place: u64) -> Option<usize> {
        self.words
            .phrases
            .iter()
            .filter(|(_, at)| *at == place)
            .filter_map(|(phrase, _)| phrase_end(tokens, index, phrase))
            .max()
    }

    /// An ordinal word for `place` ("first", "Second").
    fn ordinal_marker(&self, tokens: &[Token], index: usize, place: u64) -> Option<usize> {
        let word = tokens.get(index).filter(|token| token.is_word())?;
        let value = self
            .numbers?
            .ordinal(&word.folded())
            .map(|(value, _)| value);
        (value == Some(place)).then_some(index + 1)
    }

    /// An item noun and the number `place`, in words or digits ("number one", "step 2").
    fn numbered_marker(&self, tokens: &[Token], index: usize, place: u64) -> Option<usize> {
        let noun = tokens.get(index).filter(|token| token.is_word())?;
        if !self.words.item_nouns.contains(&noun.folded().as_str()) {
            return None;
        }
        tokens
            .get(index + 1)
            .filter(|token| token.is_space() && !token.is_line_break())?;
        let number = tokens.get(index + 2).filter(|token| token.is_word())?;
        let value = number.text.parse::<u64>().ok().or_else(|| {
            self.numbers
                .and_then(|numbers| numbers.cardinal(&number.folded()))
        });
        (value == Some(place)).then_some(index + 3)
    }
}

/// Only whitespace stands between `index` and the text start, a line break, or a pause or sentence mark.
fn opens_clause(tokens: &[Token], index: usize) -> bool {
    tokens[..index.min(tokens.len())]
        .iter()
        .rev()
        .find(|token| !token.is_space() || token.is_line_break())
        .is_none_or(|token| {
            token.is_line_break() || token.punct().is_some_and(|c| is_pause(c) || is_terminal(c))
        })
}

/// Appends `list` to `out`: the lead-in already there ends its line, then one numbered line per item.
fn write_list(out: &mut Vec<Token>, tokens: &[Token], list: &List, mark: char) {
    out.truncate(kept_len(out));
    if !out.is_empty() {
        out.push(line_break());
    }
    for (number, item) in (1_u64..).zip(&list.items) {
        if number > 1 {
            out.push(line_break());
        }
        out.push(Token::new(TokenKind::Word, number.to_string()));
        out.push(Token::new(TokenKind::Punct, mark.to_string()));
        out.push(Token::new(TokenKind::Space, " "));
        let content = &tokens[item.clone()];
        let content = &content[leading_len(content)..];
        out.extend_from_slice(&content[..kept_len(content)]);
    }
}

/// How many tokens open `tokens` before its first word: whitespace and the pause mark said after a marker.
fn leading_len(tokens: &[Token]) -> usize {
    tokens
        .iter()
        .take_while(|token| {
            token.is_space() || token.punct().is_some_and(|c| is_pause(c) || is_dash(c))
        })
        .count()
}

/// How many tokens of `tokens` stay once trailing whitespace, pause marks and one full stop are dropped; an
/// ellipsis, "?" and "!" stay.
fn kept_len(tokens: &[Token]) -> usize {
    let punct_before = |len: usize| {
        len.checked_sub(1)
            .and_then(|last| tokens.get(last))
            .and_then(Token::punct)
    };
    let mut len = tokens.len();
    let mut full_stop_dropped = false;
    while let Some(last) = len.checked_sub(1).and_then(|last| tokens.get(last)) {
        let full_stop =
            !full_stop_dropped && last.punct() == Some('.') && punct_before(len - 1) != Some('.');
        if !(last.is_space() || last.punct().is_some_and(is_pause) || full_stop) {
            break;
        }
        full_stop_dropped |= full_stop;
        len -= 1;
    }
    len
}

fn line_break() -> Token {
    Token::new(TokenKind::Space, "\n")
}

#[cfg(test)]
mod tests {
    use super::{
        super::{
            lexicon::LEXICONS,
            text::{render, tokenize},
        },
        *,
    };

    fn run(text: &str) -> String {
        let english = &LEXICONS[0];
        let words = english.lists.expect("English has list words");
        render(&format_lists(&tokenize(text), words, english.numbers))
    }

    #[test]
    fn spoken_enumerations_become_numbered_lists() {
        let cases = [
            (
                "The list of grocery items are as follows. First apple, second oranges. Third mangoes, Fourth watermelon.",
                "The list of grocery items are as follows\n1. apple\n2. oranges\n3. mangoes\n4. watermelon",
            ),
            ("First, apple. Second, oranges.", "1. apple\n2. oranges"),
            (
                "Buy these: first milk, second bread and third eggs.",
                "Buy these\n1. milk\n2. bread\n3. eggs",
            ),
            (
                "Firstly we plan, secondly we build, thirdly we ship.",
                "1. we plan\n2. we build\n3. we ship",
            ),
            (
                "First of all, book the room. Second, send the invite.",
                "1. book the room\n2. send the invite",
            ),
            ("Number one milk, number two bread.", "1. milk\n2. bread"),
            ("Step 1 open it, step 2 close it", "1. open it\n2. close it"),
            (
                "First apple, second oranges. Then I went home.",
                "1. apple\n2. oranges\nThen I went home.",
            ),
            (
                "Can you get these? First apple, second pears.",
                "Can you get these?\n1. apple\n2. pears",
            ),
            (
                "Plan:\nfirst draft it, second review it",
                "Plan\n1. draft it\n2. review it",
            ),
            (
                "Wait... first this, second that... ok",
                "Wait...\n1. this\n2. that...\nok",
            ),
            (
                "First red, second blue. Also, first cats, second dogs.",
                "1. red\n2. blue\nAlso\n1. cats\n2. dogs",
            ),
        ];
        for (input, expected) in cases {
            assert_eq!(run(input), expected, "{input}");
        }
    }

    #[test]
    fn prose_stays_prose() {
        let cases = [
            "First time I went there it rained.",
            "Wait a second, I need coffee first.",
            "At first, it was hard. Second place went to Ana.",
            "First, second and third place winners.",
            "First apple, third mangoes.",
            "First-class seats, second-class seats.",
            "The first, second and third floors.",
            "First apple. I love apples. Second oranges.",
            "He was number one, then number two.",
            "First, we should finalize the plan for the whole team before the end of the week because the client \
             expects a draft on Monday morning and we still need sign-off from legal and finance. Second, book it.",
            "",
        ];
        for input in cases {
            assert_eq!(run(input), input, "{input}");
        }
    }
}
