/*!
 * SOURCE OF TRUTH KEYWORDS: addresses stage, write_addresses, spoken email, spoken web address, dot com, at gmail, domain ending, www, url path, address chain
 * WHAT:  `write_addresses` writes spoken emails and web addresses with the lexicon's AddressWords: "john at gmail dot
 *        com" → "john@gmail.com", "harshav dot shah at outlook dot co dot in" → "harshav.shah@outlook.co.in",
 *        "google dot com slash docs" → "google.com/docs", "w w w dot example dot org" → "www.example.org".
 * WHY:   Stage 4 of 02 §8.3. "at" and "dot" are everyday words, so a false address is worse than a missed one: words
 *        joined by marks (spoken "dot"/"at"/"slash"/"dash"/"underscore", or written ".", "@", "/", "-", "_") form a
 *        chain, and only a chain whose domain ends in a lexicon ending becomes an address. An email is the chain's
 *        last "at" with a domain after it; otherwise a web address must start at the chain's first word, so "the dot
 *        google dot com" stays whole instead of turning into "the dot google.com". A domain, a one-word mailbox and
 *        a path segment never start with a common word ("look at google dot com" → "look at google.com"). Spelled
 *        letters join into one label ("w w w" → "www"), which is why this runs before repeats: that stage would
 *        collapse "w w w" to "w". Hosts and mailboxes are written lowercase (they ignore case and the engine
 *        capitalizes names like "Gmail"); a path keeps its case. A chain with no spoken mark ("john@gmail.com",
 *        "3.5", "and/or", "well-known") is text the engine already wrote and is copied unchanged. The written
 *        address is re-tokenized, so later stages see exactly what a typed address would give them.
 * WHERE: RulePolisher::apply (rules/mod.rs), after symbols and before repeats, when the take's lexicon has
 *        AddressWords.
 */

use super::{
    lexicon::{AddressMark, AddressWords},
    text::{Token, tokenize},
};

/// `tokens` with every spoken email and web address written out.
pub fn write_addresses(tokens: &[Token], words: &AddressWords) -> Vec<Token> {
    let reader = Reader { tokens, words };
    let mut out = Vec::with_capacity(tokens.len());
    let mut index = 0;
    while index < tokens.len() {
        let Some(chain) = reader.chain(index) else {
            out.push(tokens[index].clone());
            index += 1;
            continue;
        };
        match reader.address(&chain) {
            Some(address) => {
                let first = &chain.labels[address.first];
                let last = &chain.labels[address.last];
                out.extend_from_slice(&tokens[index..first.start]);
                out.extend(tokenize(&reader.write(&chain, &address)));
                index = last.end + 1;
            }
            None => {
                // A chain that is no address is copied whole, so no later word of it starts a partial one.
                let end = chain.labels.last().map_or(index, |label| label.end);
                out.extend_from_slice(&tokens[index..=end]);
                index = end + 1;
            }
        }
    }
    out
}

/// One part of an address: a word, or letters spelled one by one ("w w w" → "www").
#[derive(Debug)]
struct Label {
    text: String,
    /// Index of its first token.
    start: usize,
    /// Index of its last token.
    end: usize,
}

/// A mark between two labels.
#[derive(Debug, Clone, Copy)]
struct Joint {
    mark: AddressMark,
    /// Said as a word or written apart from its labels, so the address still has to be written.
    spoken: bool,
}

/// Labels joined by marks; `joints[i]` sits between `labels[i]` and `labels[i + 1]`.
#[derive(Debug, Default)]
struct Chain {
    labels: Vec<Label>,
    joints: Vec<Joint>,
}

/// The labels of a chain written as one address.
#[derive(Debug)]
struct Address {
    first: usize,
    /// The last label of the domain; labels after it are the path.
    host_last: usize,
    last: usize,
}

struct Reader<'a> {
    tokens: &'a [Token],
    words: &'a AddressWords,
}

impl Reader<'_> {
    /// The chain of labels starting at `index`.
    fn chain(&self, index: usize) -> Option<Chain> {
        let mut chain = Chain::default();
        let mut label = self.label(index)?;
        loop {
            let after = label.end + 1;
            chain.labels.push(label);
            let Some((joint, next)) = self.joint(after) else {
                break;
            };
            let Some(next_label) = self.label(next) else {
                break;
            };
            chain.joints.push(joint);
            label = next_label;
        }
        Some(chain)
    }

    /// The label starting at `index`: a word of letters and digits that is not itself a spoken mark.
    fn label(&self, index: usize) -> Option<Label> {
        let token = self.tokens.get(index)?;
        let plain = token.is_word()
            && token.text.chars().all(char::is_alphanumeric)
            && self.words.mark(&token.folded()).is_none();
        if !plain {
            return None;
        }
        let mut text = token.text.clone();
        let mut end = index;
        if is_letter(token) {
            while self.is_gap(end + 1)
                && let Some(letter) = self.tokens.get(end + 2).filter(|next| is_letter(next))
            {
                text.push_str(&letter.text);
                end += 2;
            }
        }
        Some(Label {
            text,
            start: index,
            end,
        })
    }

    /// The mark at `index` (right after a label) and the index of the label after it.
    fn joint(&self, index: usize) -> Option<(Joint, usize)> {
        let gap_before = self.is_gap(index);
        let at = if gap_before { index + 1 } else { index };
        let token = self.tokens.get(at)?;
        let (mark, said) = if token.is_word() {
            (self.words.mark(&token.folded())?, true)
        } else {
            (token.punct().and_then(AddressMark::of_written)?, false)
        };
        let gap_after = self.is_gap(at + 1);
        let next = if gap_after { at + 2 } else { at + 1 };
        // A spoken mark is its own word; a written one is glued, except "@" and "/", which may stand apart.
        let fits = if said {
            gap_before && gap_after
        } else {
            !(gap_before || gap_after) || mark.may_stand_apart()
        };
        let joint = Joint {
            mark,
            spoken: said || gap_before || gap_after,
        };
        (fits && self.tokens.get(next).is_some_and(Token::is_word)).then_some((joint, next))
    }

    /// The address in `chain`, when it holds one with something left to write.
    fn address(&self, chain: &Chain) -> Option<Address> {
        let address = self.email(chain).or_else(|| self.web(chain, 0))?;
        chain.joints[address.first..address.last]
            .iter()
            .any(|joint| joint.spoken)
            .then_some(address)
    }

    /// The email around the chain's last "at", or its domain alone when the word before "at" is prose.
    fn email(&self, chain: &Chain) -> Option<Address> {
        let at = chain
            .joints
            .iter()
            .rposition(|joint| joint.mark == AddressMark::At)?;
        let host_last = self.domain(chain, at + 1)?;
        let mut first = at;
        while first > 0
            && matches!(
                chain.joints[first - 1].mark,
                AddressMark::Dot | AddressMark::Underscore | AddressMark::Dash
            )
        {
            first -= 1;
        }
        if first == at && self.is_common(&chain.labels[at]) {
            return self.web(chain, at + 1);
        }
        Some(Address {
            first,
            host_last,
            last: host_last,
        })
    }

    /// The web address starting at label `first`: its domain and any path after it.
    fn web(&self, chain: &Chain, first: usize) -> Option<Address> {
        let host_last = self.domain(chain, first)?;
        let mut last = host_last;
        while chain
            .joints
            .get(last)
            .is_some_and(|joint| joint.mark == AddressMark::Slash)
            && chain
                .labels
                .get(last + 1)
                .is_some_and(|label| !self.is_common(label))
        {
            last += 1;
            while chain.joints.get(last).is_some_and(|joint| {
                matches!(
                    joint.mark,
                    AddressMark::Dot | AddressMark::Dash | AddressMark::Underscore
                )
            }) {
                last += 1;
            }
        }
        Some(Address {
            first,
            host_last,
            last,
        })
    }

    /// The last label of the longest domain starting at label `first`, ending in a lexicon ending.
    fn domain(&self, chain: &Chain, first: usize) -> Option<usize> {
        if self.is_common(chain.labels.get(first)?) {
            return None;
        }
        let mut host_last = None;
        let mut index = first;
        while let Some(joint) = chain
            .joints
            .get(index)
            .filter(|joint| matches!(joint.mark, AddressMark::Dot | AddressMark::Dash))
        {
            index += 1;
            if joint.mark == AddressMark::Dot && self.ends_domain(chain, first, index) {
                host_last = Some(index);
            }
        }
        host_last
    }

    /// Label `last` ends a domain that starts at label `first`: alone, or with the label before it ("co.in").
    fn ends_domain(&self, chain: &Chain, first: usize, last: usize) -> bool {
        let folded = |index: usize| chain.labels[index].text.to_lowercase();
        let ending = folded(last);
        if self.words.endings.contains(&ending.as_str()) {
            return true;
        }
        last >= first + 2
            && chain.joints[last - 2].mark == AddressMark::Dot
            && self
                .words
                .paired_endings
                .iter()
                .any(|[second, top]| *second == folded(last - 1) && *top == ending)
    }

    fn is_common(&self, label: &Label) -> bool {
        self.words
            .common_words
            .contains(&label.text.to_lowercase().as_str())
    }

    /// `address` written out: lowercase up to the end of the domain, the path as spoken.
    fn write(&self, chain: &Chain, address: &Address) -> String {
        let mut text = String::new();
        for index in address.first..=address.last {
            if index > address.first {
                text.push(chain.joints[index - 1].mark.written());
            }
            let label = &chain.labels[index].text;
            if index <= address.host_last {
                text.push_str(&label.to_lowercase());
            } else {
                text.push_str(label);
            }
        }
        text
    }

    /// One run of whitespace inside a line.
    fn is_gap(&self, index: usize) -> bool {
        self.tokens
            .get(index)
            .is_some_and(|token| token.is_space() && !token.is_line_break())
    }
}

/// A one-letter word, as spelled out letter by letter.
fn is_letter(token: &Token) -> bool {
    let mut chars = token.text.chars();
    token.is_word() && chars.next().is_some_and(char::is_alphabetic) && chars.next().is_none()
}

#[cfg(test)]
mod tests {
    use super::{
        super::{lexicon::LEXICONS, text::render},
        *,
    };

    fn run(text: &str) -> String {
        let english = LEXICONS[0].addresses.expect("English has address words");
        render(&write_addresses(&tokenize(text), english))
    }

    #[test]
    fn writes_spoken_emails() {
        let cases = [
            (
                "Send it to john at gmail dot com.",
                "Send it to john@gmail.com.",
            ),
            (
                "Mail harshav dot shah at outlook dot co dot in",
                "Mail harshav.shah@outlook.co.in",
            ),
            (
                "Send it to John at Gmail dot com",
                "Send it to john@gmail.com",
            ),
            ("john underscore doe at gmail dot com", "john_doe@gmail.com"),
            ("john_doe at gmail dot com", "john_doe@gmail.com"),
            ("mary dash ann at example dot org", "mary-ann@example.org"),
            ("john at gmail.com", "john@gmail.com"),
            ("ping john @gmail dot com", "ping john@gmail.com"),
            (
                "reach me at john at gmail dot com today",
                "reach me at john@gmail.com today",
            ),
            (
                "Contact us at hello at echo dot app",
                "Contact us at hello@echo.app",
            ),
            ("j s m i t h at gmail dot com", "jsmith@gmail.com"),
            ("john23 at gmail dot com", "john23@gmail.com"),
        ];
        for (input, expected) in cases {
            assert_eq!(run(input), expected, "{input:?}");
        }
    }

    #[test]
    fn writes_spoken_web_addresses() {
        let cases = [
            ("Go to google dot com slash docs", "Go to google.com/docs"),
            ("Open w w w dot example dot org", "Open www.example.org"),
            ("visit docs dot google dot com", "visit docs.google.com"),
            (
                "see example dot com slash Read Me",
                "see example.com/Read Me",
            ),
            (
                "example dot com slash docs slash api",
                "example.com/docs/api",
            ),
            ("cool dash site dot com", "cool-site.com"),
            ("my dash site dot com", "my dash site dot com"),
            ("open google dot com dot au", "open google.com.au"),
            ("go to google dot com/docs", "go to google.com/docs"),
            ("look at google dot com", "look at google.com"),
            ("I work at google dot com", "I work at google.com"),
            ("google dot com at noon", "google.com at noon"),
            ("Use open dot ai.", "Use open.ai."),
        ];
        for (input, expected) in cases {
            assert_eq!(run(input), expected, "{input:?}");
        }
    }

    #[test]
    fn keeps_prose_and_written_addresses() {
        let unchanged = [
            "I'm at home.",
            "Meet me at the office.",
            "The dot on the map.",
            "Connect the dots.",
            "the dot com bubble",
            "the dot google dot com",
            "It is a dot net app",
            "one dot in red",
            "outlook dot co",
            "Put a dot on the i",
            "three dot five",
            "Send it to john@gmail.com.",
            "Go to google.com/docs",
            "It costs $5 at 3.5% and/or more",
            "a well-known e.g. example",
            "Mr. Smith at noon",
            "w w w",
            "google dot\ncom",
            "",
        ];
        for input in unchanged {
            assert_eq!(run(input), input, "{input:?}");
        }
    }
}
