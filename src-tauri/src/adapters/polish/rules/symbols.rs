/*!
 * SOURCE OF TRUTH KEYWORDS: symbols stage, write_symbols, spoken symbols, symbol names, open bracket close bracket, underscore, hashtag, ampersand, at sign, spoken_symbols setting
 * WHAT:  `write_symbols` writes spoken symbol names as their symbols with the lexicon's SymbolWords: "open bracket x
 *        close bracket" → "(x)", "snake underscore case" → "snake_case", "hashtag launch" → "#launch", "Tom
 *        ampersand Jerry" → "Tom & Jerry", "and forward slash or" → "and/or".
 * WHY:   Stage 3 of 02 §8.3, behind `polish.spoken_symbols`. The longest phrase wins, so "open square bracket" is
 *        "[" and never "(" followed by "square". A name after a determiner is the thing itself ("the underscore
 *        key"), so it stays a word. How a symbol joins its neighbours is lexicon data (SymbolJoin): one that joins
 *        both sides glues only when words stand on both sides, so "type underscore." keeps its space instead of
 *        becoming "type_.". Brackets keep their spaces here and the spacing stage pulls them onto their contents.
 *        It runs before the addresses stage, so "john underscore doe at gmail dot com" reaches it as "john_doe".
 * WHERE: RulePolisher::apply (rules/mod.rs), after fillers and before addresses, when the context asks for it and
 *        the take's lexicon has SymbolWords.
 */

use super::{
    lexicon::{SymbolJoin, SymbolWords},
    text::{Token, TokenKind, phrase_end, previous_significant},
};

/// `tokens` with every spoken symbol name written as its symbol.
pub fn write_symbols(tokens: &[Token], words: &SymbolWords) -> Vec<Token> {
    let mut out: Vec<Token> = Vec::with_capacity(tokens.len());
    let mut index = 0;
    while index < tokens.len() {
        let Some((end, symbol, join)) = symbol_at(tokens, index, words, &out) else {
            out.push(tokens[index].clone());
            index += 1;
            continue;
        };
        let word_after = is_gap(tokens.get(end)) && tokens.get(end + 1).is_some_and(Token::is_word);
        let word_before = matches!(
            out.as_slice(),
            [.., before, gap] if before.is_word() && is_gap(Some(gap))
        );
        let glue_before = join == SymbolJoin::Both && word_before && word_after;
        let glue_after = match join {
            SymbolJoin::Spaced => false,
            SymbolJoin::Next => word_after,
            SymbolJoin::Both => glue_before,
        };
        if glue_before {
            out.pop();
        }
        out.push(Token::new(TokenKind::Punct, symbol.to_string()));
        // Skipping the gap after the phrase glues the symbol to the next word.
        index = if glue_after { end + 1 } else { end };
    }
    out
}

/// The end (exclusive), symbol and join of the longest symbol phrase starting at `index`.
fn symbol_at(
    tokens: &[Token],
    index: usize,
    words: &SymbolWords,
    out: &[Token],
) -> Option<(usize, char, SymbolJoin)> {
    // "the underscore key" names the key; in "( a close paren" the "a" is the bracket's content.
    let noun = previous_significant(out, out.len()).is_some_and(|before| {
        let determiner =
            out[before].is_word() && words.nouns_after.contains(&out[before].folded().as_str());
        let bracketed = previous_significant(out, before)
            .and_then(|opening| out[opening].punct())
            .is_some_and(|c| matches!(c, '(' | '[' | '{'));
        determiner && !bracketed
    });
    if noun {
        return None;
    }
    words
        .phrases
        .iter()
        .filter_map(|(phrase, symbol, join)| {
            phrase_end(tokens, index, phrase).map(|end| (end, *symbol, *join))
        })
        .max_by_key(|(end, ..)| *end)
}

/// One run of whitespace inside a line.
fn is_gap(token: Option<&Token>) -> bool {
    token.is_some_and(|token| token.is_space() && !token.is_line_break())
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
        let english = LEXICONS[0].symbols.expect("English has symbol words");
        render(&write_symbols(&tokenize(text), english))
    }

    #[test]
    fn writes_spoken_symbol_names_as_symbols() {
        let cases = [
            (
                "Wrap it in open bracket x close bracket",
                "Wrap it in ( x )",
            ),
            ("open paren a close paren", "( a )"),
            ("open bracket the close bracket", "( the )"),
            ("open parenthesis a close parenthesis", "( a )"),
            ("open square bracket one close square bracket", "[ one ]"),
            ("open curly brace key close curly brace", "{ key }"),
            ("open curly bracket key close curly bracket", "{ key }"),
            ("snake underscore case", "snake_case"),
            ("Snake Underscore Case", "Snake_Case"),
            ("and forward slash or", "and/or"),
            ("C colon backslash temp", "C colon\\temp"),
            ("folder back slash file", "folder\\file"),
            ("hashtag launch day", "#launch day"),
            ("ping at sign harshav", "ping @harshav"),
            ("Tom ampersand Jerry", "Tom & Jerry"),
            ("two asterisk three", "two * three"),
            ("type underscore.", "type _."),
            ("underscore", "_"),
            ("hashtag", "#"),
        ];
        for (input, expected) in cases {
            assert_eq!(run(input), expected, "{input:?}");
        }
    }

    #[test]
    fn keeps_symbol_nouns_and_other_words() {
        let unchanged = [
            "Press the underscore key",
            "Add an asterisk here",
            "The hashtag is trending",
            "A trial period.",
            "Five pounds of flour.",
            "Close the door and open the window",
            "a bracket",
            "open\nbracket",
            "",
        ];
        for input in unchanged {
            assert_eq!(run(input), input, "{input:?}");
        }
    }
}
